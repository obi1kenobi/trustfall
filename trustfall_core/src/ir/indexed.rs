use std::{collections::BTreeMap, convert::TryFrom, ptr, sync::Arc};

use async_graphql_parser::types::{BaseType, Type};
use serde::{Deserialize, Serialize};

use crate::util::BTreeMapTryInsertExt;

use super::{
    types::is_scalar_only_subtype, Argument, Eid, IREdge, IRFold, IRQuery, IRQueryComponent, Vid,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedQuery {
    pub ir_query: IRQuery,

    pub vids: BTreeMap<Vid, Arc<IRQueryComponent>>,

    pub eids: BTreeMap<Eid, EdgeKind>,

    pub outputs: BTreeMap<Arc<str>, Output>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub name: Arc<str>,

    #[serde(serialize_with = "crate::ir::serialization::serde_type_serializer")]
    #[serde(deserialize_with = "crate::ir::serialization::serde_type_deserializer")]
    pub value_type: Type,

    pub vid: Vid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvalidIRQueryError {
    GetBetterVariant(i32),
}

impl TryFrom<IRQuery> for IndexedQuery {
    type Error = InvalidIRQueryError;

    fn try_from(ir_query: IRQuery) -> Result<Self, Self::Error> {
        // desired property:
        // - queries may be safely executed with edges expanded in increasing Eid order,
        //   such that once a component has begun executing, it is fully completed before
        //   execution moves back to its parent component (if any)
        //
        // invariants sufficient for the above:
        // - the edge with Eid i points to vertex with Vid i+1
        // - in every component, every vertex is used in an edge in that component
        // - non-fold edges use vertices from the same component
        // - fold edges use vertices where the "to" vertex is in the fold component
        //   which is contained in the "from" vertex
        // - every edge is expanded such that the vids increase in the from->to direction
        // - fold edges have a lower Eid than any of the edges within their component
        // - for any component, the Eids of the edges within it and its (recursive) subcomponents
        //   form an interval [low, high] where the edge that begins the component (if any) has
        //   Eid of low-1, and every Eid in that interval is occupied by some edge in
        //   the component or one of its (recursive) subcomponents
        // - vertices containing tagged values are always expanded into before the tag is used
        //   (i.e. the edge with the tagged value vertex as its "to" side has a lower Eid than
        //    the edge with the filtering vertex as its "to" side)
        // TODO: most of the above
        let mut vids = Default::default();
        let mut eids = Default::default();
        let mut outputs = Default::default();

        add_data_from_component(
            &mut vids,
            &mut eids,
            &mut outputs,
            &ir_query.variables,
            &ir_query.root_component,
            0,
        )?;

        Ok(Self {
            ir_query,
            vids,
            eids,
            outputs,
        })
    }
}

fn add_data_from_component(
    vids: &mut BTreeMap<Vid, Arc<IRQueryComponent>>,
    eids: &mut BTreeMap<Eid, EdgeKind>,
    outputs: &mut BTreeMap<Arc<str>, Output>,
    variables: &BTreeMap<Arc<str>, Type>,
    component: &Arc<IRQueryComponent>,
    fold_depth: usize,
) -> Result<(), InvalidIRQueryError> {
    // the root vertex Vid must belong to an existing vertex in the component
    if component.vertices.get(&component.root).is_none() {
        return Err(InvalidIRQueryError::GetBetterVariant(-1));
    }

    for (vid, vertex) in &component.vertices {
        let existing = vids.insert(*vid, component.clone());
        if existing.is_some() {
            return Err(InvalidIRQueryError::GetBetterVariant(0));
        }

        for filter in &vertex.filters {
            match filter.right() {
                Some(Argument::Variable(vref)) => {
                    match variables.get(&vref.variable_name) {
                        Some(var_type) => {
                            // The variable type at top level must be a subtype of (or same type as)
                            // the type recorded at the point of use of the variable. It can be
                            // a subtype if another point of use has narrowed the type:
                            // for example, if the other point of use requires it to be non-null
                            // but this point of use allows a nullable value.
                            //
                            // If the variable type at top level is not a subtype of the type here,
                            // this query is not valid.
                            if !is_scalar_only_subtype(&vref.variable_type, var_type) {
                                return Err(InvalidIRQueryError::GetBetterVariant(-2));
                            }
                        }
                        None => {
                            // This variable is used in the query but never recorded at
                            // the top level of the query. This query is invalid.
                            return Err(InvalidIRQueryError::GetBetterVariant(-3));
                        }
                    }
                }
                Some(Argument::Tag(..)) | None => {}
            }
        }
    }

    for (output_name, field) in component.outputs.iter() {
        let output_vid = field.vertex_id;

        // the output must be from a vertex in this component
        let output_component = vids
            .get(&output_vid)
            .ok_or(InvalidIRQueryError::GetBetterVariant(1))?;
        if !ptr::eq(component.as_ref(), output_component.as_ref()) {
            return Err(InvalidIRQueryError::GetBetterVariant(2));
        }

        let output_type = if fold_depth == 0 {
            field.field_type.clone()
        } else {
            let mut wrapped_output_type = field.field_type.clone();
            for _ in 0..fold_depth {
                wrapped_output_type = Type {
                    base: BaseType::List(Box::new(wrapped_output_type)),
                    nullable: false,
                };
            }
            wrapped_output_type
        };

        let output_name = output_name.clone();
        let output = Output {
            name: output_name.clone(),
            value_type: output_type,
            vid: output_vid,
        };
        let existing = outputs.insert(output_name, output);
        if existing.is_some() {
            return Err(InvalidIRQueryError::GetBetterVariant(3));
        }
    }

    for (eid, edge) in component.edges.iter() {
        // the "to" vertex must have Vid equal to the edge's Eid + 1
        if usize::from(eid.0) + 1 != usize::from(edge.to_vid.0) {
            return Err(InvalidIRQueryError::GetBetterVariant(4));
        }

        // the edge's endpoints must be vertices from this component
        let from_component = vids
            .get(&edge.from_vid)
            .ok_or(InvalidIRQueryError::GetBetterVariant(5))?;
        if !ptr::eq(component.as_ref(), from_component.as_ref()) {
            return Err(InvalidIRQueryError::GetBetterVariant(6));
        }
        let to_component = vids
            .get(&edge.to_vid)
            .ok_or(InvalidIRQueryError::GetBetterVariant(7))?;
        if !ptr::eq(component.as_ref(), to_component.as_ref()) {
            return Err(InvalidIRQueryError::GetBetterVariant(8));
        }

        let existing = eids.insert(*eid, EdgeKind::Regular(edge.clone()));
        if existing.is_some() {
            return Err(InvalidIRQueryError::GetBetterVariant(9));
        }
    }

    let new_fold_depth = fold_depth + 1;
    for (eid, fold) in component.folds.iter() {
        // The "to" vertex must have Vid equal to the folded edge's Eid + 1.
        if usize::from(eid.0) + 1 != usize::from(fold.to_vid.0) {
            return Err(InvalidIRQueryError::GetBetterVariant(10));
        }

        // The folded edge's "from" vertex must be from this component.
        let from_component = vids
            .get(&fold.from_vid)
            .ok_or(InvalidIRQueryError::GetBetterVariant(11))?;
        if !ptr::eq(component.as_ref(), from_component.as_ref()) {
            return Err(InvalidIRQueryError::GetBetterVariant(12));
        }

        // The folded edge's "to" vertex must be the root of the fold component.
        if fold.to_vid != fold.component.root {
            return Err(InvalidIRQueryError::GetBetterVariant(13));
        }

        let existing = eids.insert(*eid, EdgeKind::Fold(fold.clone()));
        if existing.is_some() {
            return Err(InvalidIRQueryError::GetBetterVariant(14));
        }

        // Include fold-specific outputs in the list of outputs.
        for (name, kind) in &fold.fold_specific_outputs {
            outputs
                .insert_or_error(
                    name.clone(),
                    Output {
                        name: name.clone(),
                        value_type: kind.field_type().clone(),
                        vid: fold.to_vid,
                    },
                )
                .map_err(|_| InvalidIRQueryError::GetBetterVariant(15))?;
        }

        add_data_from_component(
            vids,
            eids,
            outputs,
            variables,
            &fold.component,
            new_fold_depth,
        )?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    Regular(Arc<IREdge>),
    Fold(Arc<IRFold>),
}

impl From<Arc<IREdge>> for EdgeKind {
    fn from(edge: Arc<IREdge>) -> Self {
        Self::Regular(edge)
    }
}

impl From<Arc<IRFold>> for EdgeKind {
    fn from(fold: Arc<IRFold>) -> Self {
        Self::Fold(fold)
    }
}
