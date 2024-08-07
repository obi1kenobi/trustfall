use async_graphql_parser::types::{BaseType, Type};
use async_graphql_value::Name;

use crate::ir::Vid;

/// Retrieves the underlying type name by looping through any [list
/// types](BaseType::List) until a [named type](Type) is found
pub(super) fn get_underlying_named_type(t: &Type) -> &Name {
    let mut base_type = &t.base;
    loop {
        match base_type {
            BaseType::Named(n) => return n,
            BaseType::List(l) => base_type = &l.base,
        }
    }
}

/// The list of component root vertices that have to be traversed in order to reach
/// the component that contains the location of interest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ComponentPath {
    path: Vec<Vid>,
}

impl ComponentPath {
    pub(super) fn new(starting_vid: Vid) -> Self {
        Self { path: vec![starting_vid] }
    }

    #[inline(always)]
    pub(super) fn len(&self) -> usize {
        self.path.len()
    }

    pub(super) fn push(&mut self, component_start_vid: Vid) {
        self.path.push(component_start_vid);
    }

    /// Pops the last component of the current path.
    ///
    /// Will panic if the popped value is not the provided `component_start_vid`
    pub(super) fn pop(&mut self, component_start_vid: Vid) {
        let popped_vid = self.path.pop().unwrap();
        debug_assert_eq!(popped_vid, component_start_vid);
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct QueryPath {
    component_path: ComponentPath,
    vertices: Vec<(Vid, TraversalModifier)>,
}

impl QueryPath {
    pub(super) fn new(starting_vid: Vid) -> Self {
        Self {
            component_path: ComponentPath::new(starting_vid),
            vertices: vec![(starting_vid, TraversalModifier::NONE)],
        }
    }

    pub(super) fn component_roots(&self) -> &[Vid] {
        self.component_path.path.as_slice()
    }

    pub(super) fn is_component_root(&self, vid: Vid) -> bool {
        self.component_path.is_component_root(vid)
    }

    pub(super) fn push_fold(&mut self, fold_root_vid: Vid) {
        self.component_path.push(fold_root_vid);
        self.vertices.push((fold_root_vid, TraversalModifier::FOLDED));
    }

    pub(super) fn pop_fold(&mut self, fold_root_vid: Vid) {
        let (vid, modifier) = self.vertices.pop().expect("no vertex to pop, this is a bug");
        debug_assert_eq!(vid, fold_root_vid, "unexpected vertex was popped: {fold_root_vid:?} != {vid:?} with modifier {modifier:?} for {self:?}");
        debug_assert!(modifier.is_folded());

        self.component_path.pop(fold_root_vid);
    }

    pub(super) fn push_traversal(&mut self, next_vid: Vid, optional: bool) {
        let modifier = if optional { TraversalModifier::OPTIONAL } else { TraversalModifier::NONE };
        self.vertices.push((next_vid, modifier));
    }

    pub(super) fn pop_traversal(&mut self, pop_vid: Vid) {
        let (vid, modifier) = self.vertices.pop().expect("no vertex to pop, this is a bug");
        debug_assert_eq!(vid, pop_vid, "unexpected vertex was popped: {pop_vid:?} != {vid:?} with modifier {modifier:?} for {self:?}");
    }

    pub(super) fn is_same_or_parent_component(&self, other: &QueryPath) -> bool {
        self.component_path.is_parent(&other.component_path)
    }

    pub(super) fn component_len(&self) -> usize {
        self.component_path.len()
    }

    pub(super) fn diff_suffix(&self, other: &QueryPath) -> &'_ [(Vid, TraversalModifier)] {
        let slice = self.vertices.as_slice();
        let mut iter = slice.iter();
        let mut count = 0;

        for (diff_vid, diff_modifier) in &other.vertices {
            let Some((vid, modifier)) = iter.next() else {
                break;
            };

            if vid == diff_vid {
                debug_assert_eq!(modifier, diff_modifier, "found the same vid with different modifiers in two QueryPath values: {self:?} {other:?}");
                count += 1;
            } else {
                break;
            }
        }

        slice.split_at(count).1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TraversalModifier {
    mask: i8,
}

impl TraversalModifier {
    const OPTIONAL_MASK: i8 = 1;
    const FOLDED_MASK: i8 = 2;

    pub(super) const NONE: Self = Self { mask: 0 };

    pub(super) const OPTIONAL: Self = Self { mask: Self::OPTIONAL_MASK };

    pub(super) const FOLDED: Self = Self { mask: Self::FOLDED_MASK };

    #[inline]
    pub(super) fn is_optional(&self) -> bool {
        (self.mask & Self::OPTIONAL_MASK) != 0
    }

    #[inline]
    pub(super) fn is_folded(&self) -> bool {
        (self.mask & Self::FOLDED_MASK) != 0
    }
}
