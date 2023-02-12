#![allow(dead_code)]

use hn_api::{types::Item, HnClient};
use trustfall_core::{
    field_property,
    interpreter::{
        basic_adapter::BasicAdapter,
        helpers::{resolve_coercion_with, resolve_neighbors_with, resolve_property_with},
        ContextIterator, ContextOutcomeIterator, VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
};

use crate::token::Token;

lazy_static! {
    static ref CLIENT: HnClient = HnClient::init().unwrap();
}

pub struct HackerNewsAdapter;

impl HackerNewsAdapter {
    fn front_page(&self) -> VertexIterator<'static, Token> {
        self.top(Some(30))
    }

    fn top(&self, max: Option<usize>) -> VertexIterator<'static, Token> {
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

    fn latest_stories(&self, max: Option<usize>) -> VertexIterator<'static, Token> {
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

    fn user(&self, username: &str) -> VertexIterator<'static, Token> {
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

macro_rules! item_property_resolver {
    ($attr:ident) => {
        |vertex| -> FieldValue {
            if let Some(s) = vertex.as_story() {
                (&s.$attr).into()
            } else if let Some(j) = vertex.as_job() {
                (&j.$attr).into()
            } else if let Some(c) = vertex.as_comment() {
                (&c.$attr).into()
            } else {
                unreachable!()
            }
        }
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
            ("Item" | "Story" | "Job" | "Comment", "id") => {
                resolve_property_with(contexts, item_property_resolver!(id))
            }
            ("Item" | "Story" | "Job" | "Comment", "unixTime") => {
                resolve_property_with(contexts, item_property_resolver!(time))
            }

            // properties on Job
            ("Job", "score") => resolve_property_with(contexts, field_property!(as_job, score)),
            ("Job", "title") => resolve_property_with(contexts, field_property!(as_job, title)),
            ("Job", "url") => resolve_property_with(contexts, field_property!(as_job, url)),

            // properties on Story
            ("Story", "byUsername") => {
                resolve_property_with(contexts, field_property!(as_story, by))
            }
            ("Story", "text") => resolve_property_with(contexts, field_property!(as_story, text)),
            ("Story", "commentsCount") => {
                resolve_property_with(contexts, field_property!(as_story, descendants))
            }
            ("Story", "score") => resolve_property_with(contexts, field_property!(as_story, score)),
            ("Story", "title") => resolve_property_with(contexts, field_property!(as_story, title)),
            ("Story", "url") => resolve_property_with(contexts, field_property!(as_story, url)),

            // properties on Comment
            ("Comment", "byUsername") => {
                resolve_property_with(contexts, field_property!(as_comment, by))
            }
            ("Comment", "text") => {
                resolve_property_with(contexts, field_property!(as_comment, text))
            }
            ("Comment", "childCount") => resolve_property_with(
                contexts,
                field_property!(as_comment, kids, {
                    kids.as_ref().map(|v| v.len() as u64).unwrap_or(0).into()
                }),
            ),

            // properties on User
            ("User", "id") => resolve_property_with(contexts, field_property!(as_user, id)),
            ("User", "karma") => resolve_property_with(contexts, field_property!(as_user, karma)),
            ("User", "about") => resolve_property_with(contexts, field_property!(as_user, about)),
            ("User", "unixCreatedAt") => {
                resolve_property_with(contexts, field_property!(as_user, created))
            }
            ("User", "delay") => resolve_property_with(contexts, field_property!(as_user, delay)),
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
                resolve_neighbors_with(contexts, edge_resolver)
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
                resolve_neighbors_with(contexts, edge_resolver)
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
                resolve_neighbors_with(contexts, edge_resolver)
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
                resolve_neighbors_with(contexts, edge_resolver)
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
                resolve_neighbors_with(contexts, edge_resolver)
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
                resolve_neighbors_with(contexts, edge_resolver)
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
        match (type_name, coerce_to_type) {
            ("Item", "Job") => resolve_coercion_with(contexts, |token| token.as_job().is_some()),
            ("Item", "Story") => {
                resolve_coercion_with(contexts, |token| token.as_story().is_some())
            }
            ("Item", "Comment") => {
                resolve_coercion_with(contexts, |token| token.as_comment().is_some())
            }
            _ => unreachable!(),
        }
    }
}
