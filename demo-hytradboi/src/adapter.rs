#![allow(dead_code)]

use std::{rc::Rc, sync::Arc};

use consecrates::{api::Crate, Query, Sorting};
use git_url_parse::GitUrl;
use hn_api::{types::Item, HnClient};
use lazy_static::__Deref;
use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
};

use crate::token::Token;

const USER_AGENT: &str = "demo-hytradboi (github.com/obi1kenobi/trustfall)";

lazy_static! {
    static ref HN_CLIENT: HnClient = HnClient::init().unwrap();
    static ref CRATES_CLIENT: consecrates::Client = consecrates::Client::new(USER_AGENT);
    static ref GITHUB_CLIENT: octorust::Client = octorust::Client::new(USER_AGENT, None,).unwrap();
}

pub struct DemoAdapter;

impl DemoAdapter {
    fn front_page(&self) -> Box<dyn Iterator<Item = Token>> {
        self.top(Some(30))
    }

    fn top(&self, max: Option<usize>) -> Box<dyn Iterator<Item = Token>> {
        let iterator = HN_CLIENT
            .get_top_stories()
            .unwrap()
            .into_iter()
            .take(max.unwrap_or(usize::MAX))
            .filter_map(|id| match HN_CLIENT.get_item(id) {
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
            .map(move |id| HN_CLIENT.get_item(id))
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
        match HN_CLIENT.get_user(username) {
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

    fn most_downloaded_crates(&self) -> Box<dyn Iterator<Item = Token>> {
        Box::new(CratesIterator::from(CRATES_CLIENT.deref()).map(|x| x.into()))
    }
}

struct CratesIterator<'a> {
    client: &'a consecrates::Client,
    next_page: usize,
    batch: Option<std::vec::IntoIter<Crate>>,
}

impl<'a> From<&'a consecrates::Client> for CratesIterator<'a> {
    fn from(client: &'a consecrates::Client) -> Self {
        Self {
            client,
            next_page: 1,
            batch: None,
        }
    }
}

impl<'a> CratesIterator<'a> {
    fn fetch_next_page(&mut self) -> bool {
        let current_page = self.next_page;
        self.next_page += 1;
        match self.client.get_crates(Query {
            page: Some(current_page),
            sort: Some(Sorting::RecentDownloads),
            per_page: Some(100),
            ..Default::default()
        }) {
            Ok(c) => {
                if c.crates.is_empty() {
                    false
                } else {
                    self.batch = Some(c.crates.into_iter());
                    true
                }
            }
            Err(e) => {
                eprintln!(
                    "Got an error while getting most downloaded crates page {}: {}",
                    current_page, e
                );
                false
            }
        }
    }
}

impl<'a> Iterator for CratesIterator<'a> {
    type Item = Crate;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.batch.take() {
                Some(mut iter) => match iter.next() {
                    Some(c) => {
                        self.batch = Some(iter);
                        return Some(c);
                    }
                    None => {
                        if !self.fetch_next_page() {
                            return None;
                        }
                    }
                },
                None => {
                    if !self.fetch_next_page() {
                        return None;
                    }
                }
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

impl Adapter<'static> for DemoAdapter {
    type DataToken = Token;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken>> {
        match edge.as_ref() {
            "HackerNewsFrontPage" => self.front_page(),
            "HackerNewsTop" => {
                // TODO: This is unergonomic, build a more convenient API here.
                let max = parameters
                    .unwrap()
                    .0
                    .get("max")
                    .map(|v| v.as_u64().unwrap() as usize);
                self.top(max)
            }
            "HackerNewsLatestStories" => {
                // TODO: This is unergonomic, build a more convenient API here.
                let max = parameters
                    .unwrap()
                    .0
                    .get("max")
                    .map(|v| v.as_u64().unwrap() as usize);
                self.latest_stories(max)
            }
            "HackerNewsUser" => {
                let username_value = parameters.as_ref().unwrap().0.get("name").unwrap();
                self.user(username_value.as_str().unwrap())
            }
            "MostDownloadedCrates" => self.most_downloaded_crates(),
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
            (_, "__typename") => Box::new(data_contexts.map(|ctx| {
                let value = match ctx.current_token.as_ref() {
                    Some(token) => token.typename().into(),
                    None => FieldValue::Null,
                };

                (ctx, value)
            })),

            // properties on Item and its implementers
            (
                "HackerNewsItem" | "HackerNewsStory" | "HackerNewsJob" | "HackerNewsComment",
                "id",
            ) => impl_item_property!(data_contexts, id),
            (
                "HackerNewsItem" | "HackerNewsStory" | "HackerNewsJob" | "HackerNewsComment",
                "unixTime",
            ) => {
                impl_item_property!(data_contexts, time)
            }

            // properties on Job
            ("HackerNewsJob", "score") => impl_property!(data_contexts, as_job, score),
            ("HackerNewsJob", "title") => impl_property!(data_contexts, as_job, title),
            ("HackerNewsJob", "url") => impl_property!(data_contexts, as_job, url),

            // properties on Story
            ("HackerNewsStory", "byUsername") => impl_property!(data_contexts, as_story, by),
            ("HackerNewsStory", "text") => impl_property!(data_contexts, as_story, text),
            ("HackerNewsStory", "commentsCount") => {
                impl_property!(data_contexts, as_story, descendants)
            }
            ("HackerNewsStory", "score") => impl_property!(data_contexts, as_story, score),
            ("HackerNewsStory", "title") => impl_property!(data_contexts, as_story, title),
            ("HackerNewsStory", "url") => impl_property!(data_contexts, as_story, url),

            // properties on Comment
            ("HackerNewsComment", "byUsername") => impl_property!(data_contexts, as_comment, by),
            ("HackerNewsComment", "text") => impl_property!(data_contexts, as_comment, text),
            ("HackerNewsComment", "childCount") => {
                impl_property!(data_contexts, as_comment, comment, {
                    comment
                        .kids
                        .as_ref()
                        .map(|v| v.len() as u64)
                        .unwrap_or(0)
                        .into()
                })
            }

            // properties on User
            ("HackerNewsUser", "id") => impl_property!(data_contexts, as_user, id),
            ("HackerNewsUser", "karma") => impl_property!(data_contexts, as_user, karma),
            ("HackerNewsUser", "about") => impl_property!(data_contexts, as_user, about),
            ("HackerNewsUser", "unixCreatedAt") => impl_property!(data_contexts, as_user, created),
            ("HackerNewsUser", "delay") => impl_property!(data_contexts, as_user, delay),

            // properties on Webpage
            ("Webpage" | "Repository" | "GitHubRepository", "url") => {
                impl_property!(data_contexts, as_webpage, url, { url.into() })
            }

            // properties on GitHubRepository
            ("GitHubRepository", "organization") => {
                impl_property!(data_contexts, as_github_repository, repo, {
                    repo.organization
                        .as_ref()
                        .map(|org| org.name.as_str())
                        .into()
                })
            }
            ("GitHubRepository", "name") => {
                impl_property!(data_contexts, as_github_repository, name)
            }
            ("GitHubRepository", "fullName") => {
                impl_property!(data_contexts, as_github_repository, full_name)
            }
            ("GitHubRepository", "lastModified") => {
                impl_property!(data_contexts, as_github_repository, updated_at)
            }
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
            ("HackerNewsStory", "byUser") => Box::new(data_contexts.map(|ctx| {
                let token = &ctx.current_token;
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let story = token.as_story().unwrap();
                        let author = story.by.as_str();
                        match HN_CLIENT.get_user(author) {
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
            ("HackerNewsStory", "comment") => Box::new(data_contexts.map(|ctx| {
                let token = &ctx.current_token;
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let story = token.as_story().unwrap();
                        let comment_ids = story.kids.clone().unwrap_or_default();
                        let story_id = story.id;

                        let neighbors_iter =
                            comment_ids.into_iter().filter_map(move |comment_id| {
                                match HN_CLIENT.get_item(comment_id) {
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
            ("HackerNewsComment", "byUser") => Box::new(data_contexts.map(|ctx| {
                let token = &ctx.current_token;
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let comment = token.as_comment().unwrap();
                        let author = comment.by.as_str();
                        match HN_CLIENT.get_user(author) {
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
            ("HackerNewsComment", "parent") => Box::new(data_contexts.map(|ctx| {
                let token = ctx.current_token.clone();
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let comment = token.as_comment().unwrap();
                        let comment_id = comment.id;
                        let parent_id = comment.parent;

                        match HN_CLIENT.get_item(parent_id) {
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
            ("HackerNewsComment", "reply") => Box::new(data_contexts.map(|ctx| {
                let token = ctx.current_token.clone();
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let comment = token.as_comment().unwrap();
                        let comment_id = comment.id;
                        let reply_ids = comment.kids.clone().unwrap_or_default();

                        Box::new(reply_ids.into_iter().filter_map(move |reply_id| {
                            match HN_CLIENT.get_item(reply_id) {
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
            ("HackerNewsUser", "submitted") => Box::new(data_contexts.map(|ctx| {
                let token = ctx.current_token.clone();
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let user = token.as_user().unwrap();
                        let submitted_ids = user.submitted.clone();

                        Box::new(submitted_ids.into_iter().filter_map(move |submission_id| {
                            match HN_CLIENT.get_item(submission_id) {
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
            ("Crate", "sourceRepository") => Box::new(data_contexts.map(|ctx| {
                let token = ctx.current_token.clone();
                let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match token {
                    None => Box::new(std::iter::empty()),
                    Some(token) => {
                        let cr = token.as_crate().unwrap();
                        match cr.repository.as_ref() {
                            None => Box::new(std::iter::empty()),
                            Some(repo) => {
                                let token = resolve_url(repo.as_str());
                                Box::new(token.into_iter())
                            }
                        }
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
                ("HackerNewsItem", "Job") => token.as_job().is_some(),
                ("HackerNewsItem", "Story") => token.as_story().is_some(),
                ("HackerNewsItem", "Comment") => token.as_comment().is_some(),
                _ => unreachable!(),
            };

            (ctx, can_coerce)
        });

        Box::new(iterator)
    }
}

fn resolve_url(url: &str) -> Option<Token> {
    let maybe_git_url = GitUrl::parse(url);
    match maybe_git_url {
        Ok(git_url) => {
            if matches!(git_url.host, Some(x) if x == "github.com") {
                let repos = octorust::repos::Repos::new(GITHUB_CLIENT.clone());
                let future = repos.get(
                    git_url
                        .owner
                        .as_ref()
                        .unwrap_or_else(|| panic!("repo {} had no owner", url))
                        .as_str(),
                    git_url.name.as_str(),
                );
                match futures_executor::block_on(future) {
                    Ok(repo) => Some(Token::GitHubRepository(Rc::from(repo))),
                    Err(e) => {
                        eprintln!(
                            "Error getting repository information for url {}: {}",
                            url, e
                        );
                        None
                    }
                }
            } else {
                Some(Token::Repository(Rc::from(url)))
            }
        }
        Err(..) => None,
    }
}
