use std::sync::{Arc, OnceLock};

use trustfall::{FieldValue, Schema, provider::{AsVertex, ContextIterator, ContextOutcomeIterator, EdgeParameters, ResolveEdgeInfo, ResolveInfo, Typename, VertexIterator, resolve_coercion_using_schema, resolve_property_with}};

use super::vertex::Vertex;

static SCHEMA: OnceLock<Schema> = OnceLock::new();

#[non_exhaustive]
#[derive(Debug)]
pub struct Adapter {}

impl Adapter {
    pub const SCHEMA_TEXT: &'static str = include_str!("./schema.graphql");

    pub fn schema() -> &'static Schema {
        SCHEMA
            .get_or_init(|| {
                Schema::parse(Self::SCHEMA_TEXT).expect("not a valid schema")
            })
    }

    pub fn new() -> Self {
        Self {}
    }
}

impl<'a> trustfall::provider::Adapter<'a> for Adapter {
    type Vertex = Vertex;
    type Error = std::convert::Infallible;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Result<Self::Vertex, Self::Error>> {
        match edge_name.as_ref() {
            "AskHN" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'AskHN' starting vertices",
                    )
                    .as_i64();
                Box::new(super::entrypoints::ask_hn(max, resolve_info).map(Ok))
            }
            "Best" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'Best' starting vertices",
                    )
                    .as_i64();
                Box::new(super::entrypoints::best(max, resolve_info).map(Ok))
            }
            "FrontPage" => Box::new(super::entrypoints::front_page(resolve_info).map(Ok)),
            "Item" => {
                let id: i64 = parameters
                    .get("id")
                    .expect(
                        "failed to find parameter 'id' when resolving 'Item' starting vertices",
                    )
                    .as_i64()
                    .expect(
                        "unexpected null or other incorrect datatype for Trustfall type 'Int!'",
                    );
                Box::new(super::entrypoints::item(id, resolve_info).map(Ok))
            }
            "Latest" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'Latest' starting vertices",
                    )
                    .as_i64();
                Box::new(super::entrypoints::latest(max, resolve_info).map(Ok))
            }
            "RecentJob" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'RecentJob' starting vertices",
                    )
                    .as_i64();
                Box::new(super::entrypoints::recent_job(max, resolve_info).map(Ok))
            }
            "SearchByDate" => {
                let query: &str = parameters
                    .get("query")
                    .expect(
                        "failed to find parameter 'query' when resolving 'SearchByDate' starting vertices",
                    )
                    .as_str()
                    .expect(
                        "unexpected null or other incorrect datatype for Trustfall type 'String!'",
                    );
                Box::new(super::entrypoints::search_by_date(query, resolve_info).map(Ok))
            }
            "SearchByRelevance" => {
                let query: &str = parameters
                    .get("query")
                    .expect(
                        "failed to find parameter 'query' when resolving 'SearchByRelevance' starting vertices",
                    )
                    .as_str()
                    .expect(
                        "unexpected null or other incorrect datatype for Trustfall type 'String!'",
                    );
                Box::new(
                    super::entrypoints::search_by_relevance(query, resolve_info).map(Ok),
                )
            }
            "ShowHN" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'ShowHN' starting vertices",
                    )
                    .as_i64();
                Box::new(super::entrypoints::show_hn(max, resolve_info).map(Ok))
            }
            "Top" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'Top' starting vertices",
                    )
                    .as_i64();
                Box::new(super::entrypoints::top(max, resolve_info).map(Ok))
            }
            "UpdatedItem" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'UpdatedItem' starting vertices",
                    )
                    .as_i64();
                Box::new(super::entrypoints::updated_item(max, resolve_info).map(Ok))
            }
            "UpdatedUserProfile" => {
                let max: Option<i64> = parameters
                    .get("max")
                    .expect(
                        "failed to find parameter 'max' when resolving 'UpdatedUserProfile' starting vertices",
                    )
                    .as_i64();
                Box::new(
                    super::entrypoints::updated_user_profile(max, resolve_info).map(Ok),
                )
            }
            "User" => {
                let name: &str = parameters
                    .get("name")
                    .expect(
                        "failed to find parameter 'name' when resolving 'User' starting vertices",
                    )
                    .as_str()
                    .expect(
                        "unexpected null or other incorrect datatype for Trustfall type 'String!'",
                    );
                Box::new(super::entrypoints::user(name, resolve_info).map(Ok))
            }
            _ => {
                unreachable!(
                    "attempted to resolve starting vertices for unexpected edge name: {edge_name}"
                )
            }
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, FieldValue, Self::Error> {
        if property_name.as_ref() == "__typename" {
            return resolve_property_with(contexts, |vertex| vertex.typename().into());
        }
        match type_name.as_ref() {
            "Comment" => {
                super::properties::resolve_comment_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "Item" => {
                super::properties::resolve_item_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "Job" => {
                super::properties::resolve_job_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "Story" => {
                super::properties::resolve_story_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "User" => {
                super::properties::resolve_user_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "Webpage" => {
                super::properties::resolve_webpage_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            _ => {
                unreachable!(
                    "attempted to read property '{property_name}' on unexpected type: {type_name}"
                )
            }
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<
        'a,
        V,
        VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
        Self::Error,
    > {
        match type_name.as_ref() {
            "Comment" => {
                Box::new(
                    super::edges::resolve_comment_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|outcome| {
                            outcome
                                .map(|(ctx, neighbors)| (
                                    ctx,
                                    Box::new(neighbors.map(Ok))
                                        as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                                ))
                        }),
                )
            }
            "Job" => {
                Box::new(
                    super::edges::resolve_job_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|outcome| {
                            outcome
                                .map(|(ctx, neighbors)| (
                                    ctx,
                                    Box::new(neighbors.map(Ok))
                                        as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                                ))
                        }),
                )
            }
            "Story" => {
                Box::new(
                    super::edges::resolve_story_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|outcome| {
                            outcome
                                .map(|(ctx, neighbors)| (
                                    ctx,
                                    Box::new(neighbors.map(Ok))
                                        as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                                ))
                        }),
                )
            }
            "User" => {
                Box::new(
                    super::edges::resolve_user_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|outcome| {
                            outcome
                                .map(|(ctx, neighbors)| (
                                    ctx,
                                    Box::new(neighbors.map(Ok))
                                        as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                                ))
                        }),
                )
            }
            _ => {
                unreachable!(
                    "attempted to resolve edge '{edge_name}' on unexpected type: {type_name}"
                )
            }
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        _type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, bool, Self::Error> {
        resolve_coercion_using_schema(contexts, Self::schema(), coerce_to_type.as_ref())
    }
}
