#![allow(dead_code)]

use hn_api::{types::Item, HnClient};
use trustfall_core::{
    interpreter::basic_adapter::{
        BasicAdapter, ContextIterator, ContextOutcomeIterator, VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
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
                    eprintln!("Got an error while fetching item: {e}");
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
                    eprintln!("Got an error while fetching item: {e}");
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
                eprintln!("Got an error while getting user profile for user {username}: {e}",);
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

impl BasicAdapter<'static> for HackerNewsAdapter {
    type Vertex = Token;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &str,
        parameters: Option<&EdgeParameters>,
    ) -> VertexIterator<'static, Self::Vertex> {
        match edge_name {
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

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        match (type_name, property_name) {
            // properties on Item and its implementers
            ("Item" | "Story" | "Job" | "Comment", "id") => impl_item_property!(contexts, id),
            ("Item" | "Story" | "Job" | "Comment", "unixTime") => {
                impl_item_property!(contexts, time)
            }

            // properties on Job
            ("Job", "score") => impl_property!(contexts, as_job, score),
            ("Job", "title") => impl_property!(contexts, as_job, title),
            ("Job", "url") => impl_property!(contexts, as_job, url),

            // properties on Story
            ("Story", "byUsername") => impl_property!(contexts, as_story, by),
            ("Story", "text") => impl_property!(contexts, as_story, text),
            ("Story", "commentsCount") => impl_property!(contexts, as_story, descendants),
            ("Story", "score") => impl_property!(contexts, as_story, score),
            ("Story", "title") => impl_property!(contexts, as_story, title),
            ("Story", "url") => impl_property!(contexts, as_story, url),

            // properties on Comment
            ("Comment", "byUsername") => impl_property!(contexts, as_comment, by),
            ("Comment", "text") => impl_property!(contexts, as_comment, text),
            ("Comment", "childCount") => impl_property!(contexts, as_comment, comment, {
                comment
                    .kids
                    .as_ref()
                    .map(|v| v.len() as u64)
                    .unwrap_or(0)
                    .into()
            }),

            // properties on User
            ("User", "id") => impl_property!(contexts, as_user, id),
            ("User", "karma") => impl_property!(contexts, as_user, karma),
            ("User", "about") => impl_property!(contexts, as_user, about),
            ("User", "unixCreatedAt") => impl_property!(contexts, as_user, created),
            ("User", "delay") => impl_property!(contexts, as_user, delay),
            _ => unreachable!(),
        }
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        edge_name: &str,
        _parameters: Option<&EdgeParameters>,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        fn resolve_neighbors_inner(
            contexts: ContextIterator<'static, Token>,
            edge_resolver: impl Fn(&Token) -> VertexIterator<'static, Token> + 'static,
        ) -> ContextOutcomeIterator<'static, Token, VertexIterator<'static, Token>> {
            Box::new(contexts.map(move |ctx| match ctx.current_token.as_ref() {
                None => {
                    let no_neighbors: VertexIterator<'static, Token> = Box::new(std::iter::empty());
                    (ctx, no_neighbors)
                }
                Some(token) => {
                    let neighbors = edge_resolver(token);
                    (ctx, neighbors)
                }
            }))
        }

        match (type_name, edge_name) {
            ("Story", "byUser") => {
                let edge_resolver =
                    |token: &Self::Vertex| -> VertexIterator<'static, Self::Vertex> {
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
                    };
                resolve_neighbors_inner(contexts, edge_resolver)
            }
            ("Story", "comment") => {
                let edge_resolver = |token: &Self::Vertex| {
                    let story = token.as_story().unwrap();
                    let comment_ids = story.kids.clone().unwrap_or_default();
                    let story_id = story.id;

                    let neighbors: VertexIterator<'static, Self::Vertex> =
                        Box::new(comment_ids.into_iter().filter_map(move |comment_id| {
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
                                        "API error while fetching story {story_id} comment {comment_id}: {e}",
                                    );
                                    None
                                }
                            }
                        }));

                    neighbors
                };
                resolve_neighbors_inner(contexts, edge_resolver)
            }
            ("Comment", "byUser") => {
                let edge_resolver = |token: &Self::Vertex| {
                    let comment = token.as_comment().unwrap();
                    let author = comment.by.as_str();
                    let neighbors: VertexIterator<'static, Self::Vertex> =
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
                        };
                    neighbors
                };
                resolve_neighbors_inner(contexts, edge_resolver)
            }
            ("Comment", "parent") => {
                let edge_resolver = |token: &Self::Vertex| {
                    let comment = token.as_comment().unwrap();
                    let comment_id = comment.id;
                    let parent_id = comment.parent;

                    let neighbors: VertexIterator<'static, Self::Vertex> = match CLIENT
                        .get_item(parent_id)
                    {
                        Ok(None) => Box::new(std::iter::empty()),
                        Ok(Some(item)) => Box::new(std::iter::once(item.into())),
                        Err(e) => {
                            eprintln!(
                                "API error while fetching comment {comment_id} parent {parent_id}: {e}",
                            );
                            Box::new(std::iter::empty())
                        }
                    };
                    neighbors
                };
                resolve_neighbors_inner(contexts, edge_resolver)
            }
            ("Comment", "reply") => {
                let edge_resolver = |token: &Self::Vertex| {
                    let comment = token.as_comment().unwrap();
                    let comment_id = comment.id;
                    let reply_ids = comment.kids.clone().unwrap_or_default();

                    let neighbors: VertexIterator<'static, Self::Vertex> = Box::new(reply_ids.into_iter().filter_map(move |reply_id| {
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
                                    "API error while fetching comment {comment_id} reply {reply_id}: {e}",
                                );
                                None
                            }
                        }
                    }));
                    neighbors
                };
                resolve_neighbors_inner(contexts, edge_resolver)
            }
            ("User", "submitted") => {
                let edge_resolver = |token: &Self::Vertex| {
                    let user = token.as_user().unwrap();
                    let submitted_ids = user.submitted.clone();

                    let neighbors: VertexIterator<'static, Self::Vertex> =
                        Box::new(submitted_ids.into_iter().filter_map(move |submission_id| {
                            match CLIENT.get_item(submission_id) {
                                Ok(None) => None,
                                Ok(Some(item)) => Some(item.into()),
                                Err(e) => {
                                    eprintln!(
                                    "API error while fetching submitted item {submission_id}: {e}",
                                );
                                    None
                                }
                            }
                        }));
                    neighbors
                };
                resolve_neighbors_inner(contexts, edge_resolver)
            }
            _ => unreachable!("{} {}", type_name, edge_name),
        }
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        // The coercion check always looks structurally the same,
        // so let's extract that logic into a function parameterized only by
        // the closure that checks whether the vertex matches the new type or not.
        fn apply_coercion(
            contexts: ContextIterator<'static, Token>,
            coercion_check: impl Fn(&Token) -> bool + 'static,
        ) -> ContextOutcomeIterator<'static, Token, bool> {
            Box::new(contexts.map(move |ctx| {
                let token = match &ctx.current_token {
                    Some(t) => t,
                    None => return (ctx, false),
                };

                let can_coerce = coercion_check(token);

                (ctx, can_coerce)
            }))
        }

        match (type_name, coerce_to_type) {
            ("Item", "Job") => apply_coercion(contexts, |token| token.as_job().is_some()),
            ("Item", "Story") => apply_coercion(contexts, |token| token.as_story().is_some()),
            ("Item", "Comment") => apply_coercion(contexts, |token| token.as_comment().is_some()),
            _ => unreachable!(),
        }
    }
}
