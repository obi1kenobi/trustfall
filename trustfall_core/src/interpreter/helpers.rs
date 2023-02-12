use std::fmt::Debug;

use crate::ir::FieldValue;

use super::basic_adapter::{ContextIterator, ContextOutcomeIterator, VertexIterator};

/// Helper for implementing [`BasicAdapter::resolve_property`] and equivalents.
///
/// Takes a property-resolver function and applies it over each of the vertices
/// in the input context iterator, one at a time.
pub fn resolve_property_with<'vertex, Vertex: Debug + Clone + 'vertex>(
    contexts: ContextIterator<'vertex, Vertex>,
    mut resolver: impl FnMut(&Vertex) -> FieldValue + 'static,
) -> ContextOutcomeIterator<'vertex, Vertex, FieldValue> {
    Box::new(contexts.map(move |ctx| match ctx.current_token.as_ref() {
        None => (ctx, FieldValue::Null),
        Some(vertex) => {
            let value = resolver(vertex);
            (ctx, value)
        }
    }))
}

/// Helper for implementing [`BasicAdapter::resolve_neighbors`] and equivalents.
///
/// Takes a neighbor-resolver function and applies it over each of the vertices
/// in the input context iterator, one at a time.
pub fn resolve_neighbors_with<'vertex, Vertex: Debug + Clone + 'vertex>(
    contexts: ContextIterator<'vertex, Vertex>,
    mut resolver: impl FnMut(&Vertex) -> VertexIterator<'vertex, Vertex> + 'static,
) -> ContextOutcomeIterator<'vertex, Vertex, VertexIterator<'vertex, Vertex>> {
    Box::new(contexts.map(move |ctx| {
        match ctx.current_token.as_ref() {
            None => {
                // rustc needs a bit of help with the type inference here,
                // due to the Box<dyn Iterator> conversion.
                let no_neighbors: VertexIterator<'vertex, Vertex> = Box::new(std::iter::empty());
                (ctx, no_neighbors)
            }
            Some(vertex) => {
                let neighbors = resolver(vertex);
                (ctx, neighbors)
            }
        }
    }))
}

/// Helper for implementing [`BasicAdapter::resolve_coercion`] and equivalents.
///
/// Takes a coercion-resolver function and applies it over each of the vertices
/// in the input context iterator, one at a time.
pub fn resolve_coercion_with<'vertex, Vertex: Debug + Clone + 'vertex>(
    contexts: ContextIterator<'vertex, Vertex>,
    mut resolver: impl FnMut(&Vertex) -> bool + 'static,
) -> ContextOutcomeIterator<'vertex, Vertex, bool> {
    Box::new(contexts.map(move |ctx| match ctx.current_token.as_ref() {
        None => (ctx, false),
        Some(vertex) => {
            let can_coerce = resolver(vertex);
            (ctx, can_coerce)
        }
    }))
}

/// Helper for making property resolver functions based on fields.
#[macro_export]
macro_rules! field_property {
    // If the data is a field directly on the vertex type.
    ($field:ident) => {
        |vertex| -> FieldValue {
            (&vertex.$field).into()
        }
    };
    // If we need to call a fallible conversion method
    // (such as `fn as_foo() -> Option<&Foo>`) before getting the field.
    ($conversion:ident, $field:ident) => {
        |vertex| -> FieldValue {
            let vertex = vertex.$conversion().expect("conversion failed");
            (&vertex.$field).into()
        }
    };
    // Supply a block to post-process the field's value.
    // Use the field's name inside the block.
    ($conversion:ident, $field:ident, $b:block) => {
        |vertex| -> FieldValue {
            let $field = &(vertex.$conversion().expect("conversion failed").$field);
            $b
        }
    };
}

/// Helper for making property resolver functions based on accessor methods.
#[macro_export]
macro_rules! accessor_property {
    // If the data is available as an accessor method on the vertex type.
    ($accessor:ident) => {
        |vertex| -> FieldValue {
            (&vertex.$accessor()).into()
        }
    };
    // If we need to call a fallible conversion method
    // (such as `fn as_foo() -> Option<&Foo>`) before using the accessor.
    ($conversion:ident, $accessor:ident) => {
        |vertex| -> FieldValue {
            let vertex = vertex.$conversion().expect("conversion failed");
            (&vertex.$accessor()).into()
        }
    };
    // Supply a block to post-process the field's value.
    // The accessor's value is assigned to a variable with the same name as the accessor,
    // and is available as such inside the block.
    ($conversion:ident, $accessor:ident, $b:block) => {
        |vertex| -> FieldValue {
            let $accessor = vertex.$conversion().expect("conversion failed").$accessor();
            $b
        }
    };
}
