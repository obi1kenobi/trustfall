use std::{collections::BTreeSet, fmt::Debug};

use crate::{ir::FieldValue, schema::Schema};

use super::{ContextIterator, ContextOutcomeIterator, Typename, VertexIterator};

/// Helper for implementing [`BasicAdapter::resolve_property`] and equivalents.
///
/// Takes a property-resolver function and applies it over each of the vertices
/// in the input context iterator, one at a time.
///
/// Often used with resolvers from the [`field_property!`](crate::field_property) and
/// [`accessor_property!`](crate::accessor_property) macros.
///
/// [`BasicAdapter::resolve_property`]: super::basic_adapter::BasicAdapter::resolve_property
pub fn resolve_property_with<'vertex, Vertex: Debug + Clone + 'vertex>(
    contexts: ContextIterator<'vertex, Vertex>,
    mut resolver: impl FnMut(&Vertex) -> FieldValue + 'vertex,
) -> ContextOutcomeIterator<'vertex, Vertex, FieldValue> {
    Box::new(contexts.map(move |ctx| match ctx.active_vertex.as_ref() {
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
///
/// [`BasicAdapter::resolve_neighbors`]: super::basic_adapter::BasicAdapter::resolve_neighbors
pub fn resolve_neighbors_with<'vertex, Vertex: Debug + Clone + 'vertex>(
    contexts: ContextIterator<'vertex, Vertex>,
    mut resolver: impl FnMut(&Vertex) -> VertexIterator<'vertex, Vertex> + 'vertex,
) -> ContextOutcomeIterator<'vertex, Vertex, VertexIterator<'vertex, Vertex>> {
    Box::new(contexts.map(move |ctx| {
        match ctx.active_vertex.as_ref() {
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
///
/// [`BasicAdapter::resolve_coercion`]: super::basic_adapter::BasicAdapter::resolve_coercion
pub fn resolve_coercion_with<'vertex, Vertex: Debug + Clone + 'vertex>(
    contexts: ContextIterator<'vertex, Vertex>,
    mut resolver: impl FnMut(&Vertex) -> bool + 'vertex,
) -> ContextOutcomeIterator<'vertex, Vertex, bool> {
    Box::new(contexts.map(move |ctx| match ctx.active_vertex.as_ref() {
        None => (ctx, false),
        Some(vertex) => {
            let can_coerce = resolver(vertex);
            (ctx, can_coerce)
        }
    }))
}

/// Helper for implementing [`BasicAdapter::resolve_coercion`] and equivalents.
///
/// Uses the schema to look up all the subtypes of the coercion target type.
/// Then uses the [`Typename`] trait to look up the exact runtime type of each vertex
/// and checks if it's equal or a subtype of the coercion target type.
///
/// [`BasicAdapter::resolve_coercion`]: super::basic_adapter::BasicAdapter::resolve_coercion
pub fn resolve_coercion_using_schema<'vertex, Vertex: Debug + Clone + Typename + 'vertex>(
    contexts: ContextIterator<'vertex, Vertex>,
    schema: &'vertex Schema,
    coerce_to_type: &str,
) -> ContextOutcomeIterator<'vertex, Vertex, bool> {
    // If the vertex's typename is one of these types,
    // then the coercion's result is `true`.
    let subtypes: BTreeSet<_> = schema
        .subtypes(coerce_to_type)
        .unwrap_or_else(|| panic!("type {coerce_to_type} is not part of this schema"))
        .collect();

    Box::new(contexts.map(move |ctx| match ctx.active_vertex.as_ref() {
        None => (ctx, false),
        Some(vertex) => {
            let typename = vertex.typename();
            let can_coerce = subtypes.contains(typename);
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
        |vertex| -> $crate::ir::value::FieldValue { vertex.$field.clone().into() }
    };
    // If we need to call a fallible conversion method
    // (such as `fn as_foo() -> Option<&Foo>`) before getting the field.
    ($conversion:ident, $field:ident) => {
        |vertex| -> $crate::ir::value::FieldValue {
            let vertex = vertex.$conversion().expect("conversion failed");
            vertex.$field.clone().into()
        }
    };
    // Supply a block to post-process the field's value.
    // Use the field's name inside the block.
    ($conversion:ident, $field:ident, $b:block) => {
        |vertex| -> $crate::ir::value::FieldValue {
            let $field = &vertex.$conversion().expect("conversion failed").$field;
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
        |vertex| -> $crate::ir::value::FieldValue { vertex.$accessor().clone().into() }
    };
    // If we need to call a fallible conversion method
    // (such as `fn as_foo() -> Option<&Foo>`) before using the accessor.
    ($conversion:ident, $accessor:ident) => {
        |vertex| -> $crate::ir::value::FieldValue {
            let vertex = vertex.$conversion().expect("conversion failed");
            vertex.$accessor().clone().into()
        }
    };
    // Supply a block to post-process the field's value.
    // The accessor's value is assigned to a variable with the same name as the accessor,
    // and is available as such inside the block.
    ($conversion:ident, $accessor:ident, $b:block) => {
        |vertex| -> $crate::ir::value::FieldValue {
            let $accessor = vertex.$conversion().expect("conversion failed").$accessor();
            $b
        }
    };
}

/// Resolver for the `__typename` property that optimizes resolution based on the schema.
///
/// Example:
/// ```rust
/// # use std::fmt::Debug;
/// #
/// # use trustfall_core::schema::Schema;
/// # use trustfall_core::ir::FieldValue;
/// # use trustfall_core::interpreter::{
/// #     ContextIterator, ContextOutcomeIterator, helpers::{resolve_typename}, Typename,
/// # };
/// #
/// # #[derive(Debug, Clone)]
/// # enum Vertex {
/// #     Variant,
/// # }
/// #
/// # impl Typename for Vertex {
/// #     fn typename(&self) -> &'static str {
/// #         "variant"
/// #     }
/// # }
/// #
/// # struct Adapter<'vertex> {
/// #     _marker: std::marker::PhantomData<&'vertex Vertex>,
/// # }
/// #
/// # impl<'vertex> Adapter<'vertex> {
/// // Inside your `Adapter` or `BasicAdapter` implementation.
/// fn resolve_property(
///     // &mut self,
///     contexts: ContextIterator<'vertex, Vertex>,
///     type_name: &str,
///     property_name: &str,
///     // < other args >
/// ) -> ContextOutcomeIterator<'vertex, Vertex, FieldValue> {
///     if property_name == "__typename" {
/// #       #[allow(non_snake_case)]
/// #       let SCHEMA = Schema::parse("< imagine this is schema text >").expect("valid schema");
///         return resolve_typename(contexts, &SCHEMA, type_name);
///     }
///
///     // Resolve all other properties here.
/// #   todo!()
/// }
/// # }
/// ```
///
/// This resolver uses the schema to check whether the type named by `type_name` has any subtypes.
/// If so, then each vertex must be resolved dynamically since it may be any of those subtypes.
/// Otherwise, the type must be exactly the value given in `type_name`, and we can take
/// a faster path.
///
/// [`Adapter::resolve_property`]: super::Adapter::resolve_property
pub fn resolve_typename<'a, Vertex: Typename + Debug + Clone + 'a>(
    contexts: ContextIterator<'a, Vertex>,
    schema: &Schema,
    type_name: &str,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    // `type_name` is the statically-known type. The vertices are definitely *at least* that type,
    // but could also be one of its subtypes. If there are no subtypes, they *must* be that type.
    let mut subtypes_iter = match schema.subtypes(type_name) {
        Some(iter) => iter,
        None => panic!("type {type_name} is not part of this schema"),
    };

    // Types are their own subtypes in the Schema::subtypes() method.
    // Is there a subtype that isn't the starting type itself?
    if subtypes_iter.any(|name| name != type_name) {
        // Subtypes exist, we have to check each vertex separately.
        resolve_property_with(contexts, |vertex| vertex.typename().into())
    } else {
        // No other subtypes exist.
        // All vertices here must be of exactly `type_name` type.
        let type_name: FieldValue = type_name.into();
        Box::new(contexts.map(move |ctx| match ctx.active_vertex() {
            None => (ctx, FieldValue::Null),
            Some(..) => (ctx, type_name.clone()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use crate::{
        interpreter::{helpers::resolve_typename, DataContext, Typename},
        ir::FieldValue,
        schema::Schema,
    };

    #[test]
    fn typename_resolved_statically() {
        #[derive(Debug, Clone)]
        enum Vertex {
            Variant,
        }

        impl Typename for Vertex {
            fn typename(&self) -> &'static str {
                unreachable!("typename() was called, so __typename was not resolved statically")
            }
        }

        let schema = Schema::parse(
            "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD

type RootSchemaQuery {
    Vertex: Vertex!
}

type Vertex {
    field: Int
}",
        )
        .expect("failed to parse schema");
        let contexts = Box::new(std::iter::once(DataContext::new(Some(Vertex::Variant))));

        let outputs: Vec<_> = resolve_typename(contexts, &schema, "Vertex")
            .map(|(_ctx, value)| value)
            .collect();

        assert_eq!(vec![FieldValue::from("Vertex")], outputs);
    }
}
