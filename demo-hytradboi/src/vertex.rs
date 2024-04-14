#![allow(dead_code)]

use std::rc::Rc;

use consecrates::api::Crate;
use hn_api::types::{Comment, Item, Job, Story, User};
use octorust::types::{FullRepository, Workflow};
use yaml_rust::Yaml;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Vertex {
    HackerNewsItem(Rc<Item>),
    HackerNewsStory(Rc<Story>),
    HackerNewsJob(Rc<Job>),
    HackerNewsComment(Rc<Comment>),
    HackerNewsUser(Rc<User>),
    Crate(Rc<Crate>),
    Repository(Rc<str>),
    GitHubRepository(Rc<Repository>),
    GitHubWorkflow(Rc<RepoWorkflow>),
    GitHubActionsJob(Rc<ActionsJob>),
    GitHubActionsImportedStep(Rc<ActionsImportedStep>),
    GitHubActionsRunStep(Rc<ActionsRunStep>),
    NameValuePair(Rc<(String, String)>),
    Webpage(Rc<str>),
}

#[derive(Debug, Clone)]
pub struct Repository {
    pub url: String,
    pub repo: Rc<FullRepository>,
}

impl Repository {
    pub(crate) fn new(url: String, repo: Rc<FullRepository>) -> Self {
        Self { url, repo }
    }
}

#[derive(Debug, Clone)]
pub struct RepoWorkflow {
    pub repo: Rc<FullRepository>,
    pub workflow: Rc<Workflow>,
}

impl RepoWorkflow {
    pub(crate) fn new(repo: Rc<FullRepository>, workflow: Rc<Workflow>) -> Self {
        Self { repo, workflow }
    }
}

#[derive(Debug, Clone)]
pub struct ActionsJob {
    pub yaml: Yaml,
    pub name: String,
    pub runs_on: Option<String>,
}

impl ActionsJob {
    pub(crate) fn new(yaml: Yaml, name: String, runs_on: Option<String>) -> Self {
        Self { yaml, name, runs_on }
    }
}

#[derive(Debug, Clone)]
pub struct ActionsImportedStep {
    pub yaml: Yaml,
    pub name: Option<String>,
    pub uses: String,
}

impl ActionsImportedStep {
    pub(crate) fn new(yaml: Yaml, name: Option<String>, uses: String) -> Self {
        Self { yaml, name, uses }
    }
}

#[derive(Debug, Clone)]
pub struct ActionsRunStep {
    pub yaml: Yaml,
    pub name: Option<String>,
    pub run: Vec<String>,
}

impl ActionsRunStep {
    pub(crate) fn new(yaml: Yaml, name: Option<String>, run: Vec<String>) -> Self {
        Self { yaml, name, run }
    }
}

#[allow(dead_code)]
impl Vertex {
    pub fn typename(&self) -> &'static str {
        match self {
            Vertex::HackerNewsItem(..) => "HackerNewsItem",
            Vertex::HackerNewsStory(..) => "HackerNewsStory",
            Vertex::HackerNewsJob(..) => "HackerNewsJob",
            Vertex::HackerNewsComment(..) => "HackerNewsComment",
            Vertex::HackerNewsUser(..) => "HackerNewsUser",
            Vertex::Crate(..) => "Crate",
            Vertex::Repository(..) => "Repository",
            Vertex::GitHubRepository(..) => "GitHubRepository",
            Vertex::GitHubWorkflow(..) => "GitHubWorkflow",
            Vertex::GitHubActionsJob(..) => "GitHubActionsJob",
            Vertex::GitHubActionsImportedStep(..) => "GitHubActionsImportedStep",
            Vertex::GitHubActionsRunStep(..) => "GitHubActionsRunStep",
            Vertex::NameValuePair(..) => "NameValuePair",
            Vertex::Webpage(..) => "Webpage",
        }
    }

    pub fn as_story(&self) -> Option<&Story> {
        match self {
            Vertex::HackerNewsStory(s) => Some(s.as_ref()),
            Vertex::HackerNewsItem(i) => match &**i {
                Item::Story(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_job(&self) -> Option<&Job> {
        match self {
            Vertex::HackerNewsJob(s) => Some(s.as_ref()),
            Vertex::HackerNewsItem(i) => match &**i {
                Item::Job(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_comment(&self) -> Option<&Comment> {
        match self {
            Vertex::HackerNewsComment(s) => Some(s.as_ref()),
            Vertex::HackerNewsItem(i) => match &**i {
                Item::Comment(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_user(&self) -> Option<&User> {
        match self {
            Vertex::HackerNewsUser(u) => Some(u.as_ref()),
            _ => None,
        }
    }

    pub fn as_crate(&self) -> Option<&Crate> {
        match self {
            Vertex::Crate(c) => Some(c.as_ref()),
            _ => None,
        }
    }

    pub fn as_webpage(&self) -> Option<&str> {
        match self {
            Vertex::GitHubRepository(r) => Some(r.url.as_ref()),
            Vertex::Repository(r) => Some(r.as_ref()),
            Vertex::Webpage(w) => Some(w.as_ref()),
            _ => None,
        }
    }

    pub fn as_repository(&self) -> Option<&str> {
        match self {
            Vertex::GitHubRepository(r) => Some(r.url.as_ref()),
            Vertex::Repository(r) => Some(r.as_ref()),
            _ => None,
        }
    }

    pub fn as_github_repository(&self) -> Option<&FullRepository> {
        match self {
            Vertex::GitHubRepository(r) => Some(r.repo.as_ref()),
            _ => None,
        }
    }

    pub fn as_github_workflow(&self) -> Option<&RepoWorkflow> {
        match self {
            Vertex::GitHubWorkflow(w) => Some(w.as_ref()),
            _ => None,
        }
    }

    pub fn as_github_actions_job(&self) -> Option<&ActionsJob> {
        match self {
            Vertex::GitHubActionsJob(j) => Some(j.as_ref()),
            _ => None,
        }
    }

    pub fn as_github_actions_step(&self) -> Option<Option<&str>> {
        match self {
            Vertex::GitHubActionsImportedStep(imp) => Some(imp.name.as_deref()),
            Vertex::GitHubActionsRunStep(r) => Some(r.name.as_deref()),
            _ => None,
        }
    }

    pub fn as_github_actions_run_step(&self) -> Option<&ActionsRunStep> {
        match self {
            Vertex::GitHubActionsRunStep(r) => Some(r.as_ref()),
            _ => None,
        }
    }

    pub fn as_github_actions_imported_step(&self) -> Option<&ActionsImportedStep> {
        match self {
            Vertex::GitHubActionsImportedStep(imp) => Some(imp.as_ref()),
            _ => None,
        }
    }

    pub fn as_name_value_pair(&self) -> Option<&(String, String)> {
        match self {
            Vertex::NameValuePair(nvp) => Some(nvp.as_ref()),
            _ => None,
        }
    }
}

impl From<Item> for Vertex {
    fn from(item: Item) -> Self {
        Self::HackerNewsItem(Rc::from(item))
    }
}

impl From<Story> for Vertex {
    fn from(s: Story) -> Self {
        Self::HackerNewsStory(Rc::from(s))
    }
}

impl From<Job> for Vertex {
    fn from(j: Job) -> Self {
        Self::HackerNewsJob(Rc::from(j))
    }
}

impl From<Comment> for Vertex {
    fn from(c: Comment) -> Self {
        Self::HackerNewsComment(Rc::from(c))
    }
}

impl From<User> for Vertex {
    fn from(u: User) -> Self {
        Self::HackerNewsUser(Rc::from(u))
    }
}

impl From<Crate> for Vertex {
    fn from(c: Crate) -> Self {
        Self::Crate(Rc::from(c))
    }
}

impl From<Repository> for Vertex {
    fn from(r: Repository) -> Self {
        Self::GitHubRepository(Rc::from(r))
    }
}

impl From<RepoWorkflow> for Vertex {
    fn from(w: RepoWorkflow) -> Self {
        Self::GitHubWorkflow(Rc::from(w))
    }
}

impl From<ActionsJob> for Vertex {
    fn from(j: ActionsJob) -> Self {
        Self::GitHubActionsJob(Rc::from(j))
    }
}

impl From<ActionsImportedStep> for Vertex {
    fn from(imp: ActionsImportedStep) -> Self {
        Self::GitHubActionsImportedStep(Rc::from(imp))
    }
}

impl From<ActionsRunStep> for Vertex {
    fn from(r: ActionsRunStep) -> Self {
        Self::GitHubActionsRunStep(Rc::from(r))
    }
}
