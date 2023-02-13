use std::fmt::Debug;

use crate::ir::FieldValue;

use super::{ContextIterator, ContextOutcomeIterator, VertexIterator};

/// Helper for implementing [`BasicAdapter::resolve_property`] and equivalents.
///
/// Takes a property-resolver function and applies it over each of the vertices
/// in the input context iterator, one at a time.
///
/// Often used with resolvers from the [`field_property!`](crate::field_property) and
/// [`accessor_property!`](crate::accessor_property) macros.
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
///
/// Generally used with [`resolve_property_with`].
///
/// Retrieves a [`FieldValue`] from a vertex by converting it to the proper type,
/// and then retrieving the field of a struct.
///
/// If the property is computed by a function, use
/// [`accessor_property!`](crate::accessor_property) instead.
///
/// # Examples
/// ```
/// # use std::rc::Rc;
/// # use trustfall_core::{
/// #     field_property,
/// #     interpreter::{
/// #         ContextIterator,
/// #         ContextOutcomeIterator,
/// #         helpers::resolve_property_with,
/// #     },
/// #     ir::FieldValue,
/// # };
/// #[derive(Debug, Clone)]
/// struct User {
///     id: String
///     // ...
/// }
///
/// // In implementation of `BasicAdapter`
/// fn resolve_property(
///     // &mut self,
///     contexts: ContextIterator<'static, User>,
///     type_name: &str,
///     property_name: &str,
/// ) -> ContextOutcomeIterator<'static, User, FieldValue> {
///     match (type_name, property_name) {
///         ("User", "id") => {
///             resolve_property_with(contexts, field_property!(id)) // Macro used here
///         },
///         // ...
///         _ => unreachable!()
///     }
/// }
/// ```
///
/// Sometimes a vertex may have to be converted to another type before the
/// property can be accessed. To do this, simply pass a conversion method
/// implemented on the `Vertex` type (in this case `as_user`) to the macro like
/// in the example below.
/// ```
/// # use std::rc::Rc;
/// # use trustfall_core::{
/// #     field_property,
/// #     interpreter::{
/// #         ContextIterator,
/// #         ContextOutcomeIterator,
/// #         helpers::resolve_property_with,
/// #     },
/// #     ir::FieldValue,
/// # };
/// #[derive(Debug, Clone)]
/// struct User {
///     id: String,
///     // ...
/// }
///
/// #[derive(Debug, Clone)]
/// struct Bot {
///     user: User,
///     purpose: String,
/// }
///
/// #[derive(Debug, Clone)]
/// enum Vertex {
///     UserVertex(Rc<User>),
///     BotVertex(Rc<Bot>),
///     // ...
/// }
///
/// impl Vertex {
///     pub fn as_user(&self) -> Option<&User> {
///         match self {
///             Vertex::UserVertex(u) => Some(u.as_ref()),
///             Vertex::BotVertex(b) => Some(&b.user),
///             _ => None,
///         }
///     }
///     // ...
/// }
///
/// // In implementation of `BasicAdapter`
/// # fn resolve_property(
/// #    // &mut self,
/// #    contexts: ContextIterator<'static, Vertex>,
/// #    type_name: &str,
/// #    property_name: &str,
/// # ) -> ContextOutcomeIterator<'static, Vertex, FieldValue> {
/// #    match (type_name, property_name) {
/// ("User" | "Bot", "id") => {
///     resolve_property_with(contexts, field_property!(as_user, id)) // Macro used here
/// },
/// #        // ...
/// #        _ => unreachable!()
/// #    }
/// # }
/// ```
///
/// It is also possible to pass a code block to additionally handle the
/// property.
#[macro_export]
macro_rules! field_property {
    // If the data is a field directly on the vertex type.
    ($field:ident) => {
        |vertex| -> FieldValue { (&vertex.$field).into() }
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
///
/// In principle exactly the same as [`field_property!`](crate::field_property),
/// but where the property is to be accessed using an accessor function instead
/// of as a field.
///
/// # Examples
///
/// In the following example, `name` would be accessed using a field, but the
/// age is accessed using a function:
/// ```rust
/// # use std::rc::Rc;
/// # use trustfall_core::{
/// #     accessor_property,
/// #     field_property,
/// #     interpreter::{
/// #         ContextIterator,
/// #         ContextOutcomeIterator,
/// #         helpers::resolve_property_with,
/// #     },
/// #     ir::FieldValue,
/// # };
/// #[derive(Debug, Clone)]
/// struct User {
///     id: String
///     // ...
/// }
///
/// impl User {
///     pub fn age(&self) -> u8 {
///         // Some calculation
///         # let age = 69;
///         age
///     }
/// }
///
/// // In implementation of `BasicAdapter`
/// fn resolve_property(
///     // &mut self,
///     contexts: ContextIterator<'static, User>,
///     type_name: &str,
///     property_name: &str,
/// ) -> ContextOutcomeIterator<'static, User, FieldValue> {
///     match (type_name, property_name) {
///         ("User", "id") => resolve_property_with(contexts, field_property!(id)),
///         ("User", "age") => resolve_property_with(contexts, accessor_property!(age)),
///         // ...
///         _ => unreachable!()
///     }
/// }
/// ```
///
/// The usage of conversion functions and possible extra processing with a code
/// block is analogous to the ones used with
/// [`field_property!`](crate::field_property).
#[macro_export]
macro_rules! accessor_property {
    // If the data is available as an accessor method on the vertex type.
    ($accessor:ident) => {
        |vertex| -> FieldValue { (&vertex.$accessor()).into() }
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
