use std::{collections::BTreeMap, sync::Arc};

use crate::ir::{FieldRef, Vid};

#[derive(Debug)]
pub(super) struct OutputHandler<'query> {
    prefixes: BTreeMap<Vid, Option<&'query str>>,
    vid_stack: Vec<Vid>,
    root_vid: Vid,
    root_prefix: Option<&'query str>,
    component_outputs_stack: Vec<BTreeMap<Arc<str>, Vec<FieldRef>>>,
    global_outputs: BTreeMap<Arc<str>, Vec<FieldRef>>,
}

impl<'query> OutputHandler<'query> {
    pub(super) fn new(root_vid: Vid, root_prefix: Option<&'query str>) -> Self {
        Self {
            prefixes: Default::default(),
            vid_stack: Default::default(),
            root_vid,
            root_prefix,
            component_outputs_stack: Default::default(),
            global_outputs: Default::default(),
        }
    }

    pub(super) fn begin_nested_scope(&mut self, nested_vid: Vid, prefix: Option<&'query str>) {
        let stack_top_vid = *self.vid_stack.last().unwrap_or(&self.root_vid);
        self.vid_stack.push(nested_vid);

        let prior_value = self.prefixes.insert(nested_vid, prefix);
        assert!(prior_value.is_none());
    }

    pub(super) fn end_nested_scope(&mut self, nested_vid: Vid) {
        let stack_top_vid = self.vid_stack.pop().expect("stack was unexpectedly empty");
        assert_eq!(nested_vid, stack_top_vid);
    }

    pub(super) fn begin_subcomponent(&mut self) {
        self.component_outputs_stack.push(Default::default())
    }

    pub(super) fn end_subcomponent(&mut self) -> BTreeMap<Arc<str>, Vec<FieldRef>> {
        self.component_outputs_stack.pop().expect("stack was unexpectedly empty")
    }

    fn make_output_name<'a>(
        &self,
        local_name: &str,
        transforms: impl Iterator<Item = &'a str> + 'a,
    ) -> Arc<str> {
        let mut name = String::with_capacity(16);
        if let Some(prefix) = &self.root_prefix {
            name.push_str(prefix);
        }
        for vid in &self.vid_stack {
            if let Some(prefix) = &self.prefixes[vid] {
                name.push_str(prefix);
            }
        }

        name.push_str(local_name);

        for suffix in transforms {
            name.push_str(suffix);
        }

        Arc::from(name)
    }

    fn register_output(&mut self, name: Arc<str>, value: FieldRef) {
        self.component_outputs_stack
            .last_mut()
            .expect("stack was unexpectedly empty")
            .entry(name.clone())
            .or_default()
            .push(value.clone());

        self.global_outputs.entry(name).or_default().push(value);
    }

    pub(super) fn register_locally_named_output<'a>(
        &mut self,
        local_name: &str,
        transforms: Option<Box<dyn Iterator<Item = &'a str> + 'a>>,
        value: FieldRef,
    ) -> Arc<str> {
        let complete_name = self.make_output_name(local_name, transforms.into_iter().flatten());
        self.register_output(complete_name.clone(), value);
        complete_name
    }

    pub(super) fn register_explicitly_named_output(
        &mut self,
        explicit_name: Arc<str>,
        value: FieldRef,
    ) {
        self.register_output(explicit_name, value)
    }

    pub(crate) fn finish(self) -> BTreeMap<Arc<str>, Vec<FieldRef>> {
        assert!(self.vid_stack.is_empty());
        assert!(self.component_outputs_stack.is_empty());

        self.global_outputs
    }
}
