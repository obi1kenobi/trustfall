use std::rc::Rc;

use hn_api::types::{Comment, Item, Job, Story, User};

#[derive(Debug, Clone)]
pub enum Token {
    Item(Rc<Item>),
    Story(Rc<Story>),
    Job(Rc<Job>),
    Comment(Rc<Comment>),
    User(Rc<User>),
}

impl Token {
    pub fn typename(&self) -> &'static str {
        match self {
            Token::Item(..) => "Item",
            Token::Story(..) => "Story",
            Token::Job(..) => "Job",
            Token::Comment(..) => "Comment",
            Token::User(..) => "User",
        }
    }

    pub fn as_story(&self) -> Option<&Story> {
        match self {
            Token::Story(s) => Some(s.as_ref()),
            Token::Item(i) => match &**i {
                Item::Story(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_job(&self) -> Option<&Job> {
        match self {
            Token::Job(s) => Some(s.as_ref()),
            Token::Item(i) => match &**i {
                Item::Job(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_comment(&self) -> Option<&Comment> {
        match self {
            Token::Comment(s) => Some(s.as_ref()),
            Token::Item(i) => match &**i {
                Item::Comment(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }
}

impl From<Item> for Token {
    fn from(item: Item) -> Self {
        Self::Item(Rc::from(item))
    }
}

impl From<Story> for Token {
    fn from(s: Story) -> Self {
        Self::Story(Rc::from(s))
    }
}

impl From<Job> for Token {
    fn from(j: Job) -> Self {
        Self::Job(Rc::from(j))
    }
}

impl From<Comment> for Token {
    fn from(c: Comment) -> Self {
        Self::Comment(Rc::from(c))
    }
}

impl From<User> for Token {
    fn from(u: User) -> Self {
        Self::User(Rc::from(u))
    }
}
