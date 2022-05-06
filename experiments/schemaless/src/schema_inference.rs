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
    Int,
    String,
    Float,
    Unknown,
    NonNull(Box<InferredType>),
    List(Box<InferredType>),
    Vertex(Rc<str>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VertexKind {
    Interface,
    Type,
}

#[allow(dead_code)]
#[derive(Debug, Clone, derive_new::new)]
pub(crate) struct InferredVertexType {
    name: Rc<str>,
    kind: VertexKind,

    #[new(default)]
    implements: BTreeSet<Rc<str>>,

    #[new(default)]
    parameters: BTreeMap<Rc<str>, InferredType>,

    #[new(default)]
    fields: BTreeMap<Rc<str>, InferredType>,
}

#[derive(Debug)]
struct InferredSchema {
    root_type: Rc<str>,
    types: BTreeMap<Rc<str>, InferredVertexType>,
    next_anon_type_number: usize,
}

impl InferredSchema {
    const ROOT_TYPE_NAME: &'static str = "RootSchemaQuery";

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
                v.insert(field_type.clone());
            }
            Entry::Occupied(mut occ) => {
                if field_type != InferredType::Unknown && occ.get() != &field_type {
                    // TODO: handle Unknown inside NonNull or List, followed by more successful
                    //       inference from elsewhere -- right now that returns 'diverging' error
                    if occ.get() == &InferredType::Unknown {
                        *occ.get_mut() = field_type.clone();
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

    pub(crate) fn into_schema(self) -> String {
        dbg!(&self);
        todo!()
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

            // TODO: handle field parameters, if any

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
        }
    }

    Ok(())
}
