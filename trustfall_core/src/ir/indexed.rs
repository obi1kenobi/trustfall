use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    ptr,
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::util::BTreeMapTryInsertExt;

use super::{Argument, Eid, IREdge, IRFold, IRQuery, IRQueryComponent, Type, Vid};

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
            &mut vec![],
        )?;

        Ok(Self { ir_query, vids, eids, outputs })
    }
}

fn get_optional_vertices_in_component(component: &Arc<IRQueryComponent>) -> BTreeSet<Vid> {
    let mut output = BTreeSet::new();
    for edge in component.edges.values() {
        if edge.optional || output.contains(&edge.from_vid) {
            output.insert(edge.to_vid);
        }
    }
    output
}

fn get_output_type(
    output_at: Vid,
    field_type: &Type,
    component_optional_vertices: &BTreeSet<Vid>,
    are_folds_optional: &[bool],
) -> Type {
    let mut wrapped_output_type = field_type.clone();
    if component_optional_vertices.contains(&output_at) {
        wrapped_output_type = wrapped_output_type.with_nullability(true);
    }
    for is_fold_optional in are_folds_optional.iter().rev() {
        wrapped_output_type = Type::new_list_type(wrapped_output_type, *is_fold_optional);
    }
    wrapped_output_type
}

fn add_data_from_component(
    vids: &mut BTreeMap<Vid, Arc<IRQueryComponent>>,
    eids: &mut BTreeMap<Eid, EdgeKind>,
    outputs: &mut BTreeMap<Arc<str>, Output>,
    variables: &BTreeMap<Arc<str>, Type>,
    component: &Arc<IRQueryComponent>,
    are_folds_optional: &mut Vec<bool>, // whether each level of @fold is inside an @optional
) -> Result<(), InvalidIRQueryError> {
    let component_optional_vertices = get_optional_vertices_in_component(component);

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
                            if !vref.variable_type.is_scalar_only_subtype(var_type) {
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
        let output_component =
            vids.get(&output_vid).ok_or(InvalidIRQueryError::GetBetterVariant(1))?;
        if !ptr::eq(component.as_ref(), output_component.as_ref()) {
            return Err(InvalidIRQueryError::GetBetterVariant(2));
        }

        let output_name = output_name.clone();
        let output_type = get_output_type(
            output_vid,
            &field.field_type,
            &component_optional_vertices,
            are_folds_optional,
        );
        let output = Output { name: output_name.clone(), value_type: output_type, vid: output_vid };
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
        let from_component =
            vids.get(&edge.from_vid).ok_or(InvalidIRQueryError::GetBetterVariant(5))?;
        if !ptr::eq(component.as_ref(), from_component.as_ref()) {
            return Err(InvalidIRQueryError::GetBetterVariant(6));
        }
        let to_component =
            vids.get(&edge.to_vid).ok_or(InvalidIRQueryError::GetBetterVariant(7))?;
        if !ptr::eq(component.as_ref(), to_component.as_ref()) {
            return Err(InvalidIRQueryError::GetBetterVariant(8));
        }

        let existing = eids.insert(*eid, EdgeKind::Regular(edge.clone()));
        if existing.is_some() {
            return Err(InvalidIRQueryError::GetBetterVariant(9));
        }
    }

    for (eid, fold) in component.folds.iter() {
        // The "to" vertex must have Vid equal to the folded edge's Eid + 1.
        if usize::from(eid.0) + 1 != usize::from(fold.to_vid.0) {
            return Err(InvalidIRQueryError::GetBetterVariant(10));
        }

        // The folded edge's "from" vertex must be from this component.
        let from_component =
            vids.get(&fold.from_vid).ok_or(InvalidIRQueryError::GetBetterVariant(11))?;
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
            let output_type = get_output_type(
                fold.from_vid,
                kind.field_type(),
                &component_optional_vertices,
                are_folds_optional,
            );
            outputs
                .insert_or_error(
                    name.clone(),
                    Output { name: name.clone(), value_type: output_type, vid: fold.to_vid },
                )
                .map_err(|_| InvalidIRQueryError::GetBetterVariant(15))?;
        }

        are_folds_optional.push(component_optional_vertices.contains(&fold.from_vid));
        add_data_from_component(
            vids,
            eids,
            outputs,
            variables,
            &fold.component,
            are_folds_optional,
        )?;
        are_folds_optional.pop().expect("pushed value is no longer present");
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
