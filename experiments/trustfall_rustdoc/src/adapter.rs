use std::{rc::Rc, sync::Arc};

use rustdoc_types::{Crate, Item, Span, Struct, Type};
use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
};

pub struct RustdocAdapter {
    starting_crate: Rc<Crate>,
}

impl RustdocAdapter {
    pub fn new(starting_crate: Rc<Crate>) -> Self {
        Self { starting_crate }
    }
}

#[derive(Debug, Clone)]
pub enum Token {
    Crate(Rc<Crate>),
    Item(Rc<Item>),
    Span(Rc<Span>),
}

impl Token {
    fn as_crate(&self) -> Option<&Crate> {
        match self {
            Token::Crate(c) => Some(c.as_ref()),
            _ => None,
        }
    }

    fn as_item(&self) -> Option<&Item> {
        match self {
            Token::Item(item) => Some(item.as_ref()),
            _ => None,
        }
    }

    fn as_struct_item(&self) -> Option<(&Item, &Struct)> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::Struct(s) => Some((item, s)),
            _ => None,
        })
    }

    #[allow(dead_code)]
    fn as_struct_field_item(&self) -> Option<(&Item, &Type)> {
        self.as_item().and_then(|item| match &item.inner {
            rustdoc_types::ItemEnum::StructField(s) => Some((item, s)),
            _ => None,
        })
    }

    fn as_span(&self) -> Option<&Span> {
        match self {
            Token::Span(s) => Some(s.as_ref()),
            _ => None,
        }
    }
}

impl From<&Item> for Token {
    fn from(item: &Item) -> Self {
        Self::Item(Rc::from(item.clone()))
    }
}

impl From<&Crate> for Token {
    fn from(c: &Crate) -> Self {
        Self::Crate(Rc::from(c.clone()))
    }
}

impl From<&Span> for Token {
    fn from(s: &Span) -> Self {
        Self::Span(Rc::from(s.clone()))
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
        "visibilityLimit" => match &item.visibility {
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

fn property_mapper(
    ctx: DataContext<Token>,
    field_name: &str,
    property_getter: fn(&Token, &str) -> FieldValue,
) -> (DataContext<Token>, FieldValue) {
    let current_token = &ctx.current_token;
    let value = match current_token {
        Some(token) => property_getter(token, field_name),
        None => FieldValue::Null,
    };
    (ctx, value)
}

impl Adapter<'static> for RustdocAdapter {
    type DataToken = Token;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        _parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken>> {
        match edge.as_ref() {
            "Crate" => Box::new(std::iter::once(Token::Crate(self.starting_crate.clone()))),
            _ => unreachable!("{edge}"),
        }
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)>> {
        if field_name.as_ref() == "__typename" {
            match current_type_name.as_ref() {
                "Crate" | "Struct" | "StructField" | "Span" => {
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
                "Item" => {
                    // Inspect the inner type of the token and
                    // output the appropriate __typename value.
                    Box::new(data_contexts.map(|ctx| {
                        let value = match &ctx.current_token {
                            None => FieldValue::Null,
                            Some(token) => match token {
                                Token::Item(item) => match &item.inner {
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
                "Struct" | "StructField"
                    if matches!(
                        field_name.as_ref(),
                        "id" | "crate_id" | "name" | "docs" | "attrs" | "visibilityLimit"
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
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
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
                Box<dyn Iterator<Item = Self::DataToken>>,
            ),
        >,
    > {
        match current_type_name.as_ref() {
            "Crate" => {
                match edge_name.as_ref() {
                    "item" => Box::new(data_contexts.map(move |ctx| {
                        let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match &ctx
                            .current_token
                        {
                            None => Box::new(std::iter::empty()),
                            Some(token) => {
                                let crate_token = token.as_crate().expect("token was not a Crate");
                                let iter = crate_token
                                    .index
                                    .clone()
                                    .into_values()
                                    .filter(|item| {
                                        // Filter out item types that are not currently supported.
                                        matches!(
                                            item.inner,
                                            rustdoc_types::ItemEnum::Struct(_)
                                                | rustdoc_types::ItemEnum::StructField(_)
                                        )
                                    })
                                    .map(|value| (&value).into());
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
            "Item" | "Struct" | "StructField" if edge_name.as_ref() == "span" => {
                Box::new(data_contexts.map(move |ctx| {
                    let neighbors: Box<dyn Iterator<Item = Self::DataToken>> =
                        match &ctx.current_token {
                            None => Box::new(std::iter::empty()),
                            Some(token) => {
                                let item = token.as_item().expect("token was not an Item");
                                if let Some(span) = &item.span {
                                    Box::new(std::iter::once(span.into()))
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
                    let starting_crate = self.starting_crate.clone();
                    Box::new(data_contexts.map(move |ctx| {
                        let neighbors: Box<dyn Iterator<Item = Self::DataToken>> =
                            match &ctx.current_token {
                                None => Box::new(std::iter::empty()),
                                Some(token) => {
                                    let (_, struct_item) =
                                        token.as_struct_item().expect("token was not a Struct");
                                    let item_index = starting_crate.index.clone();
                                    Box::new(struct_item.fields.clone().into_iter().map(
                                        move |field_id| {
                                            item_index.get(&field_id).expect("missing item").into()
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
            _ => unreachable!("project_neighbors {current_type_name} {edge_name} {parameters:?}"),
        }
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)>> {
        match current_type_name.as_ref() {
            "Item" => {
                Box::new(data_contexts.map(move |ctx| {
                    let can_coerce = match &ctx.current_token {
                        None => false,
                        Some(token) => {
                            let actual_type_name = match token {
                                Token::Item(item) => match &item.inner {
                                    rustdoc_types::ItemEnum::Struct(_) => "Struct",
                                    rustdoc_types::ItemEnum::StructField(_) => "StructField",
                                    _ => {
                                        unreachable!("unexpected item.inner type: {:?}", item.inner)
                                    }
                                },
                                _ => unreachable!("unexpected token type: {token:?}"),
                            };

                            // We can compare the actual type name to the attempted coercion type
                            // since the inheritance hierarchy only has one level.
                            //
                            // If more layers of interfaces are added, this logic will
                            // need to change.
                            actual_type_name == coerce_to_type_name.as_ref()
                        }
                    };

                    (ctx, can_coerce)
                }))
            }
            _ => unreachable!("can_coerce_to_type {current_type_name} {coerce_to_type_name}"),
        }
    }
}
