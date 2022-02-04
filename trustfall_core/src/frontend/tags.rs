use std::{collections::{BTreeMap, btree_map::OccupiedError}, fmt::Debug};

use crate::ir::{ContextField, Vid};
use super::util::ComponentPath;

#[derive(Debug, Default)]
pub(super) struct TagHandler<'a> {
    tags: BTreeMap<&'a str, TagEntry>,
}

#[derive(Debug, Clone)]
pub(super) struct TagEntry {
    pub(super) field: ContextField,
    pub(super) path: ComponentPath,
}

impl TagEntry {
    fn new(field: ContextField, path: ComponentPath) -> Self {
        Self { field, path }
    }
}

impl<'a> TagHandler<'a> {
    #[inline]
    pub(super) fn new() -> Self {
        Default::default()
    }

    pub(super) fn register_tag(&mut self, name: &'a str, field: ContextField, path: &ComponentPath) -> Result<(), OccupiedError<'_, &'a str, TagEntry>> {
        self.tags.try_insert(name, TagEntry::new(field, path.clone()))?;

        Ok(())
    }

    pub(super) fn look_up_tag(&self, name: &'a str, use_path: &ComponentPath, use_vid: Vid) -> Result<&TagEntry, TagLookupError> {
        let entry = self.tags.get(name).ok_or_else(|| TagLookupError::UndefinedTag(name.to_string()))?;

        if entry.path.is_parent(use_path) {
            if entry.field.vertex_id <= use_vid {
                Ok(entry)
            } else {
                Err(TagLookupError::TagUsedBeforeDefinition(name.to_string()))
            }
        } else {
            // The tag is defined in a fold that is either inside of, or parallel to,
            // the component that uses the tag. This is not allowed.
            Err(TagLookupError::TagDefinedInsideFold(name.to_string()))
        }
    }
}

pub(super) enum TagLookupError {
    UndefinedTag(String),
    TagUsedBeforeDefinition(String),
    TagDefinedInsideFold(String),
}
