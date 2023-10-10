#![allow(dead_code)]

use std::{
    fs,
    rc::Rc,
    sync::{Arc, OnceLock},
};

use git_url_parse::GitUrl;
use hn_api::{types::Item, HnClient};
use octorust::types::{ContentFile, FullRepository};
use tokio::runtime::Runtime;
use trustfall::{
    provider::{
        field_property, resolve_property_with, Adapter, ContextIterator, ContextOutcomeIterator,
        EdgeParameters, ResolveEdgeInfo, ResolveInfo, VertexIterator,
    },
    FieldValue,
};

use crate::{
    actions_parser::{get_env_for_run_step, get_jobs_in_workflow_file, get_steps_in_job},
    pagers::{CratesPager, WorkflowsPager},
    util::{get_owner_and_repo, Pager},
    vertex::{Repository, Vertex},
};

const USER_AGENT: &str = "demo-hytradboi (github.com/obi1kenobi/trustfall)";

static HN_CLIENT: OnceLock<HnClient> = OnceLock::new();

fn get_hn_client() -> &'static HnClient {
    HN_CLIENT.get_or_init(|| HnClient::init().unwrap())
}

static CRATES_CLIENT: OnceLock<consecrates::Client> = OnceLock::new();

fn get_crates_client() -> &'static consecrates::Client {
    CRATES_CLIENT.get_or_init(|| consecrates::Client::new(USER_AGENT))
}

static GITHUB_CLIENT: OnceLock<octorust::Client> = OnceLock::new();

fn get_github_client() -> &'static octorust::Client {
    GITHUB_CLIENT.get_or_init(|| {
        octorust::Client::new(
            USER_AGENT,
            Some(octorust::auth::Credentials::Token(std::env::var("GITHUB_TOKEN").unwrap_or_else(
                |_| {
                    fs::read_to_string("./localdata/gh_token")
                        .expect("could not find creds file")
                        .trim()
                        .to_string()
                },
            ))),
        )
        .unwrap()
    })
}

static REPOS_CLIENT: OnceLock<octorust::repos::Repos> = OnceLock::new();

fn get_repos_client() -> &'static octorust::repos::Repos {
    REPOS_CLIENT.get_or_init(|| octorust::repos::Repos::new(get_github_client().clone()))
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME
        .get_or_init(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap())
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
        let iterator = get_hn_client()
            .get_top_stories()
            .unwrap()
            .into_iter()
            .take(max.unwrap_or(usize::MAX))
            .filter_map(|id| match get_hn_client().get_item(id) {
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
            .map(move |id| get_hn_client().get_item(id))
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
        match get_hn_client().get_user(username) {
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
        Box::new(CratesPager::new(get_crates_client()).into_iter().map(|x| x.into()))
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

impl<'a> Adapter<'a> for DemoAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
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
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
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
            ("HackerNewsJob", "score") => {
                resolve_property_with(contexts, field_property!(as_job, score))
            }
            ("HackerNewsJob", "title") => {
                resolve_property_with(contexts, field_property!(as_job, title))
            }
            ("HackerNewsJob", "url") => {
                resolve_property_with(contexts, field_property!(as_job, url))
            }

            // properties on HackerNewsStory
            ("HackerNewsStory", "byUsername") => {
                resolve_property_with(contexts, field_property!(as_story, by))
            }
            ("HackerNewsStory", "text") => {
                resolve_property_with(contexts, field_property!(as_story, text))
            }
            ("HackerNewsStory", "commentsCount") => {
                resolve_property_with(contexts, field_property!(as_story, descendants))
            }
            ("HackerNewsStory", "score") => {
                resolve_property_with(contexts, field_property!(as_story, score))
            }
            ("HackerNewsStory", "title") => {
                resolve_property_with(contexts, field_property!(as_story, title))
            }
            ("HackerNewsStory", "url") => {
                resolve_property_with(contexts, field_property!(as_story, url))
            }

            // properties on HackerNewsComment
            ("HackerNewsComment", "byUsername") => {
                resolve_property_with(contexts, field_property!(as_comment, by))
            }
            ("HackerNewsComment", "text") => {
                resolve_property_with(contexts, field_property!(as_comment, text))
            }
            ("HackerNewsComment", "childCount") => resolve_property_with(
                contexts,
                field_property!(as_comment, kids, {
                    kids.as_ref().map(|v| v.len() as u64).unwrap_or(0).into()
                }),
            ),

            // properties on HackerNewsUser
            ("HackerNewsUser", "id") => {
                resolve_property_with(contexts, field_property!(as_user, id))
            }
            ("HackerNewsUser", "karma") => {
                resolve_property_with(contexts, field_property!(as_user, karma))
            }
            ("HackerNewsUser", "about") => {
                resolve_property_with(contexts, field_property!(as_user, about))
            }
            ("HackerNewsUser", "unixCreatedAt") => {
                resolve_property_with(contexts, field_property!(as_user, created))
            }
            ("HackerNewsUser", "delay") => {
                resolve_property_with(contexts, field_property!(as_user, delay))
            }

            // properties on Crate
            ("Crate", "name") => resolve_property_with(contexts, field_property!(as_crate, name)),
            ("Crate", "latestVersion") => {
                resolve_property_with(contexts, field_property!(as_crate, max_version))
            }

            // properties on Webpage
            ("Webpage" | "Repository" | "GitHubRepository", "url") => {
                resolve_property_with(contexts, |vertex| {
                    vertex.as_webpage().expect("not a Webpage").into()
                })
            }

            // properties on GitHubRepository
            ("GitHubRepository", "owner") => resolve_property_with(contexts, |vertex| {
                let repo = vertex.as_github_repository().expect("not a GitHubRepository");
                let (owner, _) = get_owner_and_repo(repo);
                owner.into()
            }),
            ("GitHubRepository", "name") => {
                resolve_property_with(contexts, field_property!(as_github_repository, name))
            }
            ("GitHubRepository", "fullName") => {
                resolve_property_with(contexts, field_property!(as_github_repository, full_name))
            }
            ("GitHubRepository", "lastModified") => resolve_property_with(
                contexts,
                field_property!(as_github_repository, updated_at, {
                    updated_at.map(|value| value.timestamp()).into()
                }),
            ),

            // properties on GitHubWorkflow
            ("GitHubWorkflow", "name") => resolve_property_with(
                contexts,
                field_property!(as_github_workflow, workflow, { workflow.name.as_str().into() }),
            ),
            ("GitHubWorkflow", "path") => resolve_property_with(
                contexts,
                field_property!(as_github_workflow, workflow, { workflow.path.as_str().into() }),
            ),

            // properties on GitHubActionsJob
            ("GitHubActionsJob", "name") => {
                resolve_property_with(contexts, field_property!(as_github_actions_job, name))
            }
            ("GitHubActionsJob", "runsOn") => {
                resolve_property_with(contexts, field_property!(as_github_actions_job, runs_on))
            }

            // properties on GitHubActionsStep and its implementers
            (
                "GitHubActionsStep" | "GitHubActionsImportedStep" | "GitHubActionsRunStep",
                "name",
            ) => resolve_property_with(contexts, |vertex| {
                vertex.as_github_actions_step().expect("not a step").into()
            }),

            // properties on GitHubActionsImportedStep
            ("GitHubActionsImportedStep", "uses") => resolve_property_with(
                contexts,
                field_property!(as_github_actions_imported_step, uses),
            ),

            // properties on GitHubActionsRunStep
            ("GitHubActionsRunStep", "run") => {
                resolve_property_with(contexts, field_property!(as_github_actions_run_step, run))
            }

            // properties on NameValuePair
            ("NameValuePair", "name") => resolve_property_with(contexts, |vertex| {
                vertex.as_name_value_pair().expect("not a NameValuePair").0.clone().into()
            }),
            ("NameValuePair", "value") => resolve_property_with(contexts, |vertex| {
                vertex.as_name_value_pair().expect("not a NameValuePair").0.clone().into()
            }),
            _ => unreachable!(),
        }
    }

    fn resolve_neighbors(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
        match (type_name.as_ref(), edge_name.as_ref()) {
            ("HackerNewsStory", "byUser") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex();
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let story = vertex.as_story().unwrap();
                        let author = story.by.as_str();
                        match get_hn_client().get_user(author) {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let story = vertex.as_story().unwrap();
                        let comment_ids = story.kids.clone().unwrap_or_default();
                        let story_id = story.id;

                        let neighbors_iter =
                            comment_ids.into_iter().filter_map(move |comment_id| {
                                match get_hn_client().get_item(comment_id) {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
                        let author = comment.by.as_str();
                        match get_hn_client().get_user(author) {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
                        let comment_id = comment.id;
                        let parent_id = comment.parent;

                        match get_hn_client().get_item(parent_id) {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
                        let mut comment_id = comment.id;
                        let mut parent_id = comment.parent;
                        loop {
                            match get_hn_client().get_item(parent_id) {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let comment = vertex.as_comment().unwrap();
                        let comment_id = comment.id;
                        let reply_ids = comment.kids.clone().unwrap_or_default();

                        Box::new(reply_ids.into_iter().filter_map(move |reply_id| {
                            match get_hn_client().get_item(reply_id) {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => {
                        let user = vertex.as_user().unwrap();
                        let submitted_ids = user.submitted.clone();

                        Box::new(submitted_ids.into_iter().filter_map(move |submission_id| {
                            match get_hn_client().get_item(submission_id) {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(vertex) => Box::new(
                        WorkflowsPager::new(get_github_client().clone(), vertex, get_runtime())
                            .into_iter()
                            .map(|x| x.into()),
                    ),
                };

                (ctx, neighbors)
            })),
            ("GitHubWorkflow", "jobs") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
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
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
                    None => Box::new(std::iter::empty()),
                    Some(Vertex::GitHubActionsJob(job)) => get_steps_in_job(job),
                    _ => unreachable!(),
                };

                (ctx, neighbors)
            })),
            ("GitHubActionsRunStep", "env") => Box::new(contexts.map(|ctx| {
                let vertex = ctx.active_vertex().cloned();
                let neighbors: VertexIterator<'a, Self::Vertex> = match vertex {
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
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
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
                let future = get_repos_client().get(
                    git_url
                        .owner
                        .as_ref()
                        .unwrap_or_else(|| panic!("repo {url} had no owner"))
                        .as_str(),
                    git_url.name.as_str(),
                );
                match get_runtime().block_on(future) {
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

    match get_runtime().block_on(get_repos_client().get_content_file(
        owner,
        repo_name,
        path,
        main_branch,
    )) {
        Ok(content) => Some(content),
        Err(e) => {
            eprintln!(
                "Error getting repo {owner}/{repo_name} branch {main_branch} file {path}: {e}",
            );
            None
        }
    }
}
