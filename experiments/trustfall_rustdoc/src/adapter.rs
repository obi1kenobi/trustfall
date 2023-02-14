use std::{collections::BTreeSet, sync::Arc};

use rustdoc_types::{
    Crate, Enum, Function, Id, Impl, Item, ItemEnum, Method, Path, Span, Struct, Trait, Type,
    Variant,
};
use trustfall_core::{
    interpreter::{
        Adapter, ContextIterator, ContextOutcomeIterator, DataContext, QueryInfo, VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
    schema::Schema,
};

use crate::indexed_crate::IndexedCrate;

#[non_exhaustive]
pub struct RustdocAdapter<'a> {
    current_crate: &'a IndexedCrate<'a>,
    previous_crate: Option<&'a IndexedCrate<'a>>,
}

impl<'a> RustdocAdapter<'a> {
    pub fn new(
        current_crate: &'a IndexedCrate<'a>,
        previous_crate: Option<&'a IndexedCrate<'a>>,
    ) -> Self {
        Self {
            current_crate,
            previous_crate,
        }
    }

    pub fn schema() -> Schema {
        Schema::parse(include_str!("rustdoc_schema.graphql")).expect("schema not valid")
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum Origin {
    CurrentCrate,
    PreviousCrate,
}

impl Origin {
    fn make_item_vertex<'a>(&self, item: &'a Item) -> Token<'a> {
        Token {
            origin: *self,
            kind: item.into(),
        }
    }

    fn make_span_vertex<'a>(&self, span: &'a Span) -> Token<'a> {
        Token {
            origin: *self,
            kind: span.into(),
        }
    }

    fn make_path_vertex<'a>(&self, path: &'a [String]) -> Token<'a> {
        Token {
            origin: *self,
            kind: TokenKind::Path(path),
        }
    }

    fn make_importable_path_vertex<'a>(&self, importable_path: Vec<&'a str>) -> Token<'a> {
        Token {
            origin: *self,
            kind: TokenKind::ImportablePath(importable_path),
        }
    }

    fn make_raw_type_vertex<'a>(&self, raw_type: &'a rustdoc_types::Type) -> Token<'a> {
        Token {
            origin: *self,
            kind: TokenKind::RawType(raw_type),
        }
    }

    fn make_attribute_vertex<'a>(&self, attr: &'a str) -> Token<'a> {
        Token {
            origin: *self,
            kind: TokenKind::Attribute(attr),
        }
    }

    fn make_implemented_trait_vertex<'a>(
        &self,
        path: &'a rustdoc_types::Path,
        trait_def: &'a Item,
    ) -> Token<'a> {
        Token {
            origin: *self,
            kind: TokenKind::ImplementedTrait(path, trait_def),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Token<'a> {
    origin: Origin,
    kind: TokenKind<'a>,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum TokenKind<'a> {
    CrateDiff((&'a IndexedCrate<'a>, &'a IndexedCrate<'a>)),
    Crate(&'a IndexedCrate<'a>),
    Item(&'a Item),
    Span(&'a Span),
    Path(&'a [String]),
    ImportablePath(Vec<&'a str>),
    RawType(&'a Type),
    Attribute(&'a str),
    ImplementedTrait(&'a Path, &'a Item),
}

#[allow(dead_code)]
impl<'a> Token<'a> {
    fn new_crate(origin: Origin, crate_: &'a IndexedCrate<'a>) -> Self {
        Self {
            origin,
            kind: TokenKind::Crate(crate_),
        }
    }

    /// The name of the actual runtime type of this vertex,
    /// intended to fulfill resolution requests for the __typename property.
    #[inline]
    fn typename(&self) -> &'static str {
        match self.kind {
            TokenKind::Item(item) => match &item.inner {
                rustdoc_types::ItemEnum::Struct(..) => "Struct",
                rustdoc_types::ItemEnum::Enum(..) => "Enum",
                rustdoc_types::ItemEnum::Function(..) => "Function",
                rustdoc_types::ItemEnum::Method(..) => "Method",
                rustdoc_types::ItemEnum::Variant(Variant::Plain) => "PlainVariant",
                rustdoc_types::ItemEnum::Variant(Variant::Tuple(..)) => "TupleVariant",
                rustdoc_types::ItemEnum::Variant(Variant::Struct(..)) => "StructVariant",
                rustdoc_types::ItemEnum::StructField(..) => "StructField",
                rustdoc_types::ItemEnum::Impl(..) => "Impl",
                rustdoc_types::ItemEnum::Trait(..) => "Trait",
                _ => unreachable!("unexpected item.inner for item: {item:?}"),
            },
            TokenKind::Span(..) => "Span",
            TokenKind::Path(..) => "Path",
            TokenKind::ImportablePath(..) => "ImportablePath",
            TokenKind::Crate(..) => "Crate",
            TokenKind::CrateDiff(..) => "CrateDiff",
            TokenKind::Attribute(..) => "Attribute",
            TokenKind::ImplementedTrait(..) => "ImplementedTrait",
            TokenKind::RawType(ty) => match ty {
                rustdoc_types::Type::ResolvedPath { .. } => "ResolvedPathType",
                rustdoc_types::Type::Primitive(..) => "PrimitiveType",
                _ => "OtherType",
            },
        }
    }

    fn as_crate_diff(&self) -> Option<(&'a IndexedCrate<'a>, &'a IndexedCrate<'a>)> {
        match &self.kind {
            TokenKind::CrateDiff(tuple) => Some(*tuple),
            _ => None,
        }
    }

    fn as_indexed_crate(&self) -> Option<&'a IndexedCrate<'a>> {
        match self.kind {
            TokenKind::Crate(c) => Some(c),
            _ => None,
        }
    }

    fn as_crate(&self) -> Option<&'a Crate> {
        self.as_indexed_crate().map(|c| c.inner)
    }

    fn as_item(&self) -> Option<&'a Item> {
        match self.kind {
            TokenKind::Item(item) => Some(item),
            _ => None,
        }
    }

    fn as_struct_item(&self) -> Option<(&'a Item, &'a Struct)> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Struct(s) => Some((item, s)),
            _ => None,
        })
    }

    fn as_struct_field_item(&self) -> Option<(&'a Item, &'a Type)> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::StructField(s) => Some((item, s)),
            _ => None,
        })
    }

    fn as_span(&self) -> Option<&'a Span> {
        match self.kind {
            TokenKind::Span(s) => Some(s),
            _ => None,
        }
    }

    fn as_enum(&self) -> Option<&'a Enum> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Enum(e) => Some(e),
            _ => None,
        })
    }

    fn as_trait(&self) -> Option<&'a Trait> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Trait(t) => Some(t),
            _ => None,
        })
    }

    fn as_variant(&self) -> Option<&'a Variant> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Variant(v) => Some(v),
            _ => None,
        })
    }

    fn as_path(&self) -> Option<&'a [String]> {
        match &self.kind {
            TokenKind::Path(path) => Some(*path),
            _ => None,
        }
    }

    fn as_importable_path(&self) -> Option<&'_ Vec<&'a str>> {
        match &self.kind {
            TokenKind::ImportablePath(path) => Some(path),
            _ => None,
        }
    }

    fn as_function(&self) -> Option<&'a Function> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Function(func) => Some(func),
            _ => None,
        })
    }

    fn as_method(&self) -> Option<&'a Method> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Method(func) => Some(func),
            _ => None,
        })
    }

    fn as_impl(&self) -> Option<&'a Impl> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Impl(x) => Some(x),
            _ => None,
        })
    }

    fn as_attribute(&self) -> Option<&'a str> {
        match &self.kind {
            TokenKind::Attribute(attr) => Some(*attr),
            _ => None,
        }
    }

    fn as_raw_type(&self) -> Option<&'a rustdoc_types::Type> {
        match &self.kind {
            TokenKind::RawType(ty) => Some(*ty),
            _ => None,
        }
    }

    fn as_implemented_trait(&self) -> Option<(&'a rustdoc_types::Path, &'a Item)> {
        match &self.kind {
            TokenKind::ImplementedTrait(path, trait_item) => Some((*path, *trait_item)),
            _ => None,
        }
    }
}

impl<'a> From<&'a Item> for TokenKind<'a> {
    fn from(item: &'a Item) -> Self {
        Self::Item(item)
    }
}

impl<'a> From<&'a IndexedCrate<'a>> for TokenKind<'a> {
    fn from(c: &'a IndexedCrate<'a>) -> Self {
        Self::Crate(c)
    }
}

impl<'a> From<&'a Span> for TokenKind<'a> {
    fn from(s: &'a Span) -> Self {
        Self::Span(s)
    }
}

fn get_crate_property(crate_vertex: &Token, field_name: &str) -> FieldValue {
    let crate_item = crate_vertex.as_crate().expect("vertex was not a Crate");
    match field_name {
        "root" => (&crate_item.root.0).into(),
        "crate_version" => (&crate_item.crate_version).into(),
        "includes_private" => crate_item.includes_private.into(),
        "format_version" => crate_item.format_version.into(),
        _ => unreachable!("Crate property {field_name}"),
    }
}

fn get_item_property(item_vertex: &Token, field_name: &str) -> FieldValue {
    let item = item_vertex.as_item().expect("vertex was not an Item");
    match field_name {
        "id" => (&item.id.0).into(),
        "crate_id" => (&item.crate_id).into(),
        "name" => (&item.name).into(),
        "docs" => (&item.docs).into(),
        "attrs" => item.attrs.clone().into(),
        "visibility_limit" => match &item.visibility {
            rustdoc_types::Visibility::Public => "public".into(),
            rustdoc_types::Visibility::Default => "default".into(),
            rustdoc_types::Visibility::Crate => "crate".into(),
            rustdoc_types::Visibility::Restricted { parent: _, path } => {
                format!("restricted ({path})").into()
            }
        },
        _ => unreachable!("Item property {field_name}"),
    }
}

fn get_struct_property(item_vertex: &Token, field_name: &str) -> FieldValue {
    let (_, struct_item) = item_vertex
        .as_struct_item()
        .expect("vertex was not a Struct");
    match field_name {
        "struct_type" => match struct_item.struct_type {
            rustdoc_types::StructType::Plain => "plain",
            rustdoc_types::StructType::Tuple => "tuple",
            rustdoc_types::StructType::Unit => "unit",
        }
        .into(),
        "fields_stripped" => struct_item.fields_stripped.into(),
        _ => unreachable!("Struct property {field_name}"),
    }
}

fn get_span_property(item_vertex: &Token, field_name: &str) -> FieldValue {
    let span = item_vertex.as_span().expect("vertex was not a Span");
    match field_name {
        "filename" => span
            .filename
            .to_str()
            .expect("non-representable path")
            .into(),
        "begin_line" => (span.begin.0 as u64).into(),
        "begin_column" => (span.begin.1 as u64).into(),
        "end_line" => (span.end.0 as u64).into(),
        "end_column" => (span.end.1 as u64).into(),
        _ => unreachable!("Span property {field_name}"),
    }
}

fn get_enum_property(item_vertex: &Token, field_name: &str) -> FieldValue {
    let enum_item = item_vertex.as_enum().expect("vertex was not an Enum");
    match field_name {
        "variants_stripped" => enum_item.variants_stripped.into(),
        _ => unreachable!("Enum property {field_name}"),
    }
}

fn get_path_property(vertex: &Token, field_name: &str) -> FieldValue {
    let path_vertex = vertex.as_path().expect("vertex was not a Path");
    match field_name {
        "path" => path_vertex.into(),
        _ => unreachable!("Path property {field_name}"),
    }
}

fn get_importable_path_property(vertex: &Token, field_name: &str) -> FieldValue {
    let path_vertex = vertex
        .as_importable_path()
        .expect("vertex was not an ImportablePath");
    match field_name {
        "path" => path_vertex
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .into(),
        "visibility_limit" => "public".into(),
        _ => unreachable!("ImportablePath property {field_name}"),
    }
}

fn get_function_like_property(vertex: &Token, field_name: &str) -> FieldValue {
    let maybe_function = vertex.as_function();
    let maybe_method = vertex.as_method();

    let (header, _decl) = maybe_function
        .map(|func| (&func.header, &func.decl))
        .unwrap_or_else(|| {
            let method = maybe_method.unwrap_or_else(|| {
                unreachable!("vertex was neither a function nor a method: {vertex:?}")
            });
            (&method.header, &method.decl)
        });

    match field_name {
        "const" => header.const_.into(),
        "async" => header.async_.into(),
        "unsafe" => header.unsafe_.into(),
        _ => unreachable!("FunctionLike property {field_name}"),
    }
}

fn get_impl_property(vertex: &Token, field_name: &str) -> FieldValue {
    let impl_vertex = vertex.as_impl().expect("vertex was not an Impl");
    match field_name {
        "unsafe" => impl_vertex.is_unsafe.into(),
        "negative" => impl_vertex.negative.into(),
        "synthetic" => impl_vertex.synthetic.into(),
        _ => unreachable!("Impl property {field_name}"),
    }
}

fn get_attribute_property(vertex: &Token, field_name: &str) -> FieldValue {
    let attribute_vertex = vertex.as_attribute().expect("vertex was not an Attribute");
    match field_name {
        "value" => attribute_vertex.into(),
        _ => unreachable!("Attribute property {field_name}"),
    }
}

fn get_raw_type_property(vertex: &Token, field_name: &str) -> FieldValue {
    let type_vertex = vertex.as_raw_type().expect("vertex was not a RawType");
    match field_name {
        "name" => match type_vertex {
            rustdoc_types::Type::ResolvedPath(path) => (&path.name).into(),
            rustdoc_types::Type::Primitive(name) => name.into(),
            _ => unreachable!("unexpected RawType vertex content: {type_vertex:?}"),
        },
        _ => unreachable!("RawType property {field_name}"),
    }
}

fn get_implemented_trait_property(vertex: &Token, field_name: &str) -> FieldValue {
    let (path, _) = vertex
        .as_implemented_trait()
        .expect("vertex was not a ImplementedTrait");
    match field_name {
        "name" => (&path.name).into(),
        _ => unreachable!("ImplementedTrait property {field_name}"),
    }
}

fn property_mapper<'a>(
    ctx: DataContext<Token<'a>>,
    field_name: &str,
    property_getter: fn(&Token<'a>, &str) -> FieldValue,
) -> (DataContext<Token<'a>>, FieldValue) {
    let value = match ctx.active_vertex() {
        Some(vertex) => property_getter(vertex, field_name),
        None => FieldValue::Null,
    };
    (ctx, value)
}

impl<'a> Adapter<'a> for RustdocAdapter<'a> {
    type Vertex = Token<'a>;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _query_info: &QueryInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name.as_ref() {
            "Crate" => Box::new(std::iter::once(Token::new_crate(
                Origin::CurrentCrate,
                self.current_crate,
            ))),
            "CrateDiff" => {
                let previous_crate = self.previous_crate.expect("no previous crate provided");
                Box::new(std::iter::once(Token {
                    origin: Origin::CurrentCrate,
                    kind: TokenKind::CrateDiff((self.current_crate, previous_crate)),
                }))
            }
            _ => unreachable!("{edge_name}"),
        }
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
        if property_name.as_ref() == "__typename" {
            Box::new(contexts.map(|ctx| match ctx.active_vertex() {
                Some(vertex) => {
                    let value = vertex.typename().into();
                    (ctx, value)
                }
                None => (ctx, FieldValue::Null),
            }))
        } else {
            let property_name = property_name.clone();
            match type_name.as_ref() {
                "Crate" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_crate_property)
                })),
                "Item" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_item_property)
                })),
                "ImplOwner" | "Struct" | "StructField" | "Enum" | "Variant" | "PlainVariant"
                | "TupleVariant" | "StructVariant" | "Trait" | "Function" | "Method" | "Impl"
                    if matches!(
                        property_name.as_ref(),
                        "id" | "crate_id" | "name" | "docs" | "attrs" | "visibility_limit"
                    ) =>
                {
                    // properties inherited from Item, accesssed on Item subtypes
                    Box::new(contexts.map(move |ctx| {
                        property_mapper(ctx, property_name.as_ref(), get_item_property)
                    }))
                }
                "Struct" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_struct_property)
                })),
                "Enum" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_enum_property)
                })),
                "Span" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_span_property)
                })),
                "Path" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_path_property)
                })),
                "ImportablePath" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_importable_path_property)
                })),
                "FunctionLike" | "Function" | "Method"
                    if matches!(property_name.as_ref(), "const" | "unsafe" | "async") =>
                {
                    Box::new(contexts.map(move |ctx| {
                        property_mapper(ctx, property_name.as_ref(), get_function_like_property)
                    }))
                }
                "Impl" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_impl_property)
                })),
                "Attribute" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_attribute_property)
                })),
                "ImplementedTrait" => Box::new(contexts.map(move |ctx| {
                    property_mapper(ctx, property_name.as_ref(), get_implemented_trait_property)
                })),
                "RawType" | "ResolvedPathType" | "PrimitiveType"
                    if matches!(property_name.as_ref(), "name") =>
                {
                    Box::new(contexts.map(move |ctx| {
                        // fields from "RawType"
                        property_mapper(ctx, property_name.as_ref(), get_raw_type_property)
                    }))
                }
                _ => unreachable!("resolve_property {type_name} {property_name}"),
            }
        }
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
        match type_name.as_ref() {
            "CrateDiff" => match edge_name.as_ref() {
                "current" => Box::new(contexts.map(move |ctx| {
                    let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex() {
                        None => Box::new(std::iter::empty()),
                        Some(vertex) => {
                            let crate_tuple =
                                vertex.as_crate_diff().expect("vertex was not a CrateDiff");
                            let neighbor = Token::new_crate(Origin::CurrentCrate, crate_tuple.0);
                            Box::new(std::iter::once(neighbor))
                        }
                    };

                    (ctx, neighbors)
                })),
                "baseline" => Box::new(contexts.map(move |ctx| {
                    let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex() {
                        None => Box::new(std::iter::empty()),
                        Some(vertex) => {
                            let crate_tuple =
                                vertex.as_crate_diff().expect("vertex was not a CrateDiff");
                            let neighbor = Token::new_crate(Origin::PreviousCrate, crate_tuple.1);
                            Box::new(std::iter::once(neighbor))
                        }
                    };

                    (ctx, neighbors)
                })),
                _ => {
                    unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}")
                }
            },
            "Crate" => {
                match edge_name.as_ref() {
                    "item" => Box::new(contexts.map(move |ctx| {
                        let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex()
                        {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                let origin = vertex.origin;
                                let crate_vertex =
                                    vertex.as_indexed_crate().expect("vertex was not a Crate");

                                // let iter = crate_vertex
                                //     .public_items
                                //     .iter()
                                //     .copied()
                                //     .filter_map(|id| crate_vertex.inner.index.get(id))
                                //     .filter(|item| {
                                //         matches!(
                                //             item.inner,
                                //             rustdoc_types::ItemEnum::Struct(..)
                                //                 | rustdoc_types::ItemEnum::StructField(..)
                                //                 | rustdoc_types::ItemEnum::Enum(..)
                                //                 | rustdoc_types::ItemEnum::Variant(..)
                                //                 | rustdoc_types::ItemEnum::Function(..)
                                //                 | rustdoc_types::ItemEnum::Method(..)
                                //                 | rustdoc_types::ItemEnum::Impl(..)
                                //         )
                                //     })
                                //     .map(move |value| origin.make_item_vertex(value));
                                // Box::new(iter)
                                // TODO: temporarily only return public items for testing
                                //
                                let iter = crate_vertex
                                    .inner
                                    .index
                                    .values()
                                    .filter(|item| {
                                        // Filter out item types that are not currently supported.
                                        matches!(
                                            item.inner,
                                            rustdoc_types::ItemEnum::Struct(..)
                                                | rustdoc_types::ItemEnum::StructField(..)
                                                | rustdoc_types::ItemEnum::Enum(..)
                                                | rustdoc_types::ItemEnum::Variant(..)
                                                | rustdoc_types::ItemEnum::Function(..)
                                                | rustdoc_types::ItemEnum::Method(..)
                                                | rustdoc_types::ItemEnum::Impl(..)
                                        )
                                    })
                                    .map(move |value| origin.make_item_vertex(value));
                                Box::new(iter)
                            }
                        };

                        (ctx, neighbors)
                    })),
                    _ => unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}"),
                }
            }
            "Importable" | "ImplOwner" | "Struct" | "Enum" | "Trait" | "Function"
                if matches!(edge_name.as_ref(), "importable_path" | "canonical_path") =>
            {
                match edge_name.as_ref() {
                    "canonical_path" => {
                        let current_crate = self.current_crate;
                        let previous_crate = self.previous_crate;

                        Box::new(contexts.map(move |ctx| {
                            let neighbors: VertexIterator<'a, Self::Vertex> = match ctx
                                .active_vertex()
                            {
                                None => Box::new(std::iter::empty()),
                                Some(vertex) => {
                                    let origin = vertex.origin;
                                    let item = vertex.as_item().expect("vertex was not an Item");
                                    let item_id = &item.id;

                                    if let Some(path) = match origin {
                                        Origin::CurrentCrate => {
                                            current_crate.inner.paths.get(item_id).map(|x| &x.path)
                                        }
                                        Origin::PreviousCrate => previous_crate
                                            .expect("no baseline provided")
                                            .inner
                                            .paths
                                            .get(item_id)
                                            .map(|x| &x.path),
                                    } {
                                        Box::new(std::iter::once(origin.make_path_vertex(path)))
                                    } else {
                                        Box::new(std::iter::empty())
                                    }
                                }
                            };

                            (ctx, neighbors)
                        }))
                    }
                    "importable_path" => {
                        let current_crate = self.current_crate;
                        let previous_crate = self.previous_crate;

                        Box::new(contexts.map(move |ctx| {
                            let neighbors: VertexIterator<'a, Self::Vertex> = match ctx
                                .active_vertex()
                            {
                                None => Box::new(std::iter::empty()),
                                Some(vertex) => {
                                    let origin = vertex.origin;
                                    let item = vertex.as_item().expect("vertex was not an Item");
                                    let item_id = &item.id;

                                    let parent_crate = match origin {
                                        Origin::CurrentCrate => current_crate,
                                        Origin::PreviousCrate => {
                                            previous_crate.expect("no baseline provided")
                                        }
                                    };

                                    Box::new(
                                        parent_crate
                                            .publicly_importable_names(item_id)
                                            .into_iter()
                                            .map(move |x| origin.make_importable_path_vertex(x)),
                                    )
                                }
                            };

                            (ctx, neighbors)
                        }))
                    }
                    _ => unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}"),
                }
            }
            "Item" | "ImplOwner" | "Struct" | "StructField" | "Enum" | "Variant"
            | "PlainVariant" | "TupleVariant" | "StructVariant" | "Trait" | "Function"
            | "Method" | "Impl"
                if matches!(edge_name.as_ref(), "span" | "attribute") =>
            {
                match edge_name.as_ref() {
                    "span" => Box::new(contexts.map(move |ctx| {
                        let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex()
                        {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                let origin = vertex.origin;
                                let item = vertex.as_item().expect("vertex was not an Item");
                                if let Some(span) = &item.span {
                                    Box::new(std::iter::once(origin.make_span_vertex(span)))
                                } else {
                                    Box::new(std::iter::empty())
                                }
                            }
                        };

                        (ctx, neighbors)
                    })),
                    "attribute" => Box::new(contexts.map(move |ctx| {
                        let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex()
                        {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                let origin = vertex.origin;
                                let item = vertex.as_item().expect("vertex was not an Item");
                                Box::new(
                                    item.attrs
                                        .iter()
                                        .map(move |attr| origin.make_attribute_vertex(attr)),
                                )
                            }
                        };

                        (ctx, neighbors)
                    })),
                    _ => unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}"),
                }
            }
            "ImplOwner" | "Struct" | "Enum"
                if matches!(edge_name.as_ref(), "impl" | "inherent_impl") =>
            {
                let current_crate = self.current_crate;
                let previous_crate = self.previous_crate;
                let inherent_impls_only = edge_name.as_ref() == "inherent_impl";
                Box::new(contexts.map(move |ctx| {
                    let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex() {
                        None => Box::new(std::iter::empty()),
                        Some(vertex) => {
                            let origin = vertex.origin;
                            let item_index = match origin {
                                Origin::CurrentCrate => &current_crate.inner.index,
                                Origin::PreviousCrate => {
                                    &previous_crate
                                        .expect("no previous crate provided")
                                        .inner
                                        .index
                                }
                            };

                            // Get the IDs of all the impl blocks.
                            // Relies on the fact that only structs and enums can have impls,
                            // so we know that the vertex must represent either a struct or an enum.
                            let impl_ids = vertex
                                .as_struct_item()
                                .map(|(_, s)| &s.impls)
                                .or_else(|| vertex.as_enum().map(|e| &e.impls))
                                .expect("vertex was neither a struct nor an enum");

                            Box::new(impl_ids.iter().filter_map(move |item_id| {
                                let next_item = item_index.get(item_id);
                                next_item.and_then(|next_item| match &next_item.inner {
                                    rustdoc_types::ItemEnum::Impl(imp) => {
                                        if !inherent_impls_only || imp.trait_.is_none() {
                                            Some(origin.make_item_vertex(next_item))
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                })
                            }))
                        }
                    };

                    (ctx, neighbors)
                }))
            }
            "Struct" => match edge_name.as_ref() {
                "field" => {
                    let current_crate = self.current_crate;
                    let previous_crate = self.previous_crate;
                    Box::new(contexts.map(move |ctx| {
                        let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex()
                        {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                let origin = vertex.origin;
                                let (_, struct_item) =
                                    vertex.as_struct_item().expect("vertex was not a Struct");

                                let item_index = match origin {
                                    Origin::CurrentCrate => &current_crate.inner.index,
                                    Origin::PreviousCrate => {
                                        &previous_crate
                                            .expect("no previous crate provided")
                                            .inner
                                            .index
                                    }
                                };
                                Box::new(struct_item.fields.clone().into_iter().map(
                                    move |field_id| {
                                        origin.make_item_vertex(
                                            item_index.get(&field_id).expect("missing item"),
                                        )
                                    },
                                ))
                            }
                        };

                        (ctx, neighbors)
                    }))
                }
                _ => {
                    unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}")
                }
            },
            "Enum" => match edge_name.as_ref() {
                "variant" => {
                    let current_crate = self.current_crate;
                    let previous_crate = self.previous_crate;
                    Box::new(contexts.map(move |ctx| {
                        let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex()
                        {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                let origin = vertex.origin;
                                let enum_item = vertex.as_enum().expect("vertex was not a Enum");

                                let item_index = match origin {
                                    Origin::CurrentCrate => &current_crate.inner.index,
                                    Origin::PreviousCrate => {
                                        &previous_crate
                                            .expect("no previous crate provided")
                                            .inner
                                            .index
                                    }
                                };
                                Box::new(enum_item.variants.iter().map(move |field_id| {
                                    origin.make_item_vertex(
                                        item_index.get(field_id).expect("missing item"),
                                    )
                                }))
                            }
                        };

                        (ctx, neighbors)
                    }))
                }
                _ => {
                    unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}")
                }
            },
            "StructField" => match edge_name.as_ref() {
                "raw_type" => Box::new(contexts.map(move |ctx| {
                    let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex() {
                        None => Box::new(std::iter::empty()),
                        Some(vertex) => {
                            let origin = vertex.origin;
                            let (_, field_type) = vertex
                                .as_struct_field_item()
                                .expect("not a StructField vertex");
                            Box::new(std::iter::once(origin.make_raw_type_vertex(field_type)))
                        }
                    };

                    (ctx, neighbors)
                })),
                _ => {
                    unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}")
                }
            },
            "Impl" => match edge_name.as_ref() {
                "method" => {
                    let current_crate = self.current_crate;
                    let previous_crate = self.previous_crate;
                    Box::new(contexts.map(move |ctx| {
                        let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex()
                        {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                let origin = vertex.origin;
                                let item_index = match origin {
                                    Origin::CurrentCrate => &current_crate.inner.index,
                                    Origin::PreviousCrate => {
                                        &previous_crate
                                            .expect("no previous crate provided")
                                            .inner
                                            .index
                                    }
                                };

                                let impl_vertex = vertex.as_impl().expect("not an Impl vertex");
                                let provided_methods: VertexIterator<'a, &Id> = if impl_vertex
                                    .provided_trait_methods
                                    .is_empty()
                                {
                                    Box::new(std::iter::empty())
                                } else {
                                    let method_names: BTreeSet<&str> = impl_vertex
                                        .provided_trait_methods
                                        .iter()
                                        .map(|x| x.as_str())
                                        .collect();

                                    let trait_path = impl_vertex.trait_.as_ref().expect(
                                        "no trait but provided_trait_methods was non-empty",
                                    );
                                    let trait_item = item_index.get(&trait_path.id);

                                    if let Some(trait_item) = trait_item {
                                        if let ItemEnum::Trait(trait_item) = &trait_item.inner {
                                            Box::new(trait_item.items.iter().filter(
                                                move |item_id| {
                                                    let next_item = &item_index.get(item_id);
                                                    if let Some(name) =
                                                        next_item.and_then(|x| x.name.as_deref())
                                                    {
                                                        method_names.contains(name)
                                                    } else {
                                                        false
                                                    }
                                                },
                                            ))
                                        } else {
                                            unreachable!("found a non-trait type {trait_item:?}");
                                        }
                                    } else {
                                        Box::new(std::iter::empty())
                                    }
                                };
                                Box::new(
                                    provided_methods.chain(impl_vertex.items.iter()).filter_map(
                                        move |item_id| {
                                            let next_item = &item_index.get(item_id);
                                            if let Some(next_item) = next_item {
                                                match &next_item.inner {
                                                    rustdoc_types::ItemEnum::Method(..) => {
                                                        Some(origin.make_item_vertex(next_item))
                                                    }
                                                    _ => None,
                                                }
                                            } else {
                                                None
                                            }
                                        },
                                    ),
                                )
                            }
                        };

                        (ctx, neighbors)
                    }))
                }
                "implemented_trait" => {
                    let current_crate = self.current_crate;
                    let previous_crate = self.previous_crate;
                    Box::new(contexts.map(move |ctx| {
                        let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex()
                        {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                let origin = vertex.origin;
                                let item_index = match origin {
                                    Origin::CurrentCrate => &current_crate.inner.index,
                                    Origin::PreviousCrate => {
                                        &previous_crate
                                            .expect("no previous crate provided")
                                            .inner
                                            .index
                                    }
                                };

                                let impl_vertex = vertex.as_impl().expect("not an Impl vertex");

                                if let Some(path) = &impl_vertex.trait_ {
                                    if let Some(item) = item_index.get(&path.id) {
                                        Box::new(std::iter::once(
                                            origin.make_implemented_trait_vertex(path, item),
                                        ))
                                    } else {
                                        Box::new(std::iter::empty())
                                    }
                                } else {
                                    Box::new(std::iter::empty())
                                }
                            }
                        };

                        (ctx, neighbors)
                    }))
                }
                _ => {
                    unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}")
                }
            },
            "ImplementedTrait" => match edge_name.as_ref() {
                "trait" => Box::new(contexts.map(move |ctx| {
                    let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex() {
                        None => Box::new(std::iter::empty()),
                        Some(vertex) => {
                            let origin = vertex.origin;

                            let (_, trait_item) = vertex
                                .as_implemented_trait()
                                .expect("vertex was not an ImplementedTrait");
                            Box::new(std::iter::once(origin.make_item_vertex(trait_item)))
                        }
                    };

                    (ctx, neighbors)
                })),
                _ => {
                    unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}")
                }
            },
            _ => unreachable!("resolve_neighbors {type_name} {edge_name} {parameters:?}"),
        }
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        let coerce_to_type = coerce_to_type.clone();
        match type_name.as_ref() {
            "Item" | "Variant" | "FunctionLike" | "Importable" | "ImplOwner" | "RawType"
            | "ResolvedPathType" => {
                Box::new(contexts.map(move |ctx| {
                    let can_coerce = match ctx.active_vertex() {
                        None => false,
                        Some(vertex) => {
                            let actual_type_name = vertex.typename();

                            match coerce_to_type.as_ref() {
                                "Variant" => matches!(
                                    actual_type_name,
                                    "PlainVariant" | "TupleVariant" | "StructVariant"
                                ),
                                "ImplOwner" => matches!(actual_type_name, "Struct" | "Enum"),
                                "ResolvedPathType" => matches!(
                                    actual_type_name,
                                    "ResolvedPathType" | "ImplementedTrait"
                                ),
                                _ => {
                                    // The remaining types are final (don't have any subtypes)
                                    // so we can just compare the actual type name to
                                    // the type we are attempting to coerce to.
                                    actual_type_name == coerce_to_type.as_ref()
                                }
                            }
                        }
                    };

                    (ctx, can_coerce)
                }))
            }
            _ => unreachable!("resolve_coercion {type_name} {coerce_to_type}"),
        }
    }
}
