use std::ops::Index;

use async_graphql_parser::types::{BaseType, Type};
use async_graphql_value::Name;

use crate::ir::Vid;

pub(super) fn get_underlying_named_type(t: &Type) -> &Name {
    let mut base_type = &t.base;
    loop {
        match base_type {
            BaseType::Named(n) => return n,
            BaseType::List(l) => base_type = &l.base,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ComponentPath {
    path: Vec<Vid>,
}

impl ComponentPath {
    pub(super) fn new(starting_vid: Vid) -> Self {
        Self {
            path: vec![starting_vid],
        }
    }

    #[inline(always)]
    pub(super) fn len(&self) -> usize {
        self.path.len()
    }

    pub(super) fn push(&mut self, component_start_vid: Vid) {
        self.path.push(component_start_vid);
    }

    pub(super) fn pop(&mut self, component_start_vid: Vid) {
        let popped_vid = self.path.pop().unwrap();
        assert_eq!(popped_vid, component_start_vid);
    }

    pub(super) fn is_parent(&self, other: &ComponentPath) -> bool {
        let self_len = self.path.len();
        let other_len = other.path.len();

        if self_len <= other_len {
            let other_slice = &other.path[..self_len];
            self.path == other_slice
        } else {
            false
        }
    }

    pub(super) fn is_component_root(&self, vid: Vid) -> bool {
        self.path.last().expect("empty component path") == &vid
    }
}

impl Index<usize> for ComponentPath {
    type Output = Vid;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        &self.path[index]
    }
}
