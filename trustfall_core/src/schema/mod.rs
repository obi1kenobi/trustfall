#![allow(dead_code)]
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    ops::Add,
    sync::Arc,
};

use async_graphql_parser::{
    parse_schema,
    types::{
        BaseType, DirectiveDefinition, FieldDefinition, ObjectType, SchemaDefinition,
        ServiceDocument, Type, TypeDefinition, TypeKind, TypeSystemDefinition,
    },
    Positioned,
};

pub use ::async_graphql_parser::Error;
use async_graphql_value::Name;
use itertools::Itertools;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::ir::types::{get_base_named_type, is_argument_type_valid, is_scalar_only_subtype};
use crate::util::{BTreeMapTryInsertExt, HashMapTryInsertExt};

use self::error::InvalidSchemaError;

mod adapter;
pub mod error;

pub use adapter::SchemaAdapter;

#[derive(Debug, Clone)]
pub struct Schema {
    pub(crate) schema: SchemaDefinition,
    pub(crate) query_type: ObjectType,
    pub(crate) directives: HashMap<Arc<str>, DirectiveDefinition>,
    pub(crate) scalars: HashMap<Arc<str>, TypeDefinition>,
    pub(crate) vertex_types: HashMap<Arc<str>, TypeDefinition>,
    pub(crate) fields: HashMap<(Arc<str>, Arc<str>), FieldDefinition>,
    pub(crate) field_origins: BTreeMap<(Arc<str>, Arc<str>), FieldOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FieldOrigin {
    SingleAncestor(Arc<str>), // the name of the parent (super) type that first defined this field
    MultipleAncestors(BTreeSet<Arc<str>>),
}

impl Add for &FieldOrigin {
    type Output = FieldOrigin;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (FieldOrigin::SingleAncestor(l), FieldOrigin::SingleAncestor(r)) => {
                if l == r {
                    self.clone()
                } else {
                    FieldOrigin::MultipleAncestors(btreeset![l.clone(), r.clone()])
                }
            }
            (FieldOrigin::SingleAncestor(single), FieldOrigin::MultipleAncestors(multi))
            | (FieldOrigin::MultipleAncestors(multi), FieldOrigin::SingleAncestor(single)) => {
                let mut new_set = multi.clone();
                new_set.insert(single.clone());
                FieldOrigin::MultipleAncestors(new_set)
            }
            (FieldOrigin::MultipleAncestors(l_set), FieldOrigin::MultipleAncestors(r_set)) => {
                let mut new_set = l_set.clone();
                new_set.extend(r_set.iter().cloned());
                FieldOrigin::MultipleAncestors(new_set)
            }
        }
    }
}

pub(crate) static BUILTIN_SCALARS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    hashset! {
        "Int",
        "Float",
        "String",
        "Boolean",
        "ID",
    }
});

const RESERVED_PREFIX: &str = "__";

impl Schema {
    pub const ALL_DIRECTIVE_DEFINITIONS: &'static str = "
directive @filter(op: String!, value: [String!]) repeatable on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD
";

    pub fn parse(input: impl AsRef<str>) -> Result<Self, InvalidSchemaError> {
        let doc = parse_schema(input)?;
        Self::new(doc)
    }

    pub fn new(doc: ServiceDocument) -> Result<Self, InvalidSchemaError> {
        let mut schema: Option<SchemaDefinition> = None;
        let mut directives: HashMap<Arc<str>, DirectiveDefinition> = Default::default();
        let mut scalars: HashMap<Arc<str>, TypeDefinition> = Default::default();

        // The schema is mostly type definitions, except for one schema definition, and
        // perhaps a small number of other definitions like custom scalars or directives.
        let mut vertex_types: HashMap<Arc<str>, TypeDefinition> =
            HashMap::with_capacity(doc.definitions.len() - 1);

        // Each type has probably at least one field.
        let mut fields: HashMap<(Arc<str>, Arc<str>), FieldDefinition> =
            HashMap::with_capacity(doc.definitions.len() - 1);

        for definition in doc.definitions {
            match definition {
                TypeSystemDefinition::Schema(s) => {
                    assert!(schema.is_none());
                    if s.node.extend {
                        unimplemented!();
                    }

                    schema = Some(s.node);
                }
                TypeSystemDefinition::Directive(d) => {
                    directives
                        .insert_or_error(Arc::from(d.node.name.node.to_string()), d.node)
                        .unwrap();
                }
                TypeSystemDefinition::Type(t) => {
                    let node = t.node;
                    let type_name: Arc<str> = Arc::from(node.name.node.to_string());
                    assert!(!BUILTIN_SCALARS.contains(type_name.as_ref()));

                    if node.extend {
                        unimplemented!();
                    }

                    match &node.kind {
                        TypeKind::Scalar => {
                            scalars.insert_or_error(type_name.clone(), node.clone()).unwrap();
                        }
                        TypeKind::Object(_) | TypeKind::Interface(_) => {
                            match vertex_types.insert_or_error(type_name.clone(), node.clone()) {
                                Ok(_) => {}
                                Err(err) => {
                                    let type_or_interface_name = err.entry.key();
                                    return Err(
                                        InvalidSchemaError::DuplicateTypeOrInterfaceDefinition(
                                            type_or_interface_name.to_string(),
                                        ),
                                    );
                                }
                            }
                        }
                        TypeKind::Enum(_) => unimplemented!(),
                        TypeKind::Union(_) => unimplemented!(),
                        TypeKind::InputObject(_) => unimplemented!(),
                    }

                    let field_defs = match node.kind {
                        TypeKind::Object(object) => Some(object.fields),

                        TypeKind::Interface(interface) => Some(interface.fields),
                        _ => None,
                    };
                    if let Some(field_defs) = field_defs {
                        for field in field_defs {
                            let field_node = field.node;
                            let field_name = Arc::from(field_node.name.node.to_string());

                            match fields
                                .insert_or_error((type_name.clone(), field_name), field_node)
                            {
                                Ok(_) => {}
                                Err(err) => {
                                    let (key, value) = err.entry.key();
                                    return Err(InvalidSchemaError::DuplicateFieldDefinition(
                                        key.to_string(),
                                        value.to_string(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        let schema = schema.expect("Schema definition was not present.");
        let query_type_name =
            schema.query.as_ref().expect("No query type was declared in the schema").node.as_ref();
        let query_type_definition = vertex_types
            .get(query_type_name)
            .expect("The query type set in the schema object was never defined.");
        let query_type = match &query_type_definition.kind {
            TypeKind::Object(o) => o.clone(),
            _ => unreachable!(),
        };

        let mut errors = vec![];
        if let Err(e) = check_required_transitive_implementations(&vertex_types) {
            errors.extend(e);
        }
        if let Err(e) = check_field_type_narrowing(&vertex_types, &fields) {
            errors.extend(e);
        }
        if let Err(e) = check_fields_required_by_interface_implementations(&vertex_types, &fields) {
            errors.extend(e);
        }
        if let Err(e) =
            check_type_and_property_and_edge_invariants(query_type_definition, &vertex_types)
        {
            errors.extend(e);
        }
        if let Err(e) =
            check_root_query_type_invariants(query_type_definition, &query_type, &vertex_types)
        {
            errors.extend(e);
        }

        let field_origins = match get_field_origins(&vertex_types) {
            Ok(field_origins) => {
                if let Err(e) = check_ambiguous_field_origins(&fields, &field_origins) {
                    errors.extend(e);
                }
                Some(field_origins)
            }
            Err(e) => {
                errors.push(e);
                None
            }
        };

        if errors.is_empty() {
            Ok(Self {
                schema,
                query_type,
                directives,
                scalars,
                vertex_types,
                fields,
                field_origins: field_origins.expect("no field origins but also no errors"),
            })
        } else {
            Err(errors.into())
        }
    }

    /// If the named type is defined, iterate through the names of its subtypes including itself.
    /// Otherwise, return None.
    pub fn subtypes<'a, 'slf: 'a>(
        &'slf self,
        type_name: &'a str,
    ) -> Option<impl Iterator<Item = &'slf str> + 'a> {
        if !self.vertex_types.contains_key(type_name) {
            return None;
        }

        Some(self.vertex_types.iter().filter_map(move |(name, defn)| {
            if name.as_ref() == type_name
                || get_vertex_type_implements(defn).iter().any(|x| x.node.as_ref() == type_name)
            {
                Some(name.as_ref())
            } else {
                None
            }
        }))
    }

    pub(crate) fn query_type_name(&self) -> &str {
        self.schema.query.as_ref().unwrap().node.as_ref()
    }

    pub(crate) fn vertex_type_implements(&self, vertex_type: &str) -> &[Positioned<Name>] {
        get_vertex_type_implements(&self.vertex_types[vertex_type])
    }

    pub(crate) fn is_subtype(&self, parent_type: &Type, maybe_subtype: &Type) -> bool {
        is_subtype(&self.vertex_types, parent_type, maybe_subtype)
    }

    pub(crate) fn is_named_type_subtype(&self, parent_type: &str, maybe_subtype: &str) -> bool {
        is_named_type_subtype(&self.vertex_types, parent_type, maybe_subtype)
    }
}

fn check_root_query_type_invariants(
    query_type_definition: &TypeDefinition,
    query_type: &ObjectType,
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
) -> Result<(), Vec<InvalidSchemaError>> {
    let mut errors: Vec<InvalidSchemaError> = vec![];

    for field_defn in &query_type.fields {
        let field_type = &field_defn.node.ty.node;
        let base_named_type = get_base_named_type(field_type);
        if BUILTIN_SCALARS.contains(base_named_type) {
            errors.push(InvalidSchemaError::PropertyFieldOnRootQueryType(
                query_type_definition.name.node.to_string(),
                field_defn.node.name.node.to_string(),
                field_type.to_string(),
            ));
        } else if !vertex_types.contains_key(base_named_type) {
            // Somehow the base named type is neither a vertex nor a scalar,
            // and this field is neither an edge nor a property.
            unreachable!()
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_type_and_property_and_edge_invariants(
    query_type_definition: &TypeDefinition,
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
) -> Result<(), Vec<InvalidSchemaError>> {
    let mut errors: Vec<InvalidSchemaError> = vec![];

    for (type_name, type_defn) in vertex_types {
        if type_name.as_ref().starts_with(RESERVED_PREFIX) {
            errors.push(InvalidSchemaError::ReservedTypeName(type_name.to_string()));
        }

        let type_fields = get_vertex_type_fields(type_defn);

        for defn in type_fields {
            let field_defn = &defn.node;
            let field_type = &field_defn.ty.node;

            if field_defn.name.node.as_ref().starts_with(RESERVED_PREFIX) {
                errors.push(InvalidSchemaError::ReservedFieldName(
                    type_name.to_string(),
                    field_defn.name.node.to_string(),
                ));
            }

            let base_named_type = get_base_named_type(field_type);
            if BUILTIN_SCALARS.contains(base_named_type) {
                // We're looking at a property field.
                if !field_defn.arguments.is_empty() {
                    errors.push(InvalidSchemaError::PropertyFieldWithParameters(
                        type_name.to_string(),
                        field_defn.name.node.to_string(),
                        field_type.to_string(),
                        field_defn.arguments.iter().map(|x| x.node.name.node.to_string()).collect(),
                    ));
                }
            } else if vertex_types.contains_key(base_named_type) {
                // We're looking at an edge field.
                if base_named_type == query_type_definition.name.node.as_ref() {
                    // This edge points to the root query type. That's not supported.
                    errors.push(InvalidSchemaError::EdgePointsToRootQueryType(
                        type_name.to_string(),
                        field_defn.name.node.to_string(),
                        field_type.to_string(),
                    ));
                } else {
                    // Check if the parameters this edge accepts (if any) have valid default values.
                    for param_defn in &field_defn.arguments {
                        if let Some(value) = &param_defn.node.default_value {
                            let param_type = &param_defn.node.ty.node;
                            match value.node.clone().try_into() {
                                Ok(value) => {
                                    if !is_argument_type_valid(param_type, &value) {
                                        errors.push(InvalidSchemaError::InvalidDefaultValueForFieldParameter(
                                            type_name.to_string(),
                                            field_defn.name.node.to_string(),
                                            param_defn.node.name.node.to_string(),
                                            param_type.to_string(),
                                            format!("{value:?}"),
                                        ));
                                    }
                                }
                                Err(_) => {
                                    errors.push(
                                        InvalidSchemaError::InvalidDefaultValueForFieldParameter(
                                            type_name.to_string(),
                                            field_defn.name.node.to_string(),
                                            param_defn.node.name.node.to_string(),
                                            param_type.to_string(),
                                            value.node.to_string(),
                                        ),
                                    );
                                }
                            }
                        }
                    }

                    // Check that the edge field doesn't have
                    // a list-of-list or more nested list type.
                    match &field_type.base {
                        BaseType::Named(_) => {}
                        BaseType::List(inner) => match &inner.base {
                            BaseType::Named(_) => {}
                            BaseType::List(_) => {
                                errors.push(InvalidSchemaError::InvalidEdgeType(
                                    type_name.to_string(),
                                    field_defn.name.node.to_string(),
                                    field_type.to_string(),
                                ));
                            }
                        },
                    }
                }
            } else {
                // Somehow the base named type is neither a vertex nor a scalar,
                // and this field is neither an edge nor a property.
                unreachable!(
                    "field {} (type {}) appears to represent neither an edge nor a property",
                    field_defn.name.node.as_ref(),
                    field_type.to_string(),
                )
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn is_named_type_subtype(
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
    parent_type: &str,
    maybe_subtype: &str,
) -> bool {
    let parent_is_vertex = vertex_types.contains_key(parent_type);
    let maybe_sub = vertex_types.get(maybe_subtype);

    match (parent_is_vertex, maybe_sub) {
        (false, None) => {
            // The types could both be scalars, which have no inheritance hierarchy.
            // Any type is a subtype of itself, so we check equality.
            parent_type == maybe_subtype
        }
        (true, Some(maybe_subtype_vertex)) => {
            // Both types are vertex types. We have a subtype relationship if
            // - the two types are actually the same type, or if
            // - the "maybe subtype" implements the parent type.
            parent_type == maybe_subtype
                || get_vertex_type_implements(maybe_subtype_vertex)
                    .iter()
                    .any(|pos| pos.node.as_ref() == parent_type)
        }
        _ => {
            // One type is a vertex type, the other should be a scalar.
            // No subtype relationship is possible between them.
            false
        }
    }
}

fn is_subtype(
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
    parent_type: &Type,
    maybe_subtype: &Type,
) -> bool {
    // If the parent type is non-nullable, all its subtypes must be non-nullable as well.
    // If the parent type is nullable, it can have both nullable and non-nullable subtypes.
    if !parent_type.nullable && maybe_subtype.nullable {
        return false;
    }

    match (&parent_type.base, &maybe_subtype.base) {
        (BaseType::Named(parent), BaseType::Named(subtype)) => {
            is_named_type_subtype(vertex_types, parent.as_ref(), subtype.as_ref())
        }
        (BaseType::List(parent_type), BaseType::List(maybe_subtype)) => {
            is_subtype(vertex_types, parent_type, maybe_subtype)
        }
        (BaseType::Named(..), BaseType::List(..)) | (BaseType::List(..), BaseType::Named(..)) => {
            false
        }
    }
}

fn check_ambiguous_field_origins(
    fields: &HashMap<(Arc<str>, Arc<str>), FieldDefinition>,
    field_origins: &BTreeMap<(Arc<str>, Arc<str>), FieldOrigin>,
) -> Result<(), Vec<InvalidSchemaError>> {
    let mut errors = vec![];

    for (key, origin) in field_origins {
        let (type_name, field_name) = key;
        if let FieldOrigin::MultipleAncestors(ancestors) = &origin {
            let field_type = fields[key].ty.node.to_string();
            errors.push(InvalidSchemaError::AmbiguousFieldOrigin(
                type_name.to_string(),
                field_name.to_string(),
                field_type,
                ancestors.iter().map(|x| x.to_string()).collect(),
            ))
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check the `implements` portion of the type definitions.
///
/// Checked invariants:
/// - Implemented types must be defined in the schema.
/// - Implemented types must be interfaces.
/// - If type X implements interface A, and A implements interface B,
///   then X must also implement B by transitivity.
fn check_required_transitive_implementations(
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
) -> Result<(), Vec<InvalidSchemaError>> {
    let mut errors: Vec<InvalidSchemaError> = vec![];

    for (type_name, type_defn) in vertex_types {
        let implementations: BTreeSet<&str> =
            get_vertex_type_implements(type_defn).iter().map(|x| x.node.as_ref()).collect();

        // Check the `implements` portion of the type definition.
        for implements_type in implementations.iter().copied() {
            match vertex_types.get(implements_type) {
                Some(implementation_defn) => {
                    if !matches!(implementation_defn.kind, TypeKind::Interface(..)) {
                        errors.push(InvalidSchemaError::ImplementingNonInterface(
                            type_name.to_string(),
                            implements_type.to_string(),
                        ));
                    } else {
                        for expected_impl in get_vertex_type_implements(implementation_defn) {
                            let expected_impl_name = expected_impl.node.as_ref();

                            // Ignore situations with an immediate cycle here
                            // (`expected_impl_name != type_name`) since we have a dedicated
                            // check for those elsewhere.
                            if expected_impl_name != type_name.as_ref()
                                && implementations.get(expected_impl_name).is_none()
                            {
                                errors.push(
                                    InvalidSchemaError::MissingTransitiveInterfaceImplementation(
                                        type_name.to_string(),
                                        implements_type.to_string(),
                                        expected_impl_name.to_string(),
                                    ),
                                );
                            }
                        }
                    }
                }
                None => {
                    errors.push(InvalidSchemaError::ImplementingNonExistentType(
                        type_name.to_string(),
                        implements_type.to_string(),
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_fields_required_by_interface_implementations(
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
    fields: &HashMap<(Arc<str>, Arc<str>), FieldDefinition>,
) -> Result<(), Vec<InvalidSchemaError>> {
    let mut errors: Vec<InvalidSchemaError> = vec![];

    for (type_name, type_defn) in vertex_types {
        let implementations = get_vertex_type_implements(type_defn);

        for implementation in implementations {
            let implementation = implementation.node.as_ref();
            let Some(impl_defn) = vertex_types.get(implementation) else {
                continue;
            };

            for field in get_vertex_type_fields(impl_defn) {
                let field_name = field.node.name.node.as_ref();

                // If the current type does not contain the implemented interface's field,
                // that's an error.
                if !fields.contains_key(&(type_name.clone(), Arc::from(field_name))) {
                    errors.push(InvalidSchemaError::MissingRequiredField(
                        type_name.to_string(),
                        implementation.to_string(),
                        field_name.to_string(),
                        field.node.ty.node.to_string(),
                    ))
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_field_type_narrowing(
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
    fields: &HashMap<(Arc<str>, Arc<str>), FieldDefinition>,
) -> Result<(), Vec<InvalidSchemaError>> {
    let mut errors: Vec<InvalidSchemaError> = vec![];

    for (type_name, type_defn) in vertex_types {
        let implementations = get_vertex_type_implements(type_defn);
        let type_fields = get_vertex_type_fields(type_defn);

        for field in type_fields {
            let field_name = field.node.name.node.as_ref();
            let field_type = &field.node.ty.node;
            let field_parameters: BTreeMap<_, _> = field
                .node
                .arguments
                .iter()
                .map(|arg| (arg.node.name.node.as_ref(), &arg.node.ty.node))
                .collect();

            for implementation in implementations {
                let implementation = implementation.node.as_ref();

                // The parent type might not contain this field. But if it does,
                // ensure that the parent field's type is a supertype of the current field's type.
                if let Some(parent_field) =
                    fields.get(&(Arc::from(implementation), Arc::from(field_name)))
                {
                    let parent_field_type = &parent_field.ty.node;
                    if !is_subtype(vertex_types, parent_field_type, field_type) {
                        errors.push(InvalidSchemaError::InvalidTypeWideningOfInheritedField(
                            field_name.to_string(),
                            type_name.to_string(),
                            implementation.to_string(),
                            field_type.to_string(),
                            parent_field_type.to_string(),
                        ));
                    }

                    let parent_field_parameters: BTreeMap<_, _> = parent_field
                        .arguments
                        .iter()
                        .map(|arg| (arg.node.name.node.as_ref(), &arg.node.ty.node))
                        .collect();

                    // Check for field parameters that the parent type requires but
                    // the child type does not accept.
                    let missing_parameters = parent_field_parameters
                        .keys()
                        .copied()
                        .filter(|name| !field_parameters.contains_key(*name))
                        .collect_vec();
                    if !missing_parameters.is_empty() {
                        errors.push(InvalidSchemaError::InheritedFieldMissingParameters(
                            field_name.to_owned(),
                            type_name.to_string(),
                            implementation.to_owned(),
                            missing_parameters.into_iter().map(ToOwned::to_owned).collect_vec(),
                        ));
                    }

                    // Check for field parameters that the parent type does not accept,
                    // but the child type defines anyway.
                    let unexpected_parameters = field_parameters
                        .keys()
                        .copied()
                        .filter(|name| !parent_field_parameters.contains_key(*name))
                        .collect_vec();
                    if !unexpected_parameters.is_empty() {
                        errors.push(InvalidSchemaError::InheritedFieldUnexpectedParameters(
                            field_name.to_owned(),
                            type_name.to_string(),
                            implementation.to_owned(),
                            unexpected_parameters.into_iter().map(ToOwned::to_owned).collect_vec(),
                        ));
                    }

                    // Check that all field parameters defined by the child have types
                    // that are legal widenings of the corresponding field parameter's type
                    // on the parent type. Field parameters are contravariant, hence widenings.
                    for (&field_parameter, &field_type) in &field_parameters {
                        if let Some(&parent_field_type) =
                            parent_field_parameters.get(field_parameter)
                        {
                            if !is_scalar_only_subtype(field_type, parent_field_type) {
                                errors.push(InvalidSchemaError::InvalidTypeNarrowingOfInheritedFieldParameter(
                                    field_name.to_owned(),
                                    type_name.to_string(),
                                    implementation.to_owned(),
                                    field_parameter.to_string(),
                                    field_type.to_string(),
                                    parent_field_type.to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn get_vertex_type_fields(vertex: &TypeDefinition) -> &[Positioned<FieldDefinition>] {
    match &vertex.kind {
        TypeKind::Object(obj) => &obj.fields,
        TypeKind::Interface(iface) => &iface.fields,
        _ => unreachable!(),
    }
}

fn get_vertex_type_implements(vertex: &TypeDefinition) -> &[Positioned<Name>] {
    match &vertex.kind {
        TypeKind::Object(obj) => &obj.implements,
        TypeKind::Interface(iface) => &iface.implements,
        _ => unreachable!(),
    }
}

#[allow(clippy::type_complexity)]
fn get_field_origins(
    vertex_types: &HashMap<Arc<str>, TypeDefinition>,
) -> Result<BTreeMap<(Arc<str>, Arc<str>), FieldOrigin>, InvalidSchemaError> {
    let mut field_origins: BTreeMap<(Arc<str>, Arc<str>), FieldOrigin> = Default::default();
    let mut queue = VecDeque::new();

    // for each type, which types have yet to have their field origins resolved first
    let mut required_resolutions: BTreeMap<&str, BTreeSet<&str>> = vertex_types
        .iter()
        .map(|(name, defn)| {
            let resolutions: BTreeSet<&str> = get_vertex_type_implements(defn)
                .iter()
                .map(|x| x.node.as_ref())
                .filter(|name| vertex_types.contains_key(*name)) // ignore undefined types
                .collect();
            if resolutions.is_empty() {
                queue.push_back(name);
            }
            (name.as_ref(), resolutions)
        })
        .collect();

    // for each type, which types does it enable resolution of
    let resolvers: BTreeMap<&str, BTreeSet<Arc<str>>> = vertex_types
        .iter()
        .flat_map(|(name, defn)| {
            get_vertex_type_implements(defn)
                .iter()
                .map(|x| (x.node.as_ref(), Arc::from(name.as_ref())))
        })
        .fold(Default::default(), |mut acc, (interface, implementer)| {
            match acc.entry(interface) {
                Entry::Vacant(v) => {
                    v.insert(btreeset![implementer]);
                }
                Entry::Occupied(occ) => {
                    occ.into_mut().insert(implementer);
                }
            }
            acc
        });

    while let Some(type_name) = queue.pop_front() {
        let defn = &vertex_types[type_name];
        let implements = get_vertex_type_implements(defn);
        let fields = get_vertex_type_fields(defn);

        let mut implemented_fields: BTreeMap<&str, FieldOrigin> = Default::default();
        for implemented_interface in implements {
            let implemented_interface = implemented_interface.node.as_ref();
            let Some(implemented_defn) = vertex_types.get(implemented_interface) else {
                continue;
            };
            let parent_fields = get_vertex_type_fields(implemented_defn);
            for field in parent_fields {
                let parent_field_origin = &field_origins
                    [&(Arc::from(implemented_interface), Arc::from(field.node.name.node.as_ref()))];

                implemented_fields
                    .entry(field.node.name.node.as_ref())
                    .and_modify(|origin| *origin = (origin as &FieldOrigin) + parent_field_origin)
                    .or_insert_with(|| parent_field_origin.clone());
            }
        }

        for field in fields {
            let field = &field.node;
            let field_name = &field.name.node;

            let origin = implemented_fields
                .remove(field_name.as_ref())
                .unwrap_or_else(|| FieldOrigin::SingleAncestor(type_name.clone()));
            field_origins
                .insert_or_error((type_name.clone(), Arc::from(field_name.as_ref())), origin)
                .unwrap();
        }

        if let Some(next_types) = resolvers.get(type_name.as_ref()) {
            for next_type in next_types.iter() {
                let remaining = required_resolutions.get_mut(next_type.as_ref()).unwrap();
                if remaining.remove(type_name.as_ref()) && remaining.is_empty() {
                    queue.push_back(next_type);
                }
            }
        }
    }

    for (required, mut remaining) in required_resolutions.into_iter() {
        if !remaining.is_empty() {
            remaining.insert(required);
            let circular_implementations =
                remaining.into_iter().map(|x| x.to_string()).collect_vec();
            return Err(InvalidSchemaError::CircularImplementsRelationships(
                circular_implementations,
            ));
        }
    }

    Ok(field_origins)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use async_graphql_parser::parse_schema;
    use itertools::Itertools;
    use trustfall_filetests_macros::parameterize;

    use super::{error::InvalidSchemaError, Schema};

    #[parameterize("trustfall_core/test_data/tests/schema_errors", "*.graphql")]
    fn schema_errors(base: &Path, stem: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{stem}.graphql"));

        let input_data = fs::read_to_string(input_path).unwrap();

        let mut error_path = PathBuf::from(base);
        error_path.push(format!("{stem}.schema-error.ron"));
        let error_data = fs::read_to_string(error_path).unwrap();
        let expected_error: InvalidSchemaError = ron::from_str(&error_data).unwrap();

        let schema_doc = parse_schema(input_data).unwrap();

        match Schema::new(schema_doc) {
            Err(e) => {
                assert_eq!(e, expected_error);
            }
            Ok(_) => panic!("Expected an error but got valid schema."),
        }
    }

    #[parameterize("trustfall_core/test_data/tests/valid_schemas", "*.graphql")]
    fn valid_schemas(base: &Path, stem: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{stem}.graphql"));

        let input_data = fs::read_to_string(input_path).unwrap();

        // Ensure all test schemas contain the directive definitions this module promises are valid.
        assert!(input_data.contains(Schema::ALL_DIRECTIVE_DEFINITIONS));

        match Schema::parse(input_data) {
            Ok(_) => {}
            Err(e) => {
                panic!("{}", e);
            }
        }
    }

    #[test]
    fn schema_subtypes() {
        let input_data = include_str!("../../test_data/schemas/numbers.graphql");
        let schema = Schema::parse(input_data).expect("valid schema");

        assert!(schema.subtypes("Nonexistent").is_none());

        let composite_subtypes = schema.subtypes("Composite").unwrap().collect_vec();
        assert_eq!(vec!["Composite"], composite_subtypes);

        let mut number_subtypes = schema.subtypes("Number").unwrap().collect_vec();
        number_subtypes.sort_unstable();
        assert_eq!(vec!["Composite", "Neither", "Number", "Prime"], number_subtypes);
    }
}
