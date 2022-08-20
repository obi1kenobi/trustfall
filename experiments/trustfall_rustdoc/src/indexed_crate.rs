use std::collections::HashMap;

use rustdoc_types::{Crate, Id, Item, Visibility};

#[derive(Debug, Clone)]
pub struct IndexedCrate<'a> {
    pub(crate) inner: &'a Crate,

    // For an Id, give the list of item Ids under which it is publicly visible.
    pub(crate) visibility_forest: HashMap<&'a Id, Vec<&'a Id>>,
}

impl<'a> IndexedCrate<'a> {
    pub fn new(crate_: &'a Crate) -> Self {
        let visibility_forest = calculate_visibility_forest(crate_);

        Self {
            inner: crate_,
            visibility_forest,
        }
    }

    pub fn publicly_importable_names(&self, id: &'a Id) -> Vec<Vec<&'a str>> {
        let mut result = vec![];

        if self.inner.index.contains_key(id) {
            self.collect_publicly_importable_names(id, &mut vec![], &mut result);
        }

        result
    }

    fn collect_publicly_importable_names(
        &self,
        next_id: &'a Id,
        stack: &mut Vec<&'a str>,
        output: &mut Vec<Vec<&'a str>>,
    ) {
        let item = &self.inner.index[next_id];
        if let Some(item_name) = item.name.as_deref() {
            stack.push(item_name);
        } else {
            assert!(
                matches!(item.inner, rustdoc_types::ItemEnum::Import(..)),
                "{item:?}"
            );
        }

        if next_id == &self.inner.root {
            let final_name = stack.iter().rev().copied().collect();
            output.push(final_name);
        } else if let Some(visible_parents) = self.visibility_forest.get(next_id) {
            for parent_id in visible_parents.iter().copied() {
                self.collect_publicly_importable_names(parent_id, stack, output);
            }
        }

        if let Some(item_name) = item.name.as_deref() {
            let popped_item = stack.pop().expect("stack was unexpectedly empty");
            assert_eq!(item_name, popped_item);
        }
    }
}

fn calculate_visibility_forest(crate_: &Crate) -> HashMap<&Id, Vec<&Id>> {
    let mut result = Default::default();
    let root_id = &crate_.root;
    if let Some(root_module) = crate_.index.get(root_id) {
        if root_module.visibility == Visibility::Public {
            collect_public_items(crate_, &mut result, root_module, None);
        }
    }

    result
}

fn collect_public_items<'a>(
    crate_: &'a Crate,
    pub_items: &mut HashMap<&'a Id, Vec<&'a Id>>,
    item: &'a Item,
    parent_id: Option<&'a Id>,
) {
    match item.visibility {
        // Some impls and methods have default visibility:
        // they are visible only if the type to which they belong is visible.
        // However, we don't recurse into non-public items with this function, so
        // reachable items with default visibility must be public.
        Visibility::Public | Visibility::Default => {
            let parents = pub_items.entry(&item.id).or_default();
            if let Some(parent_id) = parent_id {
                parents.push(parent_id);
            }

            let next_parent_id = Some(&item.id);
            match &item.inner {
                rustdoc_types::ItemEnum::Module(m) => {
                    for inner in m.items.iter().filter_map(|id| crate_.index.get(id)) {
                        collect_public_items(crate_, pub_items, inner, next_parent_id);
                    }
                }
                rustdoc_types::ItemEnum::Import(imp) => {
                    // TODO: handle glob imports (`pub use foo::bar::*`) here.
                    if let Some(item) = imp.id.as_ref().and_then(|id| crate_.index.get(id)) {
                        collect_public_items(crate_, pub_items, item, next_parent_id);
                    }
                }
                rustdoc_types::ItemEnum::Struct(struct_) => {
                    for inner in struct_
                        .fields
                        .iter()
                        .chain(struct_.impls.iter())
                        .filter_map(|id| crate_.index.get(id))
                    {
                        collect_public_items(crate_, pub_items, inner, next_parent_id);
                    }
                }
                rustdoc_types::ItemEnum::Enum(enum_) => {
                    for inner in enum_
                        .variants
                        .iter()
                        .chain(enum_.impls.iter())
                        .filter_map(|id| crate_.index.get(id))
                    {
                        collect_public_items(crate_, pub_items, inner, next_parent_id);
                    }
                }
                rustdoc_types::ItemEnum::Trait(trait_) => {
                    for inner in trait_.items.iter().filter_map(|id| crate_.index.get(id)) {
                        collect_public_items(crate_, pub_items, inner, next_parent_id);
                    }
                }
                rustdoc_types::ItemEnum::Impl(impl_) => {
                    for inner in impl_.items.iter().filter_map(|id| crate_.index.get(id)) {
                        collect_public_items(crate_, pub_items, inner, next_parent_id);
                    }
                }
                _ => {
                    // No-op, no further items within to consider.
                }
            }
        }
        Visibility::Crate | Visibility::Restricted { .. } => {}
    }
}
