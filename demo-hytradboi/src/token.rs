use std::rc::Rc;

use consecrates::api::Crate;
use hn_api::types::{Comment, Item, Job, Story, User};
use octorust::types::{FullRepository, Workflow};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Token {
    HackerNewsItem(Rc<Item>),
    HackerNewsStory(Rc<Story>),
    HackerNewsJob(Rc<Job>),
    HackerNewsComment(Rc<Comment>),
    HackerNewsUser(Rc<User>),
    Crate(Rc<Crate>),
    Repository(Rc<str>),
    GitHubRepository(Rc<FullRepository>),
    GitHubWorkflow(Rc<Workflow>),
    GitHubActionsJob(),
    GitHubActionsImportedStep(),
    GitHubActionsRunStep(),
    NameValuePair(Rc<(String, String)>),
    Webpage(Rc<str>),
}

#[allow(dead_code)]
impl Token {
    pub fn typename(&self) -> &'static str {
        match self {
            Token::HackerNewsItem(..) => "HackerNewsItem",
            Token::HackerNewsStory(..) => "HackerNewsStory",
            Token::HackerNewsJob(..) => "HackerNewsJob",
            Token::HackerNewsComment(..) => "HackerNewsComment",
            Token::HackerNewsUser(..) => "HackerNewsUser",
            Token::Crate(..) => "Crate",
            Token::Repository(..) => "Repository",
            Token::GitHubRepository(..) => "GitHubRepository",
            Token::GitHubWorkflow(..) => "GitHubWorkflow",
            Token::GitHubActionsJob(..) => "GitHubActionsJob",
            Token::GitHubActionsImportedStep(..) => "GitHubActionsImportedStep",
            Token::GitHubActionsRunStep(..) => "GitHubActionsRunStep",
            Token::NameValuePair(..) => "NameValuePair",
            Token::Webpage(..) => "Webpage",
        }
    }

    pub fn as_story(&self) -> Option<&Story> {
        match self {
            Token::HackerNewsStory(s) => Some(s.as_ref()),
            Token::HackerNewsItem(i) => match &**i {
                Item::Story(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_job(&self) -> Option<&Job> {
        match self {
            Token::HackerNewsJob(s) => Some(s.as_ref()),
            Token::HackerNewsItem(i) => match &**i {
                Item::Job(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_comment(&self) -> Option<&Comment> {
        match self {
            Token::HackerNewsComment(s) => Some(s.as_ref()),
            Token::HackerNewsItem(i) => match &**i {
                Item::Comment(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_user(&self) -> Option<&User> {
        match self {
            Token::HackerNewsUser(u) => Some(u.as_ref()),
            _ => None,
        }
    }

    pub fn as_crate(&self) -> Option<&Crate> {
        match self {
            Token::Crate(c) => Some(c.as_ref()),
            _ => None,
        }
    }

    pub fn as_webpage(&self) -> Option<&str> {
        match self {
            Token::GitHubRepository(r) => Some(r.url.as_ref()),
            Token::Repository(r) => Some(r.as_ref()),
            Token::Webpage(w) => Some(w.as_ref()),
            _ => None,
        }
    }

    pub fn as_repository(&self) -> Option<&str> {
        match self {
            Token::GitHubRepository(r) => Some(r.url.as_ref()),
            Token::Repository(r) => Some(r.as_ref()),
            _ => None,
        }
    }

    pub fn as_github_repository(&self) -> Option<&FullRepository> {
        match self {
            Token::GitHubRepository(r) => Some(r.as_ref()),
            _ => None,
        }
    }

    pub fn as_github_workflow(&self) -> Option<&Workflow> {
        match self {
            Token::GitHubWorkflow(w) => Some(w.as_ref()),
            _ => None,
        }
    }
}

impl From<Item> for Token {
    fn from(item: Item) -> Self {
        Self::HackerNewsItem(Rc::from(item))
    }
}

impl From<Story> for Token {
    fn from(s: Story) -> Self {
        Self::HackerNewsStory(Rc::from(s))
    }
}

impl From<Job> for Token {
    fn from(j: Job) -> Self {
        Self::HackerNewsJob(Rc::from(j))
    }
}

impl From<Comment> for Token {
    fn from(c: Comment) -> Self {
        Self::HackerNewsComment(Rc::from(c))
    }
}

impl From<User> for Token {
    fn from(u: User) -> Self {
        Self::HackerNewsUser(Rc::from(u))
    }
}

impl From<Crate> for Token {
    fn from(c: Crate) -> Self {
        Self::Crate(Rc::from(c))
    }
}

impl From<FullRepository> for Token {
    fn from(r: FullRepository) -> Self {
        Self::GitHubRepository(Rc::from(r))
    }
}

impl From<Workflow> for Token {
    fn from(w: Workflow) -> Self {
        Self::GitHubWorkflow(Rc::from(w))
    }
}
