use std::fmt::Debug;

use crate::{
    interpreter::{helpers::resolve_typename, DataContext, Typename},
    ir::FieldValue,
    schema::Schema,
};

#[test]
fn typename_resolved_statically() {
    #[derive(Debug, Clone)]
    enum Vertex {
        Variant,
    }

    impl Typename for Vertex {
        fn typename(&self) -> &'static str {
            unreachable!("typename() was called, so __typename was not resolved statically")
        }
    }

    let schema = Schema::parse(
        "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) repeatable on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD

type RootSchemaQuery {
    Vertex: Vertex!
}

type Vertex {
    field: Int
}",
    )
    .expect("failed to parse schema");
    let contexts = Box::new(std::iter::once(DataContext::new(Some(Vertex::Variant))));

    let outputs: Vec<_> =
        resolve_typename(contexts, &schema, "Vertex").map(|(_ctx, value)| value).collect();

    assert_eq!(vec![FieldValue::from("Vertex")], outputs);
}

mod correctness {
    use crate::numbers_interpreter::NumbersAdapter;

    #[test]
    fn correctness_checker_approves_valid_adapter() {
        let adapter = NumbersAdapter::new();
        let schema = adapter.schema().clone();

        super::super::correctness::check_adapter_invariants(&schema, adapter)
    }

    /// Failing to implement a portion of the adapter's stated schema is a bug.
    /// This module ensures that bug is reported.
    mod unimplemented_schema {
        use std::sync::Arc;

        use crate::{
            interpreter::{
                Adapter, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
                VertexIterator,
            },
            ir::{EdgeParameters, FieldValue},
            numbers_interpreter::NumbersAdapter,
        };

        #[test]
        #[should_panic(expected = "oops! we forgot to implement __typename on Named")]
        fn forget_to_implement_typename_property() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    if type_name.as_ref() == "Named" && property_name.as_ref() == "__typename" {
                        panic!("oops! we forgot to implement __typename on Named");
                    }

                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }

        #[test]
        #[should_panic(expected = "oops! we forgot to implement predecessor edge on type Neither")]
        fn forget_to_implement_edge() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    if type_name.as_ref() == "Neither" && edge_name.as_ref() == "predecessor" {
                        panic!("oops! we forgot to implement predecessor edge on type Neither");
                    }

                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }

        #[test]
        #[should_panic(expected = "oops! we forgot to implement coercion from Named to Number")]
        fn forget_to_implement_coercion() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    if type_name.as_ref() == "Named" && coerce_to_type.as_ref() == "Number" {
                        panic!("oops! we forgot to implement coercion from Named to Number");
                    }

                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }
    }

    /// Failing to resolve all contexts that were supplied in the input iterator of a resolver fn
    /// is a bug. This module ensures it is correctly caught and reported.
    mod lost_contexts {
        use std::sync::Arc;

        use crate::{
            interpreter::{
                Adapter, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
                VertexIterator,
            },
            ir::{EdgeParameters, FieldValue},
            numbers_interpreter::NumbersAdapter,
        };

        #[test]
        #[should_panic(expected = "adapter lost 1 contexts inside resolve_property() for \
                       type name 'Named' and property '__typename'")]
        fn when_resolving_property() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    mut contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    if type_name.as_ref() == "Named" && property_name.as_ref() == "__typename" {
                        // This is a context we consume from the input
                        // but don't return in the output iterator.
                        let _ = contexts.next();
                    }

                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }

        #[test]
        #[should_panic(
            expected = "adapter lost 1 contexts inside resolve_neighbors() for type 'Neither' \
                       edge 'predecessor' with parameters {}"
        )]
        fn when_resolving_neighbors() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    mut contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    if type_name.as_ref() == "Neither" && edge_name.as_ref() == "predecessor" {
                        // This is a context we consume from the input
                        // but don't return in the output iterator.
                        let _ = contexts.next();
                    }

                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }

        #[test]
        #[should_panic(expected = "adapter lost 1 contexts inside resolve_coercion() \
                       for type_name 'Named' and coerce_to_type 'Number'")]
        fn when_resolving_coercion() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    mut contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    if type_name.as_ref() == "Named" && coerce_to_type.as_ref() == "Number" {
                        // This is a context we consume from the input
                        // but don't return in the output iterator.
                        let _ = contexts.next();
                    }

                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }
    }

    /// Reordering contexts between the input and output iterators of adapter resolvers
    /// is a bug. This module ensures it is correctly caught and reported.
    mod reordered_contexts {
        use std::sync::Arc;

        use crate::{
            interpreter::{
                Adapter, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
                VertexIterator,
            },
            ir::{EdgeParameters, FieldValue},
            numbers_interpreter::NumbersAdapter,
        };

        #[test]
        #[should_panic(
            expected = "adapter illegally reordered contexts inside resolve_property() \
                       for type name 'Named' and property '__typename'"
        )]
        fn when_resolving_property() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    if type_name.as_ref() == "Named" && property_name.as_ref() == "__typename" {
                        let mut all_contexts: Vec<_> = contexts.collect();
                        let popped = all_contexts.swap_remove(3);
                        all_contexts.push(popped);
                        self.inner.resolve_property(
                            Box::new(all_contexts.into_iter()),
                            type_name,
                            property_name,
                            resolve_info,
                        )
                    } else {
                        self.inner.resolve_property(
                            contexts,
                            type_name,
                            property_name,
                            resolve_info,
                        )
                    }
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }

        #[test]
        #[should_panic(
            expected = "adapter illegally reordered contexts inside resolve_neighbors() \
                       for type 'Neither' edge 'predecessor' with parameters {}"
        )]
        fn when_resolving_neighbors() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    if type_name.as_ref() == "Neither" && edge_name.as_ref() == "predecessor" {
                        let mut all_contexts: Vec<_> = contexts.collect();
                        let popped = all_contexts.swap_remove(3);
                        all_contexts.push(popped);
                        self.inner.resolve_neighbors(
                            Box::new(all_contexts.into_iter()),
                            type_name,
                            edge_name,
                            parameters,
                            resolve_info,
                        )
                    } else {
                        self.inner.resolve_neighbors(
                            contexts,
                            type_name,
                            edge_name,
                            parameters,
                            resolve_info,
                        )
                    }
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }

        #[test]
        #[should_panic(
            expected = "adapter illegally reordered contexts inside resolve_coercion() for \
                       type_name 'Named' and coerce_to_type 'Number'"
        )]
        fn when_resolving_coercion() {
            struct AdapterWrapper {
                inner: NumbersAdapter,
            }

            impl<'a> Adapter<'a> for AdapterWrapper {
                type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;

                fn resolve_starting_vertices(
                    &self,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveInfo,
                ) -> VertexIterator<'a, Self::Vertex> {
                    self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
                }

                fn resolve_property(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    property_name: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                    self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
                }

                fn resolve_neighbors(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    edge_name: &Arc<str>,
                    parameters: &EdgeParameters,
                    resolve_info: &ResolveEdgeInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
                {
                    self.inner.resolve_neighbors(
                        contexts,
                        type_name,
                        edge_name,
                        parameters,
                        resolve_info,
                    )
                }

                fn resolve_coercion(
                    &self,
                    contexts: ContextIterator<'a, Self::Vertex>,
                    type_name: &Arc<str>,
                    coerce_to_type: &Arc<str>,
                    resolve_info: &ResolveInfo,
                ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                    if type_name.as_ref() == "Named" && coerce_to_type.as_ref() == "Number" {
                        let mut all_contexts: Vec<_> = contexts.collect();
                        let popped = all_contexts.swap_remove(3);
                        all_contexts.push(popped);
                        self.inner.resolve_coercion(
                            Box::new(all_contexts.into_iter()),
                            type_name,
                            coerce_to_type,
                            resolve_info,
                        )
                    } else {
                        self.inner.resolve_coercion(
                            contexts,
                            type_name,
                            coerce_to_type,
                            resolve_info,
                        )
                    }
                }
            }

            let adapter = AdapterWrapper { inner: NumbersAdapter::new() };
            let schema = adapter.inner.schema().clone();

            super::super::super::correctness::check_adapter_invariants(&schema, adapter)
        }
    }
}
