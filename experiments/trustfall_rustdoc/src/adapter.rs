use std::sync::Arc;

use rustdoc_types::{Crate, Enum, Item, Span, Struct, Type, Variant};
use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
    schema::Schema,
};

pub struct RustdocAdapter<'a> {
    current_crate: &'a Crate,
    previous_crate: Option<&'a Crate>,
}

impl<'a> RustdocAdapter<'a> {
    pub fn new(current_crate: &'a Crate, previous_crate: Option<&'a Crate>) -> Self {
        Self {
            current_crate,
            previous_crate,
        }
    }

    pub fn schema() -> Schema {
        Schema::parse(include_str!("rustdoc.graphql")).expect("schema not valid")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Origin {
    CurrentCrate,
    PreviousCrate,
}

impl Origin {
    fn make_item_token<'a>(&self, item: &'a Item) -> Token<'a> {
        Token {
            origin: *self,
            kind: item.into(),
        }
    }

    fn make_span_token<'a>(&self, span: &'a Span) -> Token<'a> {
        Token {
            origin: *self,
            kind: span.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token<'a> {
    origin: Origin,
    kind: TokenKind<'a>,
}

impl<'a> Token<'a> {
    fn new_crate(origin: Origin, crate_: &'a Crate) -> Self {
        Self {
            origin,
            kind: TokenKind::Crate(crate_),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TokenKind<'a> {
    CrateDiff((&'a Crate, &'a Crate)),
    Crate(&'a Crate),
    Item(&'a Item),
    Span(&'a Span),
}

#[allow(dead_code)]
impl<'a> Token<'a> {
    fn as_crate_diff(&self) -> Option<(&'a Crate, &'a Crate)> {
        match &self.kind {
            TokenKind::CrateDiff(tuple) => Some(*tuple),
            _ => None,
        }
    }

    fn as_crate(&self) -> Option<&'a Crate> {
        match self.kind {
            TokenKind::Crate(c) => Some(c),
            _ => None,
        }
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

    fn as_variant(&self) -> Option<&'a Variant> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Variant(v) => Some(v),
            _ => None,
        })
    }
}

impl<'a> From<&'a Item> for TokenKind<'a> {
    fn from(item: &'a Item) -> Self {
        Self::Item(item)
    }
}

impl<'a> From<&'a Crate> for TokenKind<'a> {
    fn from(c: &'a Crate) -> Self {
        Self::Crate(c)
    }
}

impl<'a> From<&'a Span> for TokenKind<'a> {
    fn from(s: &'a Span) -> Self {
        Self::Span(s)
    }
}

fn get_crate_property(crate_token: &Token, field_name: &str) -> FieldValue {
    let crate_item = crate_token.as_crate().expect("token was not a Crate");
    match field_name {
        "root" => (&crate_item.root.0).into(),
        "crate_version" => (&crate_item.crate_version).into(),
        "includes_private" => crate_item.includes_private.into(),
        "format_version" => crate_item.format_version.into(),
        _ => unreachable!("Crate property {field_name}"),
    }
}

fn get_item_property(item_token: &Token, field_name: &str) -> FieldValue {
    let item = item_token.as_item().expect("token was not an Item");
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

fn get_struct_property(item_token: &Token, field_name: &str) -> FieldValue {
    let (_, struct_item) = item_token.as_struct_item().expect("token was not a Struct");
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

fn get_span_property(item_token: &Token, field_name: &str) -> FieldValue {
    let span = item_token.as_span().expect("token was not a Span");
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

fn get_enum_property(item_token: &Token, field_name: &str) -> FieldValue {
    let enum_item = item_token.as_enum().expect("token was not an Enum");
    match field_name {
        "variants_stripped" => enum_item.variants_stripped.into(),
        _ => unreachable!("Enum property {field_name}"),
    }
}

fn property_mapper<'a>(
    ctx: DataContext<Token<'a>>,
    field_name: &str,
    property_getter: fn(&Token<'a>, &str) -> FieldValue,
) -> (DataContext<Token<'a>>, FieldValue) {
    let value = match &ctx.current_token {
        Some(token) => property_getter(token, field_name),
        None => FieldValue::Null,
    };
    (ctx, value)
}

impl<'a> Adapter<'a> for RustdocAdapter<'a> {
    type DataToken = Token<'a>;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        _parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'a> {
        match edge.as_ref() {
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
            _ => unreachable!("{edge}"),
        }
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)> + 'a> {
        if field_name.as_ref() == "__typename" {
            match current_type_name.as_ref() {
                "Crate" | "Struct" | "StructField" | "Span" | "Enum" | "PlainVariant"
                | "TupleVariant" | "StructVariant" => {
                    // These types have no subtypes, so their __typename
                    // is always equal to their statically-determined type.
                    let typename: FieldValue = current_type_name.as_ref().into();
                    Box::new(data_contexts.map(move |ctx| {
                        if ctx.current_token.is_some() {
                            (ctx, typename.clone())
                        } else {
                            (ctx, FieldValue::Null)
                        }
                    }))
                }
                "Variant" => {
                    // Inspect the inner type of the token and
                    // output the appropriate __typename value.
                    Box::new(data_contexts.map(|ctx| {
                        let value = match &ctx.current_token {
                            None => FieldValue::Null,
                            Some(token) => match token.as_item() {
                                Some(item) => match &item.inner {
                                    rustdoc_types::ItemEnum::Variant(Variant::Plain) => {
                                        "PlainVariant".into()
                                    }
                                    rustdoc_types::ItemEnum::Variant(Variant::Tuple(..)) => {
                                        "TupleVariant".into()
                                    }
                                    rustdoc_types::ItemEnum::Variant(Variant::Struct(..)) => {
                                        "StructVariant".into()
                                    }
                                    _ => {
                                        unreachable!("unexpected item.inner type: {:?}", item.inner)
                                    }
                                },
                                _ => unreachable!("unexpected token type: {token:?}"),
                            },
                        };
                        (ctx, value)
                    }))
                }
                "Item" => {
                    // Inspect the inner type of the token and
                    // output the appropriate __typename value.
                    Box::new(data_contexts.map(|ctx| {
                        let value = match &ctx.current_token {
                            None => FieldValue::Null,
                            Some(token) => match token.as_item() {
                                Some(item) => match &item.inner {
                                    rustdoc_types::ItemEnum::Struct(_) => "Struct".into(),
                                    rustdoc_types::ItemEnum::StructField(_) => "StructField".into(),
                                    _ => {
                                        unreachable!("unexpected item.inner type: {:?}", item.inner)
                                    }
                                },
                                _ => unreachable!("unexpected token type: {token:?}"),
                            },
                        };
                        (ctx, value)
                    }))
                }
                _ => unreachable!("project_property for __typename on {current_type_name}"),
            }
        } else {
            match current_type_name.as_ref() {
                "Crate" => {
                    Box::new(data_contexts.map(move |ctx| {
                        property_mapper(ctx, field_name.as_ref(), get_crate_property)
                    }))
                }
                "Item" => {
                    Box::new(data_contexts.map(move |ctx| {
                        property_mapper(ctx, field_name.as_ref(), get_item_property)
                    }))
                }
                "Struct" | "StructField" | "Enum" | "Variant" | "PlainVariant" | "TupleVariant"
                | "StructVariant"
                    if matches!(
                        field_name.as_ref(),
                        "id" | "crate_id" | "name" | "docs" | "attrs" | "visibility_limit"
                    ) =>
                {
                    // properties inherited from Item, accesssed on Item subtypes
                    Box::new(data_contexts.map(move |ctx| {
                        property_mapper(ctx, field_name.as_ref(), get_item_property)
                    }))
                }
                "Struct" => Box::new(data_contexts.map(move |ctx| {
                    property_mapper(ctx, field_name.as_ref(), get_struct_property)
                })),
                "Enum" => {
                    Box::new(data_contexts.map(move |ctx| {
                        property_mapper(ctx, field_name.as_ref(), get_enum_property)
                    }))
                }
                "Span" => {
                    Box::new(data_contexts.map(move |ctx| {
                        property_mapper(ctx, field_name.as_ref(), get_span_property)
                    }))
                }
                _ => unreachable!("project_property {current_type_name} {field_name}"),
            }
        }
    }

    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
        _edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
                Item = (
                    DataContext<Self::DataToken>,
                    Box<dyn Iterator<Item = Self::DataToken> + 'a>,
                ),
            > + 'a,
    > {
        match current_type_name.as_ref() {
            "CrateDiff" => match edge_name.as_ref() {
                "current" => Box::new(data_contexts.map(move |ctx| {
                    let neighbors: Box<dyn Iterator<Item = Self::DataToken> + 'a> = match &ctx
                        .current_token
                    {
                        None => Box::new(std::iter::empty()),
                        Some(token) => {
                            let crate_tuple =
                                token.as_crate_diff().expect("token was not a CrateDiff");
                            let neighbor = Token::new_crate(Origin::CurrentCrate, crate_tuple.0);
                            Box::new(std::iter::once(neighbor))
                        }
                    };

                    (ctx, neighbors)
                })),
                "previous" => Box::new(data_contexts.map(move |ctx| {
                    let neighbors: Box<dyn Iterator<Item = Self::DataToken> + 'a> = match &ctx
                        .current_token
                    {
                        None => Box::new(std::iter::empty()),
                        Some(token) => {
                            let crate_tuple =
                                token.as_crate_diff().expect("token was not a CrateDiff");
                            let neighbor = Token::new_crate(Origin::PreviousCrate, crate_tuple.1);
                            Box::new(std::iter::once(neighbor))
                        }
                    };

                    (ctx, neighbors)
                })),
                _ => {
                    unreachable!("project_neighbors {current_type_name} {edge_name} {parameters:?}")
                }
            },
            "Crate" => {
                match edge_name.as_ref() {
                    "item" => Box::new(data_contexts.map(move |ctx| {
                        let neighbors: Box<dyn Iterator<Item = Self::DataToken> + 'a> = match &ctx
                            .current_token
                        {
                            None => Box::new(std::iter::empty()),
                            Some(token) => {
                                let origin = token.origin;
                                let crate_token = token.as_crate().expect("token was not a Crate");
                                let iter = crate_token
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
                                        )
                                    })
                                    .map(move |value| origin.make_item_token(value));
                                Box::new(iter)
                            }
                        };

                        (ctx, neighbors)
                    })),
                    _ => unreachable!(
                        "project_neighbors {current_type_name} {edge_name} {parameters:?}"
                    ),
                }
            }
            "Item" | "Struct" | "StructField" | "Enum" | "Variant" | "PlainVariant"
            | "TupleVariant" | "StructVariant"
                if edge_name.as_ref() == "span" =>
            {
                Box::new(data_contexts.map(move |ctx| {
                    let neighbors: Box<dyn Iterator<Item = Self::DataToken> + 'a> =
                        match &ctx.current_token {
                            None => Box::new(std::iter::empty()),
                            Some(token) => {
                                let origin = token.origin;
                                let item = token.as_item().expect("token was not an Item");
                                if let Some(span) = &item.span {
                                    Box::new(std::iter::once(origin.make_span_token(span)))
                                } else {
                                    Box::new(std::iter::empty())
                                }
                            }
                        };

                    (ctx, neighbors)
                }))
            }
            "Struct" => match edge_name.as_ref() {
                "field" => {
                    let current_crate = self.current_crate;
                    let previous_crate = self.previous_crate;
                    Box::new(data_contexts.map(move |ctx| {
                        let neighbors: Box<dyn Iterator<Item = Self::DataToken> + 'a> = match &ctx
                            .current_token
                        {
                            None => Box::new(std::iter::empty()),
                            Some(token) => {
                                let origin = token.origin;
                                let (_, struct_item) =
                                    token.as_struct_item().expect("token was not a Struct");

                                let item_index = match origin {
                                    Origin::CurrentCrate => &current_crate.index,
                                    Origin::PreviousCrate => {
                                        &previous_crate.expect("no previous crate provided").index
                                    }
                                };
                                Box::new(struct_item.fields.clone().into_iter().map(
                                    move |field_id| {
                                        origin.make_item_token(
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
                    unreachable!("project_neighbors {current_type_name} {edge_name} {parameters:?}")
                }
            },
            "Enum" => match edge_name.as_ref() {
                "variant" => {
                    let current_crate = self.current_crate;
                    let previous_crate = self.previous_crate;
                    Box::new(data_contexts.map(move |ctx| {
                        let neighbors: Box<dyn Iterator<Item = Self::DataToken> + 'a> = match &ctx
                            .current_token
                        {
                            None => Box::new(std::iter::empty()),
                            Some(token) => {
                                let origin = token.origin;
                                let enum_item = token.as_enum().expect("token was not a Enum");

                                let item_index = match origin {
                                    Origin::CurrentCrate => &current_crate.index,
                                    Origin::PreviousCrate => {
                                        &previous_crate.expect("no previous crate provided").index
                                    }
                                };
                                Box::new(enum_item.variants.iter().map(move |field_id| {
                                    origin.make_item_token(
                                        item_index.get(field_id).expect("missing item"),
                                    )
                                }))
                            }
                        };

                        (ctx, neighbors)
                    }))
                }
                _ => {
                    unreachable!("project_neighbors {current_type_name} {edge_name} {parameters:?}")
                }
            },
            _ => unreachable!("project_neighbors {current_type_name} {edge_name} {parameters:?}"),
        }
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'a> {
        match current_type_name.as_ref() {
            "Item" | "Variant" => {
                Box::new(data_contexts.map(move |ctx| {
                    let can_coerce = match &ctx.current_token {
                        None => false,
                        Some(token) => {
                            let actual_type_name = match token.as_item() {
                                Some(item) => match &item.inner {
                                    rustdoc_types::ItemEnum::Struct(..) => "Struct",
                                    rustdoc_types::ItemEnum::StructField(..) => "StructField",
                                    rustdoc_types::ItemEnum::Enum(..) => "Enum",
                                    rustdoc_types::ItemEnum::Variant(Variant::Plain) => {
                                        "PlainVariant"
                                    }
                                    rustdoc_types::ItemEnum::Variant(Variant::Tuple(..)) => {
                                        "TupleVariant"
                                    }
                                    rustdoc_types::ItemEnum::Variant(Variant::Struct(..)) => {
                                        "StructVariant"
                                    }
                                    _ => {
                                        unreachable!("unexpected item.inner type: {:?}", item.inner)
                                    }
                                },
                                _ => unreachable!("unexpected token type: {token:?}"),
                            };

                            match coerce_to_type_name.as_ref() {
                                "Variant" => matches!(
                                    actual_type_name,
                                    "PlainVariant" | "TupleVariant" | "StructVariant"
                                ),
                                _ => {
                                    // The remaining types are final (don't have any subtypes)
                                    // so we can just compare the actual type name to
                                    // the type we are attempting to coerce to.
                                    actual_type_name == coerce_to_type_name.as_ref()
                                }
                            }
                        }
                    };

                    (ctx, can_coerce)
                }))
            }
            _ => unreachable!("can_coerce_to_type {current_type_name} {coerce_to_type_name}"),
        }
    }
}
