use feed_rs::model::{Content, Entry, Feed, FeedType, Image, Link, Text};
use trustfall_core::{
    field_property,
    interpreter::{
        basic_adapter::BasicAdapter,
        helpers::{resolve_neighbors_with as neighbors, resolve_property_with as property},
        ContextIterator, ContextOutcomeIterator, Typename, VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
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
pub(crate) enum Vertex<'a> {
    Feed(&'a Feed),
    FeedText(&'a Text),
    ChannelImage(&'a Image),
    FeedLink(&'a Link),
    FeedEntry(&'a Entry),
    FeedContent(&'a Content),
}

macro_rules! impl_downcast {
    ($name:ident, $output:ident, $arm:path) => {
        fn $name(&self) -> Option<&'a $output> {
            match self {
                $arm(x) => Some(*x),
                _ => None,
            }
        }
    };
}

impl<'a> Vertex<'a> {
    impl_downcast!(as_feed, Feed, Self::Feed);
    impl_downcast!(as_feed_text, Text, Self::FeedText);
    impl_downcast!(as_channel_image, Image, Self::ChannelImage);
    impl_downcast!(as_feed_link, Link, Self::FeedLink);
    impl_downcast!(as_feed_entry, Entry, Self::FeedEntry);
    impl_downcast!(as_feed_content, Content, Self::FeedContent);
}

impl<'a> Typename for Vertex<'a> {
    fn typename(&self) -> &'static str {
        match self {
            Vertex::Feed(..) => "Feed",
            Vertex::FeedText(..) => "FeedText",
            Vertex::ChannelImage(..) => "ChannelImage",
            Vertex::FeedLink(..) => "FeedLink",
            Vertex::FeedEntry(..) => "FeedEntry",
            Vertex::FeedContent(..) => "FeedContent",
        }
    }
}

macro_rules! iterable {
    ($conversion:ident, $field:ident, $neighbor_variant:path) => {
        |vertex| -> VertexIterator<'a, Self::Vertex> {
            let vertex = vertex.$conversion().expect("conversion failed");
            let neighbors = vertex.$field.iter().map($neighbor_variant);
            Box::new(neighbors)
        }
    };
}

impl<'a> BasicAdapter<'a> for FeedAdapter<'a> {
    type Vertex = Vertex<'a>;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &str,
        _parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            "Feed" => Box::new(self.data.iter().map(Vertex::Feed)),
            "FeedAtUrl" => {
                todo!()
            }
            _ => unimplemented!("{}", edge_name),
        }
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
        match type_name {
            "Feed" => match property_name {
                "id" => property(contexts, field_property!(as_feed, id)),
                "updated" => property(contexts, field_property!(as_feed, updated)),
                "language" => property(contexts, field_property!(as_feed, language)),
                "published" => property(contexts, field_property!(as_feed, published)),
                "ttl" => property(contexts, field_property!(as_feed, ttl)),
                "feed_type" => property(
                    contexts,
                    field_property!(as_feed, feed_type, {
                        let value = match feed_type {
                            FeedType::Atom => "Atom",
                            FeedType::JSON => "JSON",
                            FeedType::RSS0 => "RSS0",
                            FeedType::RSS1 => "RSS1",
                            FeedType::RSS2 => "RSS2",
                        };
                        value.to_owned().into()
                    }),
                ),
                _ => unreachable!("type {type_name} property {property_name} not found"),
            },
            "FeedText" => match property_name {
                "content" => property(contexts, field_property!(as_feed_text, content)),
                "src" => property(contexts, field_property!(as_feed_text, src)),
                "content_type" => property(
                    contexts,
                    field_property!(as_feed_text, content_type, {
                        content_type.essence_str().to_owned().into()
                    }),
                ),
                _ => unreachable!("type {type_name} property {property_name} not found"),
            },
            "FeedEntry" => match property_name {
                "id" => property(contexts, field_property!(as_feed_entry, id)),
                "source" => property(contexts, field_property!(as_feed_entry, source)),
                "updated" => property(contexts, field_property!(as_feed_entry, updated)),
                "published" => property(contexts, field_property!(as_feed_entry, published)),
                _ => unreachable!("type {type_name} property {property_name} not found"),
            },
            "FeedContent" => match property_name {
                "body" => property(contexts, field_property!(as_feed_content, body)),
                "length" => property(contexts, field_property!(as_feed_content, length)),
                "content_type" => property(
                    contexts,
                    field_property!(as_feed_content, content_type, {
                        content_type.essence_str().to_owned().into()
                    }),
                ),
                _ => unreachable!("type {type_name} property {property_name} not found"),
            },
            "FeedLink" => match property_name {
                "href" => property(contexts, field_property!(as_feed_link, href)),
                "rel" => property(contexts, field_property!(as_feed_link, rel)),
                "media_type" => property(contexts, field_property!(as_feed_link, media_type)),
                "href_lang" => property(contexts, field_property!(as_feed_link, href_lang)),
                "title" => property(contexts, field_property!(as_feed_link, title)),
                "length" => property(contexts, field_property!(as_feed_link, length)),
                _ => unreachable!("type {type_name} property {property_name} not found"),
            },
            "ChannelImage" => match property_name {
                "uri" => property(contexts, field_property!(as_channel_image, uri)),
                "title" => property(contexts, field_property!(as_channel_image, title)),
                "width" => property(contexts, field_property!(as_channel_image, width)),
                "height" => property(contexts, field_property!(as_channel_image, height)),
                "description" => property(contexts, field_property!(as_channel_image, description)),
                _ => unreachable!("type {type_name} property {property_name} not found"),
            },
            _ => unreachable!("type {type_name} not found"),
        }
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        edge_name: &str,
        _parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
        match type_name {
            "Feed" => match edge_name {
                "title" => neighbors(contexts, iterable!(as_feed, title, Vertex::FeedText)),
                "description" => {
                    neighbors(contexts, iterable!(as_feed, description, Vertex::FeedText))
                }
                "rights" => neighbors(contexts, iterable!(as_feed, rights, Vertex::FeedText)),
                "icon" => neighbors(contexts, iterable!(as_feed, icon, Vertex::ChannelImage)),
                "links" => neighbors(contexts, iterable!(as_feed, links, Vertex::FeedLink)),
                "entries" => neighbors(contexts, iterable!(as_feed, entries, Vertex::FeedEntry)),
                _ => unreachable!("type {type_name} edge {edge_name} not found"),
            },
            "FeedEntry" => match edge_name {
                "title" => neighbors(contexts, iterable!(as_feed_entry, title, Vertex::FeedText)),
                "content" => neighbors(
                    contexts,
                    iterable!(as_feed_entry, content, Vertex::FeedContent),
                ),
                "links" => neighbors(contexts, iterable!(as_feed_entry, links, Vertex::FeedLink)),
                "summary" => neighbors(
                    contexts,
                    iterable!(as_feed_entry, summary, Vertex::FeedText),
                ),
                "rights" => neighbors(contexts, iterable!(as_feed_entry, rights, Vertex::FeedText)),
                _ => unreachable!("type {type_name} edge {edge_name} not found"),
            },
            "FeedContent" => match edge_name {
                "src" => neighbors(contexts, iterable!(as_feed_content, src, Vertex::FeedLink)),
                _ => unreachable!("type {type_name} edge {edge_name} not found"),
            },
            "ChannelImage" => match edge_name {
                "link" => neighbors(
                    contexts,
                    iterable!(as_channel_image, link, Vertex::FeedLink),
                ),
                _ => unreachable!("type {type_name} edge {edge_name} not found"),
            },
            _ => unreachable!("type {type_name} not found"),
        }
    }

    fn resolve_coercion(
        &mut self,
        _contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        unimplemented!("{type_name} -> {coerce_to_type}")
    }
}
