#![allow(dead_code)]

use std::sync::Arc;

use hn_api::{types::Item, HnClient};
use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
};

use crate::token::Token;

lazy_static! {
    static ref CLIENT: HnClient = HnClient::init().unwrap();
}

pub struct HackerNewsAdapter;

impl HackerNewsAdapter {
    fn front_page(&self) -> Box<dyn Iterator<Item = Token>> {
        self.top(Some(30))
    }

    fn top(&self, max: Option<usize>) -> Box<dyn Iterator<Item = Token>> {
        let iterator = CLIENT
            .get_top_stories()
            .unwrap()
            .into_iter()
            .take(max.unwrap_or(usize::MAX))
            .filter_map(|id| match CLIENT.get_item(id) {
                Ok(maybe_item) => maybe_item.map(|item| item.into()),
                Err(e) => {
                    eprintln!("Got an error while fetching item: {}", e);
                    None
                }
            });

        Box::new(iterator)
    }

    fn latest_stories(&self, max: Option<usize>) -> Box<dyn Iterator<Item = Token>> {
        // Unfortunately, the HN crate we're using doesn't support getting the new stories,
        // so we're doing it manually here.
        let story_ids: Vec<u32> =
            reqwest::blocking::get("https://hacker-news.firebaseio.com/v0/newstories.json")
                .unwrap()
                .json()
                .unwrap();

        let iterator = story_ids
            .into_iter()
            .take(max.unwrap_or(usize::MAX))
            .map(move |id| CLIENT.get_item(id))
            .filter_map(|res| match res {
                Ok(maybe_item) => maybe_item.map(|item| item.into()),
                Err(e) => {
                    eprintln!("Got an error while fetching item: {}", e);
                    None
                }
            });

        Box::new(iterator)
    }
}

macro_rules! impl_item_property {
    ($data_contexts:ident, $attr:ident) => {
        Box::new($data_contexts.map(|ctx| {
            let token = ctx.current_token.as_ref();
            let value = match token {
                None => FieldValue::Null,
                Some(t) => {
                    if let Some(s) = t.as_story() {
                        (&s.$attr).into()
                    } else if let Some(j) = t.as_job() {
                        (&j.$attr).into()
                    } else {
                        unreachable!()
                    }
                }

                #[allow(unreachable_patterns)]
                _ => unreachable!(),
            };

            (ctx, value)
        }))
    };
}

macro_rules! impl_property {
    ($data_contexts:ident, $token_type:path, $attr:ident) => {
        Box::new($data_contexts.map(|ctx| {
            let token = ctx.current_token.as_ref();
            let value = match token {
                None => FieldValue::Null,
                Some($token_type(t)) => (&t.$attr).into(),

                #[allow(unreachable_patterns)]
                _ => unreachable!(),
            };

            (ctx, value)
        }))
    };

    ($data_contexts:ident, $token_type:path, $var:ident, $b:block) => {
        Box::new($data_contexts.map(|ctx| {
            let token = ctx.current_token.as_ref();
            let value = match token {
                None => FieldValue::Null,
                Some($token_type($var)) => $b,

                #[allow(unreachable_patterns)]
                _ => unreachable!(),
            };

            (ctx, value)
        }))
    };
}

impl Adapter<'static> for HackerNewsAdapter {
    type DataToken = Token;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken>> {
        match edge.as_ref() {
            "FrontPage" => self.front_page(),
            "Top" => {
                // TODO: This is unergonomic, build a more convenient API here.
                let max = parameters
                    .unwrap()
                    .0
                    .get("max")
                    .map(|v| v.as_u64().unwrap() as usize);
                self.top(max)
            }
            "LatestStory" => {
                // TODO: This is unergonomic, build a more convenient API here.
                let max = parameters
                    .unwrap()
                    .0
                    .get("max")
                    .map(|v| v.as_u64().unwrap() as usize);
                self.latest_stories(max)
            }
            _ => unreachable!(),
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
        match (current_type_name.as_ref(), field_name.as_ref()) {
            // properties on Item and its implementers
            ("Item" | "Story" | "Job", "id") => impl_item_property!(data_contexts, id),
            ("Item" | "Story" | "Job", "score") => impl_item_property!(data_contexts, score),
            ("Item" | "Story" | "Job", "title") => impl_item_property!(data_contexts, title),
            ("Item" | "Story" | "Job", "unixTime") => impl_item_property!(data_contexts, time),
            ("Item" | "Story" | "Job", "url") => impl_item_property!(data_contexts, url),

            // properties on Story
            ("Story", "byUsername") => impl_property!(data_contexts, Token::Story, by),
            ("Story", "text") => impl_property!(data_contexts, Token::Story, text),
            ("Story", "commentsCount") => impl_property!(data_contexts, Token::Story, descendants),

            // properties on Comment
            ("Comment", "byUsername") => impl_property!(data_contexts, Token::Comment, by),
            ("Comment", "id") => impl_property!(data_contexts, Token::Comment, id),
            ("Comment", "text") => impl_property!(data_contexts, Token::Comment, text),
            ("Comment", "unixTime") => impl_property!(data_contexts, Token::Comment, time),
            ("Comment", "childCount") => impl_property!(data_contexts, Token::Comment, comment, {
                comment
                    .kids
                    .as_ref()
                    .map(|v| v.len() as u64)
                    .unwrap_or(0)
                    .into()
            }),

            // properties on User
            ("User", "id") => impl_property!(data_contexts, Token::User, id),
            ("User", "karma") => impl_property!(data_contexts, Token::User, karma),
            ("User", "about") => impl_property!(data_contexts, Token::User, about),
            ("User", "unixCreatedAt") => impl_property!(data_contexts, Token::User, created),
            ("User", "delay") => impl_property!(data_contexts, Token::User, delay),
            _ => unreachable!(),
        }
    }

    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
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
                Box<dyn Iterator<Item = Self::DataToken>>,
            ),
        >,
    > {
        match (current_type_name.as_ref(), edge_name.as_ref()) {
            ("Story", "byUser") => Box::new(data_contexts.map(|ctx| {
                let token = &ctx.current_token;
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let story = token.as_story().unwrap();
                        let author = story.by.as_str();
                        match CLIENT.get_user(author) {
                            Ok(None) => Box::new(std::iter::empty()), // no known author
                            Ok(Some(user)) => Box::new(std::iter::once(user.into())),
                            Err(e) => {
                                eprintln!(
                                    "API error while fetching story {} author \"{}\": {}",
                                    story.id, author, e
                                );
                                Box::new(std::iter::empty())
                            }
                        }
                    }
                };

                (ctx, neighbors)
            })),
            ("Story", "comment") => Box::new(data_contexts.map(|ctx| {
                let token = &ctx.current_token;
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let story = token.as_story().unwrap();
                        let comment_ids = story.kids.clone().unwrap_or_default();
                        let story_id = story.id;

                        let neighbors_iter =
                            comment_ids.into_iter().filter_map(move |comment_id| {
                                match CLIENT.get_item(comment_id) {
                                    Ok(None) => None,
                                    Ok(Some(item)) => {
                                        if let Item::Comment(comment) = item {
                                            Some(comment.into())
                                        } else {
                                            unreachable!()
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "API error while fetching story {} comment {}: {}",
                                            story_id, comment_id, e
                                        );
                                        None
                                    }
                                }
                            });

                        Box::new(neighbors_iter)
                    }
                };

                (ctx, neighbors)
            })),
            ("Comment", "byUser") => Box::new(data_contexts.map(|ctx| {
                let token = &ctx.current_token;
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let comment = token.as_comment().unwrap();
                        let author = comment.by.as_str();
                        match CLIENT.get_user(author) {
                            Ok(None) => Box::new(std::iter::empty()), // no known author
                            Ok(Some(user)) => Box::new(std::iter::once(user.into())),
                            Err(e) => {
                                eprintln!(
                                    "API error while fetching comment {} author \"{}\": {}",
                                    comment.id, author, e
                                );
                                Box::new(std::iter::empty())
                            }
                        }
                    }
                };

                (ctx, neighbors)
            })),
            ("Comment", "reply") => Box::new(data_contexts.map(|ctx| {
                let token = ctx.current_token.clone();
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let comment = token.as_comment().unwrap();
                        let comment_id = comment.id;
                        let reply_ids = comment.kids.clone().unwrap_or_default();

                        Box::new(reply_ids.into_iter().filter_map(move |reply_id| {
                            match CLIENT.get_item(reply_id) {
                                Ok(None) => None,
                                Ok(Some(item)) => {
                                    if let Item::Comment(c) = item {
                                        Some(c.into())
                                    } else {
                                        unreachable!()
                                    }
                                }
                                Err(e) => {
                                    eprintln!(
                                        "API error while fetching comment {} reply {}: {}",
                                        comment_id, reply_id, e
                                    );
                                    None
                                }
                            }
                        }))
                    }
                };

                (ctx, neighbors)
            })),
            _ => unreachable!("{} {}", current_type_name.as_ref(), edge_name.as_ref()),
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
        let iterator = data_contexts.map(move |ctx| {
            let token = match &ctx.current_token {
                Some(t) => t,
                None => return (ctx, false),
            };

            // Possible optimization here:
            // This "match" is loop-invariant, and can be hoisted outside the map() call
            // at the cost of a bit of code repetition.

            let can_coerce = match (current_type_name.as_ref(), coerce_to_type_name.as_ref()) {
                ("Item", "Job") => token.as_job().is_some(),
                ("Item", "Story") => token.as_story().is_some(),
                _ => unreachable!(),
            };

            (ctx, can_coerce)
        });

        Box::new(iterator)
    }
}
