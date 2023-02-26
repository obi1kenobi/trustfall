use std::rc::Rc;

use hn_api::types::{Comment, Item, Job, Story, User};
use trustfall::provider::TrustfallEnumVertex;

#[derive(Debug, Clone, TrustfallEnumVertex)]
pub enum Vertex {
    Item(Rc<Item>),

    #[trustfall(skip_conversion)]
    Story(Rc<Story>),

    #[trustfall(skip_conversion)]
    Job(Rc<Job>),

    #[trustfall(skip_conversion)]
    Comment(Rc<Comment>),

    User(Rc<User>),
}

impl Vertex {
    pub fn as_story(&self) -> Option<&Story> {
        match self {
            Vertex::Story(s) => Some(s.as_ref()),
            Vertex::Item(i) => match &**i {
                Item::Story(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_job(&self) -> Option<&Job> {
        match self {
            Vertex::Job(s) => Some(s.as_ref()),
            Vertex::Item(i) => match &**i {
                Item::Job(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_comment(&self) -> Option<&Comment> {
        match self {
            Vertex::Comment(s) => Some(s.as_ref()),
            Vertex::Item(i) => match &**i {
                Item::Comment(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }
}

impl From<Item> for Vertex {
    fn from(item: Item) -> Self {
        Self::Item(Rc::from(item))
    }
}

impl From<Story> for Vertex {
    fn from(s: Story) -> Self {
        Self::Story(Rc::from(s))
    }
}

impl From<Job> for Vertex {
    fn from(j: Job) -> Self {
        Self::Job(Rc::from(j))
    }
}

impl From<Comment> for Vertex {
    fn from(c: Comment) -> Self {
        Self::Comment(Rc::from(c))
    }
}

impl From<User> for Vertex {
    fn from(u: User) -> Self {
        Self::User(Rc::from(u))
    }
}
