use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

use super::util::QueryPath;
use crate::{
    ir::{Argument, FieldRef, Vid},
    util::{BTreeMapOccupiedError, BTreeMapTryInsertExt},
};

#[derive(Debug, Default)]
pub(super) struct TagHandler<'a> {
    tags: BTreeMap<&'a str, TagEntry<'a>>,
    used_tags: BTreeSet<&'a str>,
    component_imported_tags: Vec<(Vid, Vec<FieldRef>)>,
}

#[derive(Debug, Clone)]
pub(super) struct TagEntry<'a> {
    pub(super) name: &'a str,
    pub(super) field: FieldRef,
    pub(super) path: QueryPath,
}

impl<'a> TagEntry<'a> {
    fn new(name: &'a str, field: FieldRef, path: QueryPath) -> Self {
        Self { name, field, path }
    }

    pub(super) fn create_tag_argument(&self, use_path: &QueryPath) -> Argument {
        let overridden_type = 'override_type: {
            let underlying_type = self.field.field_type();
            if !underlying_type.nullable() {
                for (_, modifier) in self.path.diff_suffix(use_path) {
                    if modifier.is_optional() {
                        break 'override_type Some(underlying_type.with_nullability(true));
                    }
                }
                break 'override_type None;
            } else {
                break 'override_type None;
            }
        };

        Argument::Tag(self.field.to_owned(), overridden_type)
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
        field: FieldRef,
        path: &QueryPath,
    ) -> Result<(), BTreeMapOccupiedError<'_, &'a str, TagEntry<'a>>> {
        self.tags.insert_or_error(name, TagEntry::new(name, field, path.clone()))?;

        Ok(())
    }

    pub(super) fn begin_subcomponent(&mut self, component_root: Vid) {
        self.component_imported_tags.push((component_root, vec![]));
    }

    pub(super) fn end_subcomponent(&mut self, component_root: Vid) -> Vec<FieldRef> {
        let (expected_vid, external_tags) = self.component_imported_tags.pop().unwrap();
        assert_eq!(expected_vid, component_root);
        external_tags
    }

    pub(super) fn reference_tag(
        &mut self,
        name: &str,
        use_path: &QueryPath,
        use_vid: Vid,
    ) -> Result<&TagEntry<'_>, TagLookupError> {
        let entry =
            self.tags.get(name).ok_or_else(|| TagLookupError::UndefinedTag(name.to_string()))?;

        if entry.path.is_same_or_parent_component(use_path) {
            if entry.field.defined_at() > use_vid {
                return Err(TagLookupError::TagUsedBeforeDefinition(name.to_string()));
            }

            let tag_components = entry.path.component_roots();
            let use_components = use_path.component_roots();
            if tag_components != use_components {
                // The tag is used inside a fold and imported from an outer component.
                // Mark it as imported at the appropriate level.
                let importing_component_root = use_components[entry.path.component_len()];

                // The -1 in the index calculation is because the root component
                // cannot import tags -- it has no parent component to import from.
                let (component_root, imported_tags) =
                    self.component_imported_tags.get_mut(entry.path.component_len() - 1).unwrap();
                assert_eq!(*component_root, importing_component_root);
                imported_tags.push(entry.field.clone());
            }

            self.used_tags.insert(entry.name);
            Ok(entry)
        } else {
            // The tag is defined in a fold that is either inside of, or parallel to,
            // the component that uses the tag. This is not allowed.
            Err(TagLookupError::TagDefinedInsideFold(name.to_string()))
        }
    }

    pub(super) fn finish(self) -> Result<(), BTreeSet<&'a str>> {
        let unused_tags: BTreeSet<_> =
            self.tags.keys().copied().filter(|x| !self.used_tags.contains(x)).collect();
        if unused_tags.is_empty() {
            Ok(())
        } else {
            Err(unused_tags)
        }
    }
}

pub(super) enum TagLookupError {
    UndefinedTag(String),
    TagUsedBeforeDefinition(String),
    TagDefinedInsideFold(String),
}
