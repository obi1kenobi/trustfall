use std::rc::Rc;

use hn_api::types::{Comment, Item, Job, Poll, Pollopt, Story, User};
use trustfall::provider::TrustfallEnumVertex;

#[derive(Debug, Clone, TrustfallEnumVertex)]
pub enum Vertex {
    Story(Rc<Story>),
    Job(Rc<Job>),
    Comment(Rc<Comment>),
    Poll(Rc<Poll>),
    PollOption(Rc<Pollopt>),
    User(Rc<User>),
}

impl From<Item> for Vertex {
    fn from(item: Item) -> Self {
        match item {
            Item::Story(x) => Self::Story(x.into()),
            Item::Comment(x) => Self::Comment(x.into()),
            Item::Job(x) => Self::Job(x.into()),
            Item::Poll(x) => Self::Poll(x.into()),
            Item::Pollopt(x) => Self::PollOption(x.into()),
        }
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
