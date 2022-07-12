#![allow(dead_code)]

use std::fmt::Debug;
use std::sync::Arc;
use std::{collections::BTreeMap, convert::TryFrom};

use async_graphql_parser::types::Directive;
use async_graphql_parser::{
    types::{DocumentOperations, ExecutableDocument, Field, OperationType, Selection},
    Pos, Positioned,
};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ir::FieldValue;
use crate::util::BTreeMapTryInsertExt;

use super::directives::{FoldGroup, TransformDirective, TransformGroup};
use super::{
    directives::{
        FilterDirective, FoldDirective, OptionalDirective, OutputDirective, RecurseDirective,
        TagDirective,
    },
    error::ParseError,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FieldConnection {
    pub position: Pos,
    pub name: Arc<str>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<Arc<str>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub arguments: BTreeMap<Arc<str>, FieldValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<OptionalDirective>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurse: Option<RecurseDirective>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fold: Option<FoldGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FieldNode {
    pub position: Pos,
    pub name: Arc<str>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<Arc<str>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coerced_to: Option<Arc<str>>,

    #[serde(default, skip_serializing_if = "SmallVec::is_empty")]
    pub filter: SmallVec<[FilterDirective; 1]>,

    #[serde(default, skip_serializing_if = "SmallVec::is_empty")]
    pub output: SmallVec<[OutputDirective; 1]>,

    #[serde(default, skip_serializing_if = "SmallVec::is_empty")]
    pub tag: SmallVec<[TagDirective; 0]>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<(FieldConnection, FieldNode)>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform_group: Option<TransformGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Query {
    pub root_connection: FieldConnection,

    pub root_field: FieldNode,
}

#[derive(Debug, Clone)]
enum ParsedDirective {
    Filter(FilterDirective, Pos),
    Fold(FoldDirective, Pos),
    Optional(OptionalDirective, Pos),
    Output(OutputDirective, Pos),
    Recurse(RecurseDirective, Pos),
    Tag(TagDirective, Pos),
    Transform(TransformDirective, Pos),
}

impl ParsedDirective {
    fn kind(&self) -> &str {
        match self {
            ParsedDirective::Filter(..) => "@filter",
            ParsedDirective::Fold(..) => "@fold",
            ParsedDirective::Optional(..) => "@optional",
            ParsedDirective::Output(..) => "@output",
            ParsedDirective::Recurse(..) => "@recurse",
            ParsedDirective::Tag(..) => "@tag",
            ParsedDirective::Transform(..) => "@transform",
        }
    }

    fn pos(&self) -> Pos {
        match self {
            ParsedDirective::Filter(_, pos) => *pos,
            ParsedDirective::Fold(_, pos) => *pos,
            ParsedDirective::Optional(_, pos) => *pos,
            ParsedDirective::Output(_, pos) => *pos,
            ParsedDirective::Recurse(_, pos) => *pos,
            ParsedDirective::Tag(_, pos) => *pos,
            ParsedDirective::Transform(_, pos) => *pos,
        }
    }
}

fn try_get_query_root(document: &ExecutableDocument) -> Result<&Positioned<Field>, ParseError> {
    if let Some(v) = document.fragments.values().next() {
        return Err(ParseError::DocumentContainsNonInlineFragments(v.pos));
    }

    match &document.operations {
        DocumentOperations::Multiple(mult) => {
            return Err(ParseError::MultipleOperationsInDocument(
                mult.values().next().unwrap().pos,
            ))
        }
        DocumentOperations::Single(op) => {
            let root_node = &op.node;

            if root_node.ty != OperationType::Query {
                return Err(ParseError::DocumentNotAQuery(op.pos));
            }

            if !root_node.variable_definitions.is_empty() {
                let first_variable_definition = root_node.variable_definitions.first().unwrap();
                return Err(ParseError::OtherError(
                    "Found GraphQL query variable definitions. \
                    These are not necessary since variables are defined implicitly, \
                    and must be removed."
                        .to_string(),
                    first_variable_definition.pos,
                ));
            }
            if !root_node.directives.is_empty() {
                let first_directive = root_node.directives.first().unwrap();
                return Err(ParseError::DirectiveNotInsideQueryRoot(
                    first_directive.node.name.node.to_string(),
                    first_directive.pos,
                ));
            }

            let root_selection_set = &root_node.selection_set.node;
            let root_items = &root_selection_set.items;
            if root_items.len() != 1 {
                return Err(ParseError::MultipleQueryRoots(root_items[1].pos));
            }

            let root_node = root_items.first().unwrap();
            match &root_node.node {
                Selection::Field(positioned_field) => Ok(positioned_field),
                Selection::FragmentSpread(fs) => Err(ParseError::UnsupportedQueryRoot(
                    "a fragment spread".to_string(),
                    fs.pos,
                )),
                Selection::InlineFragment(inl) => Err(ParseError::UnsupportedQueryRoot(
                    "an inline fragment".to_string(),
                    inl.pos,
                )),
            }
        }
    }
}

fn make_directives(
    directives: &[Positioned<Directive>],
) -> Result<Vec<ParsedDirective>, ParseError> {
    let mut parsed_directives = vec![];

    for directive in directives {
        match directive.node.name.node.as_str() {
            "filter" => {
                let parsed = FilterDirective::try_from(directive)?;
                parsed_directives.push(ParsedDirective::Filter(parsed, directive.pos));
            }
            "output" => {
                let parsed = OutputDirective::try_from(directive)?;
                parsed_directives.push(ParsedDirective::Output(parsed, directive.pos));
            }
            "tag" => {
                let parsed = TagDirective::try_from(directive)?;
                parsed_directives.push(ParsedDirective::Tag(parsed, directive.pos));
            }
            "transform" => {
                let parsed = TransformDirective::try_from(directive)?;
                parsed_directives.push(ParsedDirective::Transform(parsed, directive.pos));
            }
            "optional" => {
                let parsed = OptionalDirective::try_from(directive)?;
                parsed_directives.push(ParsedDirective::Optional(parsed, directive.pos));
            }
            "recurse" => {
                let parsed = RecurseDirective::try_from(directive)?;
                parsed_directives.push(ParsedDirective::Recurse(parsed, directive.pos));
            }
            "fold" => {
                let parsed = FoldDirective::try_from(directive)?;
                parsed_directives.push(ParsedDirective::Fold(parsed, directive.pos));
            }
            _ => {
                return Err(ParseError::UnrecognizedDirective(
                    directive.node.name.node.to_string(),
                    directive.pos,
                ))
            }
        }
    }

    Ok(parsed_directives)
}

fn make_field_node(field: &Positioned<Field>) -> Result<FieldNode, ParseError> {
    let name = &field.node.name.node;
    let alias = field.node.alias.as_ref().map(|x| &x.node);

    let fragment_spread = field
        .node
        .selection_set
        .node
        .items
        .iter()
        .find(|selection| matches!(selection.node, Selection::FragmentSpread(_)));
    if let Some(s) = fragment_spread {
        return Err(ParseError::UnsupportedSyntax(
            "fragment spread".to_string(),
            s.pos,
        ));
    }

    let inline_fragment = field
        .node
        .selection_set
        .node
        .items
        .iter()
        .find(|selection| matches!(selection.node, Selection::InlineFragment(_)));
    let (coerced_to, field_selections) = match inline_fragment {
        Some(s) => {
            if field.node.selection_set.node.items.len() > 1 {
                return Err(ParseError::TypeCoercionWithSiblingFields(
                    field.node.selection_set.node.items[1].pos,
                ));
            } else {
                match &s.node {
                    Selection::InlineFragment(f) => {
                        // TODO: handle possible @filter or @optional directives here,
                        //       no other directive is valid here

                        match f.node.type_condition.as_ref() {
                            None => {
                                // We have an inline fragment without a type condition.
                                // Per the spec, its type is considered to be equal to the type
                                // of the enclosing context:
                                // https://spec.graphql.org/October2021/#sec-Inline-Fragments
                                (None, &f.node.selection_set)
                            }
                            Some(cond) => (Some(&cond.node.on.node), &f.node.selection_set),
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        _ => (None, &field.node.selection_set),
    };

    let mut filter: SmallVec<[FilterDirective; 1]> = Default::default();
    let mut output: SmallVec<[OutputDirective; 1]> = Default::default();
    let mut tag: SmallVec<[TagDirective; 0]> = Default::default();

    let directives = make_directives(&field.node.directives)?;
    let mut directives_iter = directives.into_iter();
    let maybe_transform = loop {
        match directives_iter.next() {
            Some(ParsedDirective::Filter(f, _)) => filter.push(f),
            Some(ParsedDirective::Output(o, _)) => output.push(o),
            Some(ParsedDirective::Tag(t, _)) => tag.push(t),
            Some(ParsedDirective::Transform(t, _)) => break Some(t),
            Some(
                ParsedDirective::Optional(..)
                | ParsedDirective::Fold(..)
                | ParsedDirective::Recurse(..),
            ) => {
                // edge-specific directives, ignore them
            }
            None => break None,
        }
    };

    let transform_group = if let Some(transform) = maybe_transform {
        Some(make_transform_group(transform, &mut directives_iter)?)
    } else {
        None
    };

    let mut connections: Vec<(FieldConnection, FieldNode)> = vec![];
    for selection in field_selections.node.items.iter() {
        match &selection.node {
            Selection::FragmentSpread(_) => {
                return Err(ParseError::UnsupportedSyntax(
                    "fragment spread".to_string(),
                    selection.pos,
                ));
            }
            Selection::InlineFragment(_) => {
                return Err(ParseError::NestedTypeCoercion(selection.pos));
            }
            Selection::Field(f) => {
                let edge = make_field_connection(f)?;
                let vertex = make_field_node(f)?;
                connections.push((edge, vertex));
            }
        }
    }

    Ok(FieldNode {
        position: field.pos,
        name: name.as_ref().to_owned().into(),
        alias: alias.map(|x| x.as_ref().to_owned().into()),
        coerced_to: coerced_to.map(|x| x.as_ref().to_owned().into()),
        filter,
        transform_group,
        output,
        tag,
        connections,
    })
}

fn make_field_connection(field: &Positioned<Field>) -> Result<FieldConnection, ParseError> {
    let arguments = field.node.arguments.iter().try_fold(
        BTreeMap::new(),
        |mut acc, (name, value)| -> Result<BTreeMap<Arc<str>, FieldValue>, ParseError> {
            acc.insert_or_error(
                name.node.as_ref().to_owned().into(),
                FieldValue::try_from(&value.node).map_err(|_| {
                    ParseError::InvalidFieldArgument(
                        field.node.name.node.to_string(),
                        name.node.to_string(),
                        value.node.clone(),
                        value.pos,
                    )
                })?,
            )
            .map_err(|e| {
                ParseError::DuplicatedEdgeParameter(
                    e.entry.key().to_string(),
                    field.node.name.node.to_string(),
                    value.pos,
                )
            })?;
            Ok(acc)
        },
    )?;

    let mut optional: Option<OptionalDirective> = None;
    let mut recurse: Option<RecurseDirective> = None;

    let directives = make_directives(&field.node.directives)?;
    let mut directives_iter = directives.into_iter();
    let maybe_fold = loop {
        match directives_iter.next() {
            Some(ParsedDirective::Optional(opt, pos)) => {
                if optional.is_none() {
                    optional = Some(opt);
                } else {
                    return Err(ParseError::UnsupportedDuplicatedDirective(
                        "@optional".to_owned(),
                        pos,
                    ));
                }
            }
            Some(ParsedDirective::Recurse(rec, pos)) => {
                if recurse.is_none() {
                    recurse = Some(rec);
                } else {
                    return Err(ParseError::UnsupportedDuplicatedDirective(
                        "@recurse".to_owned(),
                        pos,
                    ));
                }
            }
            Some(ParsedDirective::Fold(fold, _)) => break Some(fold),
            Some(ParsedDirective::Transform(_, pos)) => {
                return Err(ParseError::OtherError(
                    // TODO: do better
                    "@transform applied to non-folded edge field".to_string(),
                    pos,
                ));
            }
            Some(
                ParsedDirective::Filter(..)
                | ParsedDirective::Output(..)
                | ParsedDirective::Tag(..),
            ) => {}
            None => break None,
        }
    };

    let fold_group = if let Some(fold) = maybe_fold {
        Some(make_fold_group(fold, &mut directives_iter)?)
    } else {
        None
    };

    Ok(FieldConnection {
        position: field.pos,
        name: field.node.name.node.as_ref().to_owned().into(),
        alias: field
            .node
            .alias
            .as_ref()
            .map(|p| p.node.as_ref().to_owned().into()),
        arguments,
        optional,
        recurse,
        fold: fold_group,
    })
}

fn make_fold_group(
    fold: FoldDirective,
    directive_iter: &mut impl Iterator<Item = ParsedDirective>,
) -> Result<FoldGroup, ParseError> {
    let transform_group = if let Some(directive) = directive_iter.next() {
        match directive {
            ParsedDirective::Transform(transform, _) => {
                Some(make_transform_group(transform, directive_iter)?)
            }
            ParsedDirective::Fold(_, pos) => {
                return Err(ParseError::UnsupportedDuplicatedDirective(
                    "@fold".to_string(),
                    pos,
                ));
            }
            _ => {
                return Err(ParseError::UnsupportedDirectivePosition(
                    directive.kind().to_string(),
                    "this directive cannot appear after a @fold directive".to_string(),
                    directive.pos(),
                ))
            }
        }
    } else {
        None
    };

    Ok(FoldGroup {
        fold,
        transform: transform_group,
    })
}

fn make_transform_group(
    transform: TransformDirective,
    directive_iter: &mut impl Iterator<Item = ParsedDirective>,
) -> Result<TransformGroup, ParseError> {
    let mut output = vec![];
    let mut tag = vec![];
    let mut filter = vec![];

    let retransform = loop {
        if let Some(directive) = directive_iter.next() {
            match directive {
                ParsedDirective::Filter(f, _) => filter.push(f),
                ParsedDirective::Output(o, _) => output.push(o),
                ParsedDirective::Tag(t, _) => tag.push(t),
                ParsedDirective::Transform(xform, _) => {
                    break Some(Box::new(make_transform_group(xform, directive_iter)?));
                }
                ParsedDirective::Fold(..)
                | ParsedDirective::Optional(..)
                | ParsedDirective::Recurse(..) => {
                    return Err(ParseError::UnsupportedDirectivePosition(
                        directive.kind().to_string(),
                        "this directive cannot appear after a @transform directive".to_string(),
                        directive.pos(),
                    ))
                }
            }
        } else {
            break None;
        }
    };

    // Once we encounter a @transform directive,
    // all other directives apply to the transformed value and are processed here.
    assert!(directive_iter.next().is_none());

    Ok(TransformGroup {
        transform,
        output,
        tag,
        filter,
        retransform,
    })
}

pub(crate) fn parse_document(document: &ExecutableDocument) -> Result<Query, ParseError> {
    let query_root = try_get_query_root(document)?;

    if let Some(dir) = query_root.node.directives.first() {
        return Err(ParseError::DirectiveNotInsideQueryRoot(
            dir.node.name.node.to_string(),
            dir.pos,
        ));
    }

    let root_connection = make_field_connection(query_root)?;
    assert!(root_connection.optional.is_none());
    assert!(root_connection.recurse.is_none());
    assert!(root_connection.fold.is_none());

    let root_field = make_field_node(query_root)?;

    Ok(Query {
        root_connection,
        root_field,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use async_graphql_parser::parse_query;

    use trustfall_filetests_macros::parameterize;

    use crate::util::{TestGraphQLQuery, TestParsedGraphQLQuery, TestParsedGraphQLQueryResult};

    use super::*;

    fn parameterizable_tester(base: &Path, stem: &str, check_file_suffix: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{}.graphql.ron", stem));

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{}{}", stem, check_file_suffix));

        let input_data = fs::read_to_string(input_path).unwrap();
        let test_query: TestGraphQLQuery = ron::from_str(&input_data).unwrap();

        let arguments = test_query.arguments;
        let document = parse_query(test_query.query).unwrap();
        let check_data = fs::read_to_string(check_path).unwrap();

        let constructed_test_item =
            parse_document(&document).map(move |query| TestParsedGraphQLQuery {
                schema_name: test_query.schema_name,
                query,
                arguments,
            });

        let check_parsed: TestParsedGraphQLQueryResult = ron::from_str(&check_data).unwrap();

        assert_eq!(check_parsed, constructed_test_item);
    }

    #[parameterize("trustfall_core/src/resources/test_data/parse_errors")]
    fn parse_errors(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".parse-error.ron")
    }

    #[parameterize("trustfall_core/src/resources/test_data/frontend_errors")]
    fn frontend_errors(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".graphql-parsed.ron")
    }

    #[parameterize("trustfall_core/src/resources/test_data/execution_errors")]
    fn execution_errors(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".graphql-parsed.ron")
    }

    #[parameterize("trustfall_core/src/resources/test_data/valid_queries")]
    fn valid_queries(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".graphql-parsed.ron")
    }
}
