use std::rc::Rc;

use consecrates::api::Crate;
use hn_api::types::{Comment, Item, Job, Story, User};
use octorust::types::FullRepository;

#[derive(Debug, Clone)]
pub enum Token {
    HackerNewsItem(Rc<Item>),
    HackerNewsStory(Rc<Story>),
    HackerNewsJob(Rc<Job>),
    HackerNewsComment(Rc<Comment>),
    HackerNewsUser(Rc<User>),
    Crate(Rc<Crate>),
    GitHubRepository(Rc<FullRepository>),
    GitHubWorkflow(),
    GitHubActionsJob(),
    GitHubActionsImportedStep(),
    GitHubActionsRunStep(),
    NameValuePair(Rc<(String, String)>),
}

impl Token {
    pub fn typename(&self) -> &'static str {
        match self {
            Token::HackerNewsItem(..) => "HackerNewsItem",
            Token::HackerNewsStory(..) => "HackerNewsStory",
            Token::HackerNewsJob(..) => "HackerNewsJob",
            Token::HackerNewsComment(..) => "HackerNewsComment",
            Token::HackerNewsUser(..) => "HackerNewsUser",
            Token::Crate(..) => "Crate",
            Token::GitHubRepository(..) => "GitHubRepository",
            Token::GitHubWorkflow(..) => "GitHubWorkflow",
            Token::GitHubActionsJob(..) => "GitHubActionsJob",
            Token::GitHubActionsImportedStep(..) => "GitHubActionsImportedStep",
            Token::GitHubActionsRunStep(..) => "GitHubActionsRunStep",
            Token::NameValuePair(..) => "NameValuePair",
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

    pub fn as_repository(&self) -> Option<&FullRepository> {
        match self {
            Token::GitHubRepository(r) => Some(r.as_ref()),
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
