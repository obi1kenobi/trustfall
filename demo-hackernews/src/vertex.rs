use std::rc::Rc;

use hn_api::types::{Comment, Item, Job, Story, User};

#[derive(Debug, Clone)]
pub enum Vertex {
    Item(Rc<Item>),
    Story(Rc<Story>),
    Job(Rc<Job>),
    Comment(Rc<Comment>),
    User(Rc<User>),
}

impl Vertex {
    pub fn typename(&self) -> &'static str {
        match self {
            Vertex::Item(..) => "Item",
            Vertex::Story(..) => "Story",
            Vertex::Job(..) => "Job",
            Vertex::Comment(..) => "Comment",
            Vertex::User(..) => "User",
        }
    }

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

    pub fn as_user(&self) -> Option<&User> {
        match self {
            Vertex::User(u) => Some(u.as_ref()),
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
