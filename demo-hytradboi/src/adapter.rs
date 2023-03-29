#![allow(dead_code)]

use std::{fs, rc::Rc, sync::Arc};

use git_url_parse::GitUrl;
use hn_api::{types::Item, HnClient};
use lazy_static::__Deref;
use octorust::types::{ContentFile, FullRepository};
use tokio::runtime::Runtime;
use trustfall_core::{
    interpreter::{
        Adapter, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
        VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
};

use crate::{
    actions_parser::{get_env_for_run_step, get_jobs_in_workflow_file, get_steps_in_job},
    pagers::{CratesPager, WorkflowsPager},
    util::{get_owner_and_repo, Pager},
    vertex::{Repository, Vertex},
};

const USER_AGENT: &str = "demo-hytradboi (github.com/obi1kenobi/trustfall)";

lazy_static! {
    static ref HN_CLIENT: HnClient = HnClient::init().unwrap();
    static ref CRATES_CLIENT: consecrates::Client = consecrates::Client::new(USER_AGENT);
    static ref GITHUB_CLIENT: octorust::Client = octorust::Client::new(
        USER_AGENT,
        Some(octorust::auth::Credentials::Token(
            std::env::var("GITHUB_TOKEN").unwrap_or_else(|_| {
                fs::read_to_string("./localdata/gh_token")
                    .expect("could not find creds file")
                    .trim()
                    .to_string()
            })
        )),
    )
    .unwrap();
    static ref REPOS_CLIENT: octorust::repos::Repos =
        octorust::repos::Repos::new(GITHUB_CLIENT.clone());
    static ref RUNTIME: Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
}

pub struct DemoAdapter;

impl DemoAdapter {
    pub fn new() -> Self {
        Self
    }

    fn front_page(&self) -> VertexIterator<'static, Vertex> {
        self.top(Some(30))
    }

    fn top(&self, max: Option<usize>) -> VertexIterator<'static, Vertex> {
        let iterator = HN_CLIENT
            .get_top_stories()
            .unwrap()
            .into_iter()
            .take(max.unwrap_or(usize::MAX))
            .filter_map(|id| match HN_CLIENT.get_item(id) {
                Ok(maybe_item) => maybe_item.map(|item| item.into()),
                Err(e) => {
                    eprintln!("Got an error while fetching item: {e}");
                    None
                }
            });

        Box::new(iterator)
    }

    fn latest_stories(&self, max: Option<usize>) -> VertexIterator<'static, Vertex> {
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
                    eprintln!("Got an error while fetching item: {e}");
                    None
                }
            });

        Box::new(iterator)
    }

    fn user(&self, username: &str) -> VertexIterator<'static, Vertex> {
        match HN_CLIENT.get_user(username) {
            Ok(Some(user)) => {
                // Found a user by that name.
                let vertex = Vertex::from(user);
                Box::new(std::iter::once(vertex))
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

    fn most_downloaded_crates(&self) -> VertexIterator<'static, Vertex> {
        Box::new(
            CratesPager::new(CRATES_CLIENT.deref())
                .into_iter()
                .map(|x| x.into()),
        )
    }
}

macro_rules! impl_item_property {
    ($contexts:ident, $attr:ident) => {
        Box::new($contexts.map(|ctx| {
            let value = ctx
                .active_vertex()
                .map(|t| {
                    if let Some(s) = t.as_story() {
                        s.$attr.clone()
                    } else if let Some(j) = t.as_job() {
                        j.$attr.clone()
                    } else if let Some(c) = t.as_comment() {
                        c.$attr.clone()
                    } else {
                        unreachable!()
                    }
                })
                .into();

            (ctx, value)
        }))
    };
}

macro_rules! impl_property {
    ($contexts:ident, $conversion:ident, $attr:ident) => {
        Box::new($contexts.map(|ctx| {
            let vertex = ctx
                .active_vertex()
                .map(|vertex| vertex.$conversion().unwrap());
            let value = vertex.map(|t| t.$attr.clone()).into();

            (ctx, value)
        }))
    };

    ($contexts:ident, $conversion:ident, $var:ident, $b:block) => {
        Box::new($contexts.map(|ctx| {
            let vertex = ctx
                .active_vertex()
                .map(|vertex| vertex.$conversion().unwrap());
            let value = vertex.map(|$var| $b).into();

            (ctx, value)
        }))
    };
}

impl Adapter<'static> for DemoAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        match edge_name.as_ref() {
            "HackerNewsFrontPage" => self.front_page(),
            "HackerNewsTop" => {
                let max = parameters.get("max").map(|v| v.as_u64().unwrap() as usize);
                self.top(max)
            }
            "HackerNewsLatestStories" => {
                let max = parameters.get("max").map(|v| v.as_u64().unwrap() as usize);
                self.latest_stories(max)
            }
            "HackerNewsUser" => {
                let username_value = parameters["name"].as_str().unwrap();
                self.user(username_value)
            }
            "MostDownloadedCrates" => self.most_downloaded_crates(),
            _ => unreachable!(),
        }
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        match (type_name.as_ref(), property_name.as_ref()) {
            (_, "__typename") => Box::new(contexts.map(|ctx| {
                let value = match ctx.active_vertex() {
                    Some(vertex) => vertex.typename().into(),
                    None => FieldValue::Null,
                };

                (ctx, value)
            })),

            // properties on HackerNewsItem and its implementers
            (
                "HackerNewsItem" | "HackerNewsStory" | "HackerNewsJob" | "HackerNewsComment",
                "id",
            ) => impl_item_property!(contexts, id),
            (
                "HackerNewsItem" | "HackerNewsStory" | "HackerNewsJob" | "HackerNewsComment",
                "unixTime",
            ) => {
                impl_item_property!(contexts, time)
            }

            // properties on HackerNewsJob
            ("HackerNewsJob", "score") => impl_property!(contexts, as_job, score),
            ("HackerNewsJob", "title") => impl_property!(contexts, as_job, title),
            ("HackerNewsJob", "url") => impl_property!(contexts, as_job, url),

            // properties on HackerNewsStory
            ("HackerNewsStory", "byUsername") => impl_property!(contexts, as_story, by),
            ("HackerNewsStory", "text") => impl_property!(contexts, as_story, text),
            ("HackerNewsStory", "commentsCount") => {
                impl_property!(contexts, as_story, descendants)
            }
            ("HackerNewsStory", "score") => impl_property!(contexts, as_story, score),
            ("HackerNewsStory", "title") => impl_property!(contexts, as_story, title),
            ("HackerNewsStory", "url") => impl_property!(contexts, as_story, url),

            // properties on HackerNewsComment
            ("HackerNewsComment", "byUsername") => impl_property!(contexts, as_comment, by),
            ("HackerNewsComment", "text") => impl_property!(contexts, as_comment, text),
            ("HackerNewsComment", "childCount") => {
                impl_property!(contexts, as_comment, comment, {
                    comment.kids.as_ref().map(|v| v.len() as u64).unwrap_or(0)
                })
            }

            // properties on HackerNewsUser
            ("HackerNewsUser", "id") => impl_property!(contexts, as_user, id),
            ("HackerNewsUser", "karma") => impl_property!(contexts, as_user, karma),
            ("HackerNewsUser", "about") => impl_property!(contexts, as_user, about),
            ("HackerNewsUser", "unixCreatedAt") => impl_property!(contexts, as_user, created),
            ("HackerNewsUser", "delay") => impl_property!(contexts, as_user, delay),

            // properties on Crate
            ("Crate", "name") => impl_property!(contexts, as_crate, name),
            ("Crate", "latestVersion") => impl_property!(contexts, as_crate, max_version),

            // properties on Webpage
            ("Webpage" | "Repository" | "GitHubRepository", "url") => {
                impl_property!(contexts, as_webpage, url, { url })
            }

            // properties on GitHubRepository
            ("GitHubRepository", "owner") => {
                impl_property!(contexts, as_github_repository, repo, {
                    let (owner, _) = get_owner_and_repo(repo);
                    owner
                })
            }
            ("GitHubRepository", "name") => {
                impl_property!(contexts, as_github_repository, name)
            }
            ("GitHubRepository", "fullName") => {
                impl_property!(contexts, as_github_repository, full_name)
            }
            ("GitHubRepository", "lastModified") => {
                impl_property!(contexts, as_github_repository, updated_at)
            }

            // properties on GitHubWorkflow
            ("GitHubWorkflow", "name") => impl_property!(contexts, as_github_workflow, wf, {
                wf.workflow.name.as_str()
            }),
            ("GitHubWorkflow", "path") => impl_property!(contexts, as_github_workflow, wf, {
                wf.workflow.path.as_str()
            }),

            // properties on GitHubActionsJob
            ("GitHubActionsJob", "name") => {
                impl_property!(contexts, as_github_actions_job, name)
            }
            ("GitHubActionsJob", "runsOn") => {
                impl_property!(contexts, as_github_actions_job, runs_on)
            }

            // properties on GitHubActionsStep and its implementers
            (
                "GitHubActionsStep" | "GitHubActionsImportedStep" | "GitHubActionsRunStep",
                "name",
            ) => impl_property!(contexts, as_github_actions_step, step_name, { step_name }),

            // properties on GitHubActionsImportedStep
            ("GitHubActionsImportedStep", "uses") => {
                impl_property!(contexts, as_github_actions_imported_step, uses)
            }

            // properties on GitHubActionsRunStep
            ("GitHubActionsRunStep", "run") => {
                impl_property!(contexts, as_github_actions_run_step, run)
            }

            // properties on NameValuePair
            ("NameValuePair", "name") => {
                impl_property!(contexts, as_name_value_pair, pair, { pair.0.clone() })
            }
            ("NameValuePair", "value") => {
                impl_property!(contexts, as_name_value_pair, pair, { pair.1.clone() })
            }
            _ => unreachable!(),
        }
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        match (type_name.as_ref(), edge_name.as_ref()) {
            ("HackerNewsStory", "byUser") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let story = vertex.as_story().unwrap();
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
            ("HackerNewsStory", "comment") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let story = vertex.as_story().unwrap();
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
                                            "API error while fetching story {story_id} comment {comment_id}: {e}",
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
            ("HackerNewsStory", "link") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let story = vertex.as_story().unwrap();
                        Box::new(
                            story
                                .url
                                .as_ref()
                                .and_then(|url| resolve_url(url.as_str()))
                                .into_iter(),
                        )
                    }
                };

                (ctx, neighbors)
            })),
            ("HackerNewsJob", "link") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let job = vertex.as_job().unwrap();
                        Box::new(
                            job.url
                                .as_ref()
                                .and_then(|url| resolve_url(url.as_str()))
                                .into_iter(),
                        )
                    }
                };

                (ctx, neighbors)
            })),
            ("HackerNewsComment", "byUser") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
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
            ("HackerNewsComment", "parent") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
                        let comment_id = comment.id;
                        let parent_id = comment.parent;

                        match HN_CLIENT.get_item(parent_id) {
                            Ok(None) => Box::new(std::iter::empty()),
                            Ok(Some(item)) => Box::new(std::iter::once(item.into())),
                            Err(e) => {
                                eprintln!(
                                    "API error while fetching comment {comment_id} parent {parent_id}: {e}",
                                );
                                Box::new(std::iter::empty())
                            }
                        }
                    }
                };

                (ctx, neighbors)
            })),
            ("HackerNewsComment", "topmostAncestor") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
                        let mut comment_id = comment.id;
                        let mut parent_id = comment.parent;
                        loop {
                            match HN_CLIENT.get_item(parent_id) {
                                Ok(None) => break Box::new(std::iter::empty()),
                                Ok(Some(item)) => match item {
                                    Item::Story(s) => break Box::new(std::iter::once(s.into())),
                                    Item::Job(j) => break Box::new(std::iter::once(j.into())),
                                    Item::Comment(c) => {
                                        comment_id = c.id;
                                        parent_id = c.parent;
                                    }
                                    Item::Poll(..) | Item::Pollopt(..) => {
                                        // Not supported, because HackerNews doesn't really
                                        // run polls anymore, even though the API still supports them.
                                        break Box::new(std::iter::empty());
                                    }
                                },
                                Err(e) => {
                                    eprintln!(
                                        "API error while fetching comment {comment_id} parent {parent_id}: {e}",
                                    );
                                    break Box::new(std::iter::empty());
                                }
                            }
                        }
                    }
                };

                (ctx, neighbors)
            })),
            ("HackerNewsComment", "reply") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
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
                                        "API error while fetching comment {comment_id} reply {reply_id}: {e}",
                                    );
                                    None
                                }
                            }
                        }))
                    }
                };

                (ctx, neighbors)
            })),
            ("HackerNewsUser", "submitted") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let user = vertex.as_user().unwrap();
                        let submitted_ids = user.submitted.clone();

                        Box::new(submitted_ids.into_iter().filter_map(move |submission_id| {
                            match HN_CLIENT.get_item(submission_id) {
                                Ok(None) => None,
                                Ok(Some(item)) => Some(item.into()),
                                Err(e) => {
                                    eprintln!(
                                        "API error while fetching submitted item {submission_id}: {e}",
                                    );
                                    None
                                }
                            }
                        }))
                    }
                };

                (ctx, neighbors)
            })),
            ("Crate", "repository") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let cr = vertex.as_crate().unwrap();
                        match cr.repository.as_ref() {
                            None => Box::new(std::iter::empty()),
                            Some(repo) => {
                                let vertex = resolve_url(repo.as_str());
                                Box::new(vertex.into_iter())
                            }
                        }
                    }
                };

                (ctx, neighbors)
            })),
            ("GitHubRepository", "workflows") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => Box::new(
                        WorkflowsPager::new(GITHUB_CLIENT.clone(), vertex, RUNTIME.deref())
                            .into_iter()
                            .map(|x| x.into()),
                    ),
                };

                (ctx, neighbors)
            })),
            ("GitHubWorkflow", "jobs") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let workflow = vertex.as_github_workflow().unwrap();
                        let path = workflow.workflow.path.as_ref();
                        let repo = workflow.repo.as_ref();
                        let workflow_content = get_repo_file_content(repo, path);
                        match workflow_content {
                            None => Box::new(std::iter::empty()),
                            Some(content) => {
                                let content = Rc::new(content);
                                get_jobs_in_workflow_file(content)
                            }
                        }
                    }
                };

                (ctx, neighbors)
            })),
            ("GitHubActionsJob", "step") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(Vertex::GitHubActionsJob(job)) => get_steps_in_job(job),
                    _ => unreachable!(),
                };

                (ctx, neighbors)
            })),
            ("GitHubActionsRunStep", "env") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'static, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(Vertex::GitHubActionsRunStep(s)) => get_env_for_run_step(s),
                    _ => unreachable!(),
                };

                (ctx, neighbors)
            })),
            _ => unreachable!("{} {}", type_name.as_ref(), edge_name.as_ref()),
        }
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        let type_name = type_name.clone();
        let coerce_to_type = coerce_to_type.clone();
        let iterator = contexts.map(move |ctx| {
            let vertex = match ctx.active_vertex() {
                Some(t) => t,
                None => return (ctx, false),
            };

            // Possible optimization here:
            // This "match" is loop-invariant, and can be hoisted outside the map() call
            // at the cost of a bit of code repetition.

            let can_coerce = match (type_name.as_ref(), coerce_to_type.as_ref()) {
                ("HackerNewsItem", "HackerNewsJob") => vertex.as_job().is_some(),
                ("HackerNewsItem", "HackerNewsStory") => vertex.as_story().is_some(),
                ("HackerNewsItem", "HackerNewsComment") => vertex.as_comment().is_some(),
                ("Webpage", "Repository") => vertex.as_repository().is_some(),
                ("Webpage", "GitHubRepository") => vertex.as_github_repository().is_some(),
                ("Repository", "GitHubRepository") => vertex.as_github_repository().is_some(),
                ("GitHubActionsStep", "GitHubActionsImportedStep") => {
                    vertex.as_github_actions_imported_step().is_some()
                }
                ("GitHubActionsStep", "GitHubActionsRunStep") => {
                    vertex.as_github_actions_run_step().is_some()
                }
                unhandled => unreachable!("{:?}", unhandled),
            };

            (ctx, can_coerce)
        });

        Box::new(iterator)
    }
}

fn resolve_url(url: &str) -> Option<Vertex> {
    // HACK: Avoiding this bug https://github.com/tjtelan/git-url-parse-rs/issues/22
    if !url.contains("github.com") && !url.contains("gitlab.com") {
        return Some(Vertex::Webpage(Rc::from(url)));
    }

    let maybe_git_url = GitUrl::parse(url);
    match maybe_git_url {
        Ok(git_url) => {
            if git_url.fullname != git_url.path.trim_matches('/') {
                // The link points *within* the repo rather than *at* the repo.
                // This is just a regular link to a webpage.
                Some(Vertex::Webpage(Rc::from(url)))
            } else if matches!(git_url.host, Some(x) if x == "github.com") {
                let future = REPOS_CLIENT.get(
                    git_url
                        .owner
                        .as_ref()
                        .unwrap_or_else(|| panic!("repo {url} had no owner"))
                        .as_str(),
                    git_url.name.as_str(),
                );
                match RUNTIME.block_on(future) {
                    Ok(repo) => Some(Repository::new(url.to_string(), Rc::new(repo)).into()),
                    Err(e) => {
                        eprintln!("Error getting repository information for url {url}: {e}",);
                        None
                    }
                }
            } else {
                Some(Vertex::Repository(Rc::from(url)))
            }
        }
        Err(..) => Some(Vertex::Webpage(Rc::from(url))),
    }
}

fn get_repo_file_content(repo: &FullRepository, path: &str) -> Option<ContentFile> {
    let (owner, repo_name) = get_owner_and_repo(repo);
    let main_branch = repo.default_branch.as_ref();

    match RUNTIME.block_on(REPOS_CLIENT.get_content_file(owner, repo_name, path, main_branch)) {
        Ok(content) => Some(content),
        Err(e) => {
            eprintln!(
                "Error getting repo {owner}/{repo_name} branch {main_branch} file {path}: {e}",
            );
            None
        }
    }
}
