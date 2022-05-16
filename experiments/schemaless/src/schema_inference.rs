use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    rc::Rc,
};

use async_graphql_parser::types::{
    DocumentOperations, InlineFragment, OperationType, Selection, SelectionSet,
};
use async_graphql_value::Value;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InferredType {
    Boolean,
    Int,
    String,
    Float,
    Unknown,
    NonNull(Box<InferredType>),
    List(Box<InferredType>),
    Vertex(Rc<str>),
}

impl InferredType {
    pub(crate) fn to_graphql_type(&self, unknown_type_standin: &str) -> String {
        match self {
            InferredType::Boolean => "Boolean".to_string(),
            InferredType::Int => "Int".to_string(),
            InferredType::String => "String".to_string(),
            InferredType::Float => "Float".to_string(),
            InferredType::Unknown => unknown_type_standin.to_string(),
            InferredType::NonNull(inner) => {
                let inner_ty = inner.to_graphql_type(unknown_type_standin);
                format!("{inner_ty}!")
            }
            InferredType::List(inner) => {
                let inner_ty = inner.to_graphql_type(unknown_type_standin);
                format!("[{inner_ty}]")
            }
            InferredType::Vertex(inner) => inner.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VertexKind {
    Interface,
    Type,
}

#[derive(Debug, Clone, PartialEq, Eq, derive_new::new)]
pub(crate) struct InferredField {
    ty: InferredType,

    #[new(default)]
    parameters: BTreeMap<Rc<str>, InferredType>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, derive_new::new)]
pub(crate) struct InferredVertexType {
    name: Rc<str>,
    kind: VertexKind,

    #[new(default)]
    implements: BTreeSet<Rc<str>>,

    #[new(default)]
    fields: BTreeMap<Rc<str>, InferredField>,
}

#[derive(Debug)]
struct InferredSchema {
    root_type: Rc<str>,
    types: BTreeMap<Rc<str>, InferredVertexType>,
    next_anon_type_number: usize,
}

impl InferredSchema {
    const ROOT_TYPE_NAME: &'static str = "RootSchemaQuery";
    const TYPE_CHOICE_FOR_FREE_TYPES: &'static str = "String";
    const ANONYMOUS_FIELD_NAME: &'static str = "_AnonField";

    pub(crate) fn new() -> Self {
        let root_type: Rc<str> = Rc::from(String::from(Self::ROOT_TYPE_NAME));
        Self {
            root_type: root_type.clone(),
            types: btreemap! {
                root_type.clone() => InferredVertexType::new(root_type, VertexKind::Type),
            },
            next_anon_type_number: 1,
        }
    }

    pub(crate) fn ensure_type_exists(&mut self, type_name: Rc<str>) {
        self.types
            .entry(type_name.clone())
            .or_insert_with(|| InferredVertexType::new(type_name, VertexKind::Type));
    }

    pub(crate) fn add_new_anon_type(&mut self) -> Rc<str> {
        let anon_type: Rc<str> = Rc::from(format!("_AnonType{}", self.next_anon_type_number));
        self.next_anon_type_number = self
            .next_anon_type_number
            .checked_add(1)
            .expect("wow! how big is your query?!");

        let existing = self.types.insert(
            anon_type.clone(),
            InferredVertexType::new(anon_type.clone(), VertexKind::Type),
        );
        assert!(
            existing.is_none(),
            "unexpected type name conflict: {}",
            anon_type
        );

        anon_type
    }

    pub(crate) fn ensure_vertex_kind_is_interface(&mut self, type_name: &Rc<str>) {
        self.types
            .get_mut(type_name)
            .expect("vertex type was never added")
            .kind = VertexKind::Interface;
    }

    pub(crate) fn ensure_vertex_type_implements(
        &mut self,
        implementor: &Rc<str>,
        implemented_interface: &Rc<str>,
    ) {
        self.ensure_vertex_kind_is_interface(implemented_interface);

        self.types
            .get_mut(implementor)
            .expect("vertex type was never added")
            .implements
            .insert(implemented_interface.clone());
    }

    pub(crate) fn ensure_field_exists(
        &mut self,
        type_name: Rc<str>,
        field_name: Rc<str>,
        field_type: InferredType,
    ) -> Result<InferredType, String> {
        match self
            .types
            .entry(type_name.clone())
            .or_insert_with(|| InferredVertexType::new(type_name.clone(), VertexKind::Type))
            .fields
            .entry(field_name.clone())
        {
            Entry::Vacant(v) => {
                v.insert(InferredField::new(field_type.clone()));
            }
            Entry::Occupied(mut occ) => {
                if field_type != InferredType::Unknown && occ.get().ty != field_type {
                    // TODO: handle Unknown inside NonNull or List, followed by more successful
                    //       inference from elsewhere -- right now that returns 'diverging' error
                    if occ.get().ty == InferredType::Unknown {
                        occ.get_mut().ty = field_type.clone();
                    } else {
                        return Err(format!(
                            "diverging inferred types for type {} field {}: {:?} vs {:?}",
                            type_name,
                            field_name,
                            occ.get(),
                            field_type,
                        ));
                    }
                }
            }
        };

        Ok(field_type)
    }

    pub(crate) fn ensure_field_has_parameter(
        &mut self,
        type_name: &Rc<str>,
        field_name: &Rc<str>,
        parameter_name: Rc<str>,
        parameter_type: InferredType,
    ) -> Result<(), String> {
        match self
            .types
            .get_mut(type_name)
            .expect("vertex type was never added")
            .fields
            .get_mut(field_name)
            .expect("field was never added")
            .parameters
            .entry(parameter_name.clone())
        {
            Entry::Vacant(v) => {
                v.insert(parameter_type);
            }
            Entry::Occupied(mut occ) => {
                if parameter_type != InferredType::Unknown && occ.get() != &parameter_type {
                    // TODO: handle Unknown inside NonNull or List, followed by more successful
                    //       inference from elsewhere -- right now that returns 'diverging' error
                    if occ.get() == &InferredType::Unknown {
                        *occ.get_mut() = parameter_type;
                    } else {
                        return Err(format!(
                            "diverging inferred types for type {} field {} parameter {}: {:?} vs {:?}",
                            type_name,
                            field_name,
                            parameter_name,
                            occ.get(),
                            parameter_type,
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) fn into_schema(self) -> String {
        let root_type = self.root_type.as_ref();
        let mut components: Vec<String> = Vec::with_capacity(1000);
        components.push(format!(
            r"schema {{
    query: {root_type}
}}

directive @filter(op: String!, value: [String!]) on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD

"
        ));

        for (type_name, type_info) in self.types.iter() {
            let is_interface = self
                .types
                .iter()
                .filter_map(|(k, v)| if k != type_name { Some(v) } else { None })
                .flat_map(|v| v.implements.iter())
                .any(|implemented| implemented == type_name);
            let type_kind = if is_interface { "interface" } else { "type" };

            assert!(!type_info.implements.contains(type_name.as_ref()));
            let mut implements: Vec<_> = type_info.implements.iter().map(|x| x.as_ref()).collect();
            implements.sort_unstable();
            let implemented = if implements.is_empty() {
                String::new()
            } else {
                let mut buffer = String::from("implements ");
                buffer.push_str(implements.join(" & ").as_str());
                buffer
            };

            components.push(format!("{type_kind} {type_name} {implemented} {{\n"));

            if type_info.fields.is_empty() {
                // GraphQL schemas do not allow types or interfaces to have no fields.
                // Add a synthetic "anonymous" field instead.
                let field_name = Self::ANONYMOUS_FIELD_NAME;
                let field_type = Self::TYPE_CHOICE_FOR_FREE_TYPES;
                components.push(format!("  {field_name}: {field_type}\n"));
            } else {
                for (field_name, field_def) in type_info.fields.iter() {
                    let field_ty = field_def
                        .ty
                        .to_graphql_type(Self::TYPE_CHOICE_FOR_FREE_TYPES);
                    let parameters = if field_def.parameters.is_empty() {
                        String::new()
                    } else {
                        let parameter_components: Vec<_> = field_def
                            .parameters
                            .iter()
                            .map(|(name, ty)| {
                                let ty = ty.to_graphql_type(Self::TYPE_CHOICE_FOR_FREE_TYPES);
                                format!("{name}: {ty}")
                            })
                            .collect();

                        let all_parameters = parameter_components.join(", ");
                        format!("({all_parameters})")
                    };
                    components.push(format!("  {field_name}{parameters}: {field_ty}\n"))
                }
            }

            components.push("}\n\n".to_string());
        }

        components.concat()
    }
}

pub(crate) fn infer_schema_from_query(query: &str) -> Result<String, String> {
    let maybe_doc = async_graphql_parser::parse_query(query);
    let doc = match maybe_doc {
        Ok(d) => d,
        Err(e) => return Err(e.to_string()),
    };

    if !doc.fragments.is_empty() {
        return Err("defining top-level fragments is not supported".into());
    }

    let operation = match &doc.operations {
        DocumentOperations::Single(s) => &s.node,
        DocumentOperations::Multiple(_) => {
            return Err("found multiple operations in GraphQL string, this is not supported".into())
        }
    };

    if operation.ty != OperationType::Query {
        return Err("GraphQL string is not a query".into());
    }
    if !operation.directives.is_empty() {
        return Err("directives at top level are not supported".into());
    }
    if !operation.variable_definitions.is_empty() {
        return Err("explicit variable definitions at top level are not supported".into());
    }

    let selection_set = &operation.selection_set.node;
    let mut inferred = InferredSchema::new();
    let starting_type = inferred.root_type.clone();

    recurse_into_selection_set(&mut inferred, starting_type, selection_set)?;

    Ok(inferred.into_schema())
}

fn recurse_into_selection_set(
    inferred: &mut InferredSchema,
    current_type: Rc<str>,
    selection_set: &SelectionSet,
) -> Result<(), String> {
    let mut inner_fields = vec![];
    let mut inline_fragment: Option<&InlineFragment> = None;

    for selection in &selection_set.items {
        let selection = &selection.node;
        match selection {
            Selection::Field(f) => {
                inner_fields.push(&f.node);
            }
            Selection::InlineFragment(f) => match inline_fragment {
                None => inline_fragment = Some(&f.node),
                Some(_) => {
                    return Err(
                        "illegal query: contains sibling inline fragments in the same scope".into(),
                    );
                }
            },
            Selection::FragmentSpread(_) => return Err("fragment spreads are not supported".into()),
        }
    }

    if inline_fragment.is_some() && !inner_fields.is_empty() {
        return Err(
            "illegal query: contains type coercion and fields as siblings in the same scope".into(),
        );
    } else if let Some(fragment) = inline_fragment {
        if !fragment.directives.is_empty() {
            return Err("illegal query: contains directives applied to an inline fragment".into());
        }

        let coerce_to: Option<Rc<str>> = fragment
            .type_condition
            .as_ref()
            .map(|tc| Rc::from(tc.node.on.node.to_string()));

        let coerced_type = if let Some(coerce_to) = coerce_to {
            inferred.ensure_type_exists(coerce_to.clone());
            inferred.ensure_vertex_type_implements(&coerce_to, &current_type);

            coerce_to
        } else {
            current_type
        };

        // then, recurse into the selection set
        recurse_into_selection_set(inferred, coerced_type, &fragment.selection_set.node)?;
    } else {
        assert!(!inner_fields.is_empty());
        let property_only_directives = btreeset! {
            "output", "filter",
        };
        let edge_only_directives = btreeset! {
            "optional", "recurse", "fold",
        };
        for field in inner_fields {
            let field_name: Rc<str> = Rc::from(field.name.node.to_string());

            // if possible, figure out if this field is an edge (i.e. vertex-typed) or a property:
            // - @output and @filter directives appear only on properties
            // - @optional, @recurse, @fold directives appear only on vertices
            // - only vertices have non-empty selection sets
            let has_non_empty_selection_set = !field.selection_set.node.items.is_empty();
            let has_only_property_directives = !field.directives.is_empty()
                && field
                    .directives
                    .iter()
                    .all(|d| property_only_directives.contains(d.node.name.node.as_ref()));
            let has_only_edge_directives = !field.directives.is_empty()
                && field
                    .directives
                    .iter()
                    .all(|d| edge_only_directives.contains(d.node.name.node.as_ref()));
            let has_both_property_and_edge_directives = field
                .directives
                .iter()
                .any(|d| property_only_directives.contains(d.node.name.node.as_ref()))
                && field
                    .directives
                    .iter()
                    .any(|d| edge_only_directives.contains(d.node.name.node.as_ref()));

            if has_both_property_and_edge_directives {
                return Err("illegal query: found property-only directive on field that seems to be an edge".into());
            } else if has_non_empty_selection_set || has_only_edge_directives {
                // found an edge
                let inferred_type_name = if field
                    .directives
                    .iter()
                    .any(|d| d.node.name.node.as_ref() == "recurse")
                {
                    // Found an edge that is being recursed,
                    // assume for simplicity that it points to the same type we came from.
                    // This is almost always true in practice, although it doesn't necessarily
                    // have to hold.
                    current_type.clone()
                } else {
                    // We can't know the exact type of the destination vertex, so make
                    // a new type with a generated name.
                    inferred.add_new_anon_type()
                };
                let inferred_type =
                    InferredType::List(Box::new(InferredType::Vertex(inferred_type_name.clone())));

                // ensure the field exists and record its inferred type
                inferred.ensure_field_exists(
                    current_type.clone(),
                    field_name.clone(),
                    inferred_type,
                )?;

                // recurse into the selection set
                recurse_into_selection_set(
                    inferred,
                    inferred_type_name,
                    &field.selection_set.node,
                )?;
            } else if has_only_property_directives {
                // found a property
                let filter_operators: BTreeSet<_> = field
                    .directives
                    .iter()
                    .filter_map(|d| {
                        if d.node.name.node.as_ref() == "filter" {
                            d.node.get_argument("op").and_then(|p| match &p.node {
                                Value::String(s) => Some(s.as_ref()),
                                _ => None,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                // TODO: look at filter parameters to determine a more precise type

                let string_only_operators = btreeset! {
                    "has_prefix",
                    "not_has_prefix",
                    "has_suffix",
                    "not_has_suffix",
                    "has_substring",
                    "not_has_substring",
                    "regex",
                    "not_regex",
                };
                let list_only_operators = btreeset! {
                    "contains",
                    "not_contains",
                };
                let non_null_only_operators = btreeset! {
                    "is_not_null",
                };

                if !list_only_operators.is_disjoint(&filter_operators)
                    && !string_only_operators.is_disjoint(&filter_operators)
                {
                    return Err("invalid query: same property field is filtered as both a string and a list".into());
                }
                let mut inferred_type = if !string_only_operators.is_disjoint(&filter_operators) {
                    InferredType::String
                } else {
                    InferredType::Unknown
                };

                if !list_only_operators.is_disjoint(&filter_operators) {
                    inferred_type = InferredType::List(Box::new(inferred_type));
                }
                if !non_null_only_operators.is_disjoint(&filter_operators) {
                    inferred_type = InferredType::NonNull(Box::new(inferred_type));
                }

                // ensure the field exists and record its inferred type
                inferred.ensure_field_exists(
                    current_type.clone(),
                    field_name.clone(),
                    inferred_type,
                )?;
            } else {
                // unable to determine the type, assume it's a property of unknown type
                // since that will almost always be correct
                let inferred_type = InferredType::Unknown;

                // ensure the field exists and record its inferred type
                inferred.ensure_field_exists(
                    current_type.clone(),
                    field_name.clone(),
                    inferred_type,
                )?;
            };

            // Add any parameters this field might take.
            for (name, value) in &field.arguments {
                let parameter_name = name.node.as_ref();
                let parameter_type = infer_type_for_value(&value.node)?;
                inferred.ensure_field_has_parameter(
                    &current_type,
                    &field_name,
                    Rc::from(parameter_name.to_string()),
                    parameter_type,
                )?;
            }
        }
    }

    Ok(())
}

fn infer_type_for_value(value: &Value) -> Result<InferredType, String> {
    Ok(match value {
        Value::Number(num) => if num.is_f64() {
            InferredType::Float
        } else {
            InferredType::Int
        }
        Value::String(_) => InferredType::String,
        Value::Boolean(_) => InferredType::Boolean,
        Value::List(l) => {
            let inferred_subtypes = l.iter().map(infer_type_for_value).collect::<Result<Vec<_>,_>>()?;

            let mut known_candidate_types = inferred_subtypes.into_iter().filter(|v| v != &InferredType::Unknown);
            let inner_type = match known_candidate_types.next() {
                Some(candidate_type) => {
                    for other_candidate in known_candidate_types {
                        if candidate_type != other_candidate {
                            return Err("found diverging types within the same list value, unable to infer a valid type for the list".into());
                        }
                    }
                    candidate_type
                }
                None => {
                    // The list either has no values or has only values whose types
                    // we weren't able to infer.
                    InferredType::Unknown
                }
            };

            InferredType::List(Box::new(inner_type))
        }
        Value::Null | Value::Binary(_) => InferredType::Unknown,
        Value::Variable(_) |
        Value::Enum(_) |
        Value::Object(_) => {
            return Err("invalid query: enums, input objects, and explicitly-defined query variables are not supported as field arguments".into())
        }
    })
}
