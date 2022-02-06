use std::{
    collections::{btree_map::OccupiedError, BTreeMap},
    fmt::Debug,
};

use super::util::ComponentPath;
use crate::ir::{ContextField, Vid};

#[derive(Debug, Default)]
pub(super) struct TagHandler<'a> {
    tags: BTreeMap<&'a str, TagEntry>,
    component_imported_tags: Vec<(Vid, Vec<ContextField>)>,
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

    pub(super) fn register_tag(
        &mut self,
        name: &'a str,
        field: ContextField,
        path: &ComponentPath,
    ) -> Result<(), OccupiedError<'_, &'a str, TagEntry>> {
        self.tags
            .try_insert(name, TagEntry::new(field, path.clone()))?;

        Ok(())
    }

    pub(super) fn begin_subcomponent(&mut self, component_root: Vid) {
        self.component_imported_tags.push((component_root, vec![]));
    }

    pub(super) fn end_subcomponent(&mut self, component_root: Vid) -> Vec<ContextField> {
        let (expected_vid, external_tags) = self.component_imported_tags.pop().unwrap();
        assert_eq!(expected_vid, component_root);
        external_tags
    }

    pub(super) fn reference_tag(
        &mut self,
        name: &str,
        use_path: &ComponentPath,
        use_vid: Vid,
    ) -> Result<&TagEntry, TagLookupError> {
        let entry = self
            .tags
            .get(name)
            .ok_or_else(|| TagLookupError::UndefinedTag(name.to_string()))?;

        if entry.path.is_parent(use_path) {
            if entry.field.vertex_id <= use_vid {
                if &entry.path != use_path {
                    // The tag is used inside a fold and imported from an outer component.
                    // Mark it as imported at the appropriate level.
                    let importing_component_root = use_path[entry.path.len()];

                    // The -1 in the index calculation is because the root component
                    // cannot import tags -- it has no parent component to import from.
                    let (component_root, imported_tags) = self
                        .component_imported_tags
                        .get_mut(entry.path.len() - 1)
                        .unwrap();
                    assert_eq!(*component_root, importing_component_root);
                    imported_tags.push(entry.field.clone());
                }
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
