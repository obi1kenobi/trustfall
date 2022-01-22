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

    fn user(&self, username: &str) -> Box<dyn Iterator<Item = Token>> {
        match CLIENT.get_user(username) {
            Ok(Some(user)) => {
                // Found a user by that name.
                let token = Token::from(user);
                Box::new(std::iter::once(token))
            }
            Ok(None) => {
                // The request succeeded but did not find a user by that name.
                Box::new(std::iter::empty())
            }
            Err(e) => {
                eprintln!(
                    "Got an error while getting user profile for user {}: {}",
                    username, e
                );
                Box::new(std::iter::empty())
            }
        }
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
                    } else if let Some(c) = t.as_comment() {
                        (&c.$attr).into()
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
    ($data_contexts:ident, $conversion:ident, $attr:ident) => {
        Box::new($data_contexts.map(|ctx| {
            let token = ctx
                .current_token
                .as_ref()
                .map(|token| token.$conversion().unwrap());
            let value = match token {
                None => FieldValue::Null,
                Some(t) => (&t.$attr).into(),

                #[allow(unreachable_patterns)]
                _ => unreachable!(),
            };

            (ctx, value)
        }))
    };

    ($data_contexts:ident, $conversion:ident, $var:ident, $b:block) => {
        Box::new($data_contexts.map(|ctx| {
            let token = ctx
                .current_token
                .as_ref()
                .map(|token| token.$conversion().unwrap());
            let value = match token {
                None => FieldValue::Null,
                Some($var) => $b,

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
            "User" => {
                let username_value = parameters.as_ref().unwrap().0.get("name").unwrap();
                self.user(username_value.as_str().unwrap())
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
            ("Item" | "Story" | "Job" | "Comment", "id") => impl_item_property!(data_contexts, id),
            ("Item" | "Story" | "Job" | "Comment", "unixTime") => {
                impl_item_property!(data_contexts, time)
            }

            // properties on Job
            ("Job", "score") => impl_property!(data_contexts, as_job, score),
            ("Job", "title") => impl_property!(data_contexts, as_job, title),
            ("Job", "url") => impl_property!(data_contexts, as_job, url),

            // properties on Story
            ("Story", "byUsername") => impl_property!(data_contexts, as_story, by),
            ("Story", "text") => impl_property!(data_contexts, as_story, text),
            ("Story", "commentsCount") => impl_property!(data_contexts, as_story, descendants),
            ("Story", "score") => impl_property!(data_contexts, as_story, score),
            ("Story", "title") => impl_property!(data_contexts, as_story, title),
            ("Story", "url") => impl_property!(data_contexts, as_story, url),

            // properties on Comment
            ("Comment", "byUsername") => impl_property!(data_contexts, as_comment, by),
            ("Comment", "text") => impl_property!(data_contexts, as_comment, text),
            ("Comment", "childCount") => impl_property!(data_contexts, as_comment, comment, {
                comment
                    .kids
                    .as_ref()
                    .map(|v| v.len() as u64)
                    .unwrap_or(0)
                    .into()
            }),

            // properties on User
            ("User", "id") => impl_property!(data_contexts, as_user, id),
            ("User", "karma") => impl_property!(data_contexts, as_user, karma),
            ("User", "about") => impl_property!(data_contexts, as_user, about),
            ("User", "unixCreatedAt") => impl_property!(data_contexts, as_user, created),
            ("User", "delay") => impl_property!(data_contexts, as_user, delay),
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
            ("Comment", "parent") => Box::new(data_contexts.map(|ctx| {
                let token = ctx.current_token.clone();
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let comment = token.as_comment().unwrap();
                        let comment_id = comment.id;
                        let parent_id = comment.parent;

                        match CLIENT.get_item(parent_id) {
                            Ok(None) => Box::new(std::iter::empty()),
                            Ok(Some(item)) => Box::new(std::iter::once(item.into())),
                            Err(e) => {
                                eprintln!(
                                    "API error while fetching comment {} parent {}: {}",
                                    comment_id, parent_id, e
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
            ("User", "submitted") => Box::new(data_contexts.map(|ctx| {
                let token = ctx.current_token.clone();
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let user = token.as_user().unwrap();
                        let submitted_ids = user.submitted.clone();

                        Box::new(submitted_ids.into_iter().filter_map(move |submission_id| {
                            match CLIENT.get_item(submission_id) {
                                Ok(None) => None,
                                Ok(Some(item)) => Some(item.into()),
                                Err(e) => {
                                    eprintln!(
                                        "API error while fetching submitted item {}: {}",
                                        submission_id, e
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
                ("Item", "Comment") => token.as_comment().is_some(),
                _ => unreachable!(),
            };

            (ctx, can_coerce)
        });

        Box::new(iterator)
    }
}
