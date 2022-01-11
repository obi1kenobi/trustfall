use std::sync::Arc;

use feed_rs::model::{Content, Entry, Feed, FeedType, Image, Link, Text};
use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
    project_neighbors, project_property,
};

#[derive(Debug)]
pub(crate) struct FeedAdapter<'a> {
    data: &'a [Feed],
}

impl<'a> FeedAdapter<'a> {
    pub(crate) fn new(data: &'a [Feed]) -> Self {
        Self { data }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Token<'a> {
    Feed(&'a Feed),
    FeedText(&'a Text),
    ChannelImage(&'a Image),
    FeedLink(&'a Link),
    FeedEntry(&'a Entry),
    FeedContent(&'a Content),
}

impl<'a> Adapter<'a> for FeedAdapter<'a> {
    type DataToken = Token<'a>;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        _parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'a> {
        match edge.as_ref() {
            "Feed" => Box::new(self.data.iter().map(Token::Feed)),
            "FeedAtUrl" => {
                todo!()
            }
            _ => unimplemented!("{}", edge),
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
        project_property! {
            data_contexts, current_type_name, field_name, [
                {
                    Feed, Token::Feed, [
                        id,
                        updated,
                        language,
                        published,
                        ttl,
                        (feedType, token, {
                            let value = match token.feed_type {
                                FeedType::Atom => "Atom",
                                FeedType::JSON => "JSON",
                                FeedType::RSS0 => "RSS0",
                                FeedType::RSS1 => "RSS1",
                                FeedType::RSS2 => "RSS2",
                            };
                            value.to_owned().into()
                        }),
                    ],
                },
                {
                    FeedText, Token::FeedText, [
                        content,
                        src,
                        (contentType, token, {
                            token.content_type.essence_str().to_owned().into()
                        }),
                    ],
                },
                {
                    FeedEntry, Token::FeedEntry, [
                        id,
                        source,
                        updated,
                        published,
                    ],
                },
                {
                    FeedContent, Token::FeedContent, [
                        body,
                        length,
                        (contentType, token, {
                            token.content_type.essence_str().to_owned().into()
                        }),
                    ]
                },
                {
                    FeedLink, Token::FeedLink, [
                        href,
                        rel,
                        media_type,
                        href_lang,
                        title,
                        length,
                    ]
                },
                {
                    ChannelImage, Token::ChannelImage, [
                        uri,
                        title,
                        width,
                        height,
                        description,
                    ],
                },
            ],
        }
    }

    #[allow(clippy::type_complexity)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        _parameters: Option<Arc<EdgeParameters>>,
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
        project_neighbors!(data_contexts, 'a, current_type_name, edge_name, [
            {
                Feed, Token::Feed, [
                    (title, Token::FeedText),
                    (description, Token::FeedText),
                    (rights, Token::FeedText),
                    (icon, Token::ChannelImage),
                    (links, Token::FeedLink),
                    (entries, Token::FeedEntry),
                ],
            }, {
                FeedEntry, Token::FeedEntry, [
                    (title, Token::FeedText),
                    (content, Token::FeedContent),
                    (links, Token::FeedLink),
                    (summary, Token::FeedText),
                    (rights, Token::FeedText),
                ],
            }, {
                FeedContent, Token::FeedContent, [
                    (src, Token::FeedLink),
                ],
            }, {
                ChannelImage, Token::ChannelImage, [
                    (link, Token::FeedLink),
                ],
            }
        ])
    }

    fn can_coerce_to_type(
        &mut self,
        _data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'a> {
        unimplemented!("{} -> {}", current_type_name, coerce_to_type_name)
    }
}
