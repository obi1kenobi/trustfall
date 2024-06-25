//! Directives in Trustfall can be identified by their prefix: `@`.
//! This module contains the logic for parsing Trustfall query directives.
use std::{collections::HashSet, num::NonZeroUsize, sync::Arc};

use async_graphql_parser::{types::Directive, Pos, Positioned};
use async_graphql_value::Value;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ir::{Operation, Tid};

use super::error::ParseError;

/// A value passed as an operator argument in a Trustfall query, for example as in
/// the `value` array of the `@filter` directive (see [FilterDirective]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatorArgument {
    /// Reference to a variable provided to the query. Variable names are always
    /// prefixed with `$`.
    VariableRef(Arc<str>),

    /// Reference to a tagged value encountered elsewhere
    /// in the query and marked with the `@tag` directive -- see [TagDirective].
    /// Tag names are always prefixed with `%`.
    TagRef(Arc<str>),
}

/// A Trustfall `@filter` directive.
///
/// The following Trustfall filter directive and Rust value would be
/// equivalent:
///
/// ```graphql
/// @filter(op: ">=", value: ["$some_value"])
/// ```
///
/// and
///
/// ```ignore
/// FilterDirective {
///     operation: Operation::GreaterThanOrEqual((), OperatorArgument::VariableRef(Arc::new("$some_value")))
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FilterDirective {
    /// Describes which operation should be made by the filter
    pub operation: Operation<(), OperatorArgument>,
}

impl TryFrom<&Positioned<Directive>> for FilterDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        let op_argument = value.node.get_argument("op").ok_or_else(|| {
            ParseError::MissingRequiredDirectiveArgument(
                "@filter".to_owned(),
                "op".to_owned(),
                value.pos,
            )
        })?;
        let op = match &op_argument.node {
            Value::String(s) => Ok(s),
            _ => Err(ParseError::InappropriateTypeForDirectiveArgument(
                "@filter".to_owned(),
                "op".to_owned(),
                op_argument.pos,
            )),
        }?;

        for (argument_name, _) in value.node.arguments.iter() {
            if !matches!(argument_name.node.as_str(), "op" | "value") {
                return Err(ParseError::UnrecognizedDirectiveArgument(
                    "@filter".to_owned(),
                    argument_name.node.to_string(),
                    argument_name.pos,
                ));
            }
        }

        let mut parsed_args: SmallVec<[OperatorArgument; 2]> = if let Some(value_argument) =
            value.node.get_argument("value")
        {
            let value_list = match &value_argument.node {
                Value::List(list) => Ok(list),
                Value::String(argument_value) => Err(ParseError::FilterExpectsListNotString(
                    op.to_owned(),
                    argument_value.to_owned(),
                    value_argument.pos,
                )),
                _ => Err(ParseError::InappropriateTypeForDirectiveArgument(
                    "@filter".to_owned(),
                    "value".to_owned(),
                    value_argument.pos,
                )),
            }?;
            value_list
                    .iter()
                    .map(|v| match v {
                        Value::String(operand) => {
                            parse_operand(operand).map_err(|e| {
                                match e {
                                    InvalidOperandError::InvalidPrefix => ParseError::InvalidFilterOperandName(
                                        operand.to_owned(),
                                        format!("Filter argument was expected to start with '$' or '%' but did not: {operand}"),
                                        value_argument.pos,
                                    ),
                                    InvalidOperandError::EmptyName => ParseError::InvalidFilterOperandName(
                                        operand.to_owned(),
                                        format!("Filter argument is empty after '{}' prefix.", operand),
                                        value_argument.pos,
                                    ),
                                    InvalidOperandError::InvalidNameStartChar(name) => ParseError::InvalidFilterOperandName(
                                        operand.to_owned(),
                                        format!("Filter argument names must start with an ASCII letter or underscore character: {name}"),
                                        value_argument.pos
                                    ),
                                    InvalidOperandError::InvalidCharsInName(name) => ParseError::InvalidFilterOperandName(
                                        operand.to_owned(),
                                        format!("Filter argument names must only contain ASCII alphanumerics or underscore characters: {name}"),
                                        value_argument.pos,
                                    ),
                                }
                            })
                        }
                        _ => Err(ParseError::InappropriateTypeForDirectiveArgument(
                            "@filter".to_owned(),
                            "value".to_owned(),
                            value_argument.pos,
                        )),
                    })
                    .collect::<Result<SmallVec<_>, _>>()?
        } else {
            SmallVec::new()
        };

        let expected_arg_count = match op.as_ref() {
            "is_null" | "is_not_null" => 0,
            _ => 1,
        };
        if parsed_args.len() != expected_arg_count {
            return Err(ParseError::OtherError(
                format!(
                    "Filter argument count mismatch: expected {} but found {}",
                    expected_arg_count,
                    parsed_args.len()
                ),
                value.node.get_argument("value").map_or(value.pos, |arg| arg.pos),
            ));
        }

        let operation = match op.as_ref() {
            "is_null" => Ok(Operation::IsNull(())),
            "is_not_null" => Ok(Operation::IsNotNull(())),
            "=" => Ok(Operation::Equals((), parsed_args.pop().unwrap())),
            "!=" => Ok(Operation::NotEquals((), parsed_args.pop().unwrap())),
            "<" => Ok(Operation::LessThan((), parsed_args.pop().unwrap())),
            "<=" => Ok(Operation::LessThanOrEqual((), parsed_args.pop().unwrap())),
            ">" => Ok(Operation::GreaterThan((), parsed_args.pop().unwrap())),
            ">=" => Ok(Operation::GreaterThanOrEqual((), parsed_args.pop().unwrap())),
            "contains" => Ok(Operation::Contains((), parsed_args.pop().unwrap())),
            "not_contains" => Ok(Operation::NotContains((), parsed_args.pop().unwrap())),
            "one_of" => Ok(Operation::OneOf((), parsed_args.pop().unwrap())),
            "not_one_of" => Ok(Operation::NotOneOf((), parsed_args.pop().unwrap())),
            "has_prefix" => Ok(Operation::HasPrefix((), parsed_args.pop().unwrap())),
            "not_has_prefix" => Ok(Operation::NotHasPrefix((), parsed_args.pop().unwrap())),
            "has_suffix" => Ok(Operation::HasSuffix((), parsed_args.pop().unwrap())),
            "not_has_suffix" => Ok(Operation::NotHasSuffix((), parsed_args.pop().unwrap())),
            "has_substring" => Ok(Operation::HasSubstring((), parsed_args.pop().unwrap())),
            "not_has_substring" => Ok(Operation::NotHasSubstring((), parsed_args.pop().unwrap())),
            "regex" => Ok(Operation::RegexMatches((), parsed_args.pop().unwrap())),
            "not_regex" => Ok(Operation::NotRegexMatches((), parsed_args.pop().unwrap())),
            unknown_op_name => Err(ParseError::UnsupportedFilterOperator(
                unknown_op_name.to_owned(),
                op_argument.pos,
            )),
        }?;
        Ok(FilterDirective { operation })
    }
}

enum InvalidOperandError<'a> {
    InvalidPrefix,
    EmptyName,
    InvalidNameStartChar(&'a str),
    InvalidCharsInName(&'a str),
}

fn parse_operand(operand: &str) -> Result<OperatorArgument, InvalidOperandError<'_>> {
    let (_, name) = if operand.starts_with('$') || operand.starts_with('%') {
        operand.split_at(1)
    } else {
        return Err(InvalidOperandError::InvalidPrefix);
    };

    if name.is_empty() {
        return Err(InvalidOperandError::EmptyName);
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return Err(InvalidOperandError::InvalidNameStartChar(name));
    }

    if name.chars().any(|c| !c.is_ascii_alphanumeric() && c != '_') {
        return Err(InvalidOperandError::InvalidCharsInName(name));
    }

    if operand.starts_with('$') {
        Ok(OperatorArgument::VariableRef(name.into()))
    } else if operand.starts_with('%') {
        Ok(OperatorArgument::TagRef(name.into()))
    } else {
        unreachable!()
    }
}

/// A Trustfall `@output` directive.
///
/// For example, the following Trustfall and Rust would be equivalent:
/// ```graphql
/// @output(name: "betterName")
/// ```
///
/// and
///
/// ```ignore
/// OutputDirective { name: Some(Arc::new("betterName"))}
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct OutputDirective {
    /// The name that should be used for this field when it is given as output
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Arc<str>>,
}

impl TryFrom<&Positioned<Directive>> for OutputDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        let mut seen_name: bool = false;
        for (arg_name, _) in &value.node.arguments {
            if arg_name.node.as_ref() == "name" {
                if !seen_name {
                    seen_name = true;
                } else {
                    return Err(ParseError::DuplicatedDirectiveArgument(
                        "@output".to_owned(),
                        arg_name.node.to_string(),
                        arg_name.pos,
                    ));
                }
            } else {
                return Err(ParseError::UnrecognizedDirectiveArgument(
                    "@output".to_owned(),
                    arg_name.node.to_string(),
                    arg_name.pos,
                ));
            }
        }

        let output_argument_node = value.node.get_argument("name");
        let parsed_output_argument = output_argument_node.map(|output| match &output.node {
            Value::String(s) => Ok(s),
            _ => Err(ParseError::InappropriateTypeForDirectiveArgument(
                "@output".to_owned(),
                "name".to_owned(),
                output.pos,
            )),
        });

        let output_argument: Option<Arc<str>> = match parsed_output_argument {
            None => None,
            Some(s) => Some(s?.to_owned().into()),
        };

        if let Some(output_name) = output_argument.as_ref() {
            ensure_name_is_valid(output_name.as_ref()).map_err(|invalid_chars| {
                ParseError::InvalidOutputName(
                    output_name.to_string(),
                    invalid_chars,
                    output_argument_node.unwrap().pos,
                )
            })?;
        }

        Ok(Self { name: output_argument })
    }
}

/// A Trustfall `@transform` directive.
///
/// For example, the following Trustfall and Rust would be equivalent:
/// ```graphql
/// @transform(op: "count")
/// ```
///
/// and
///
/// ```ignore
/// TransformDirective { kind: TransformOp::Count }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TransformDirective {
    /// The `op` in a GraphQL `@transform`, including any `value` operands that may be required.
    pub kind: TransformOp,
}

impl TryFrom<&Positioned<Directive>> for TransformDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        let mut seen_op = false;
        let mut seen_value = false;
        for (arg_name, _) in &value.node.arguments {
            let node_name = arg_name.node.as_ref();
            match node_name {
                "op" => {
                    if !seen_op {
                        seen_op = true;
                    } else {
                        return Err(ParseError::DuplicatedDirectiveArgument(
                            "@transform".to_owned(),
                            arg_name.node.to_string(),
                            arg_name.pos,
                        ));
                    }
                }
                "value" => {
                    if !seen_value {
                        seen_value = true;
                    } else {
                        return Err(ParseError::DuplicatedDirectiveArgument(
                            "@transform".to_owned(),
                            arg_name.node.to_string(),
                            arg_name.pos,
                        ));
                    }
                }
                _ => {
                    return Err(ParseError::UnrecognizedDirectiveArgument(
                        "@transform".to_owned(),
                        arg_name.node.to_string(),
                        arg_name.pos,
                    ));
                }
            }
        }

        let transform_op_node = value.node.get_argument("op").ok_or_else(|| {
            ParseError::MissingRequiredDirectiveArgument(
                "@transform".to_owned(),
                "op".to_owned(),
                value.pos,
            )
        })?;
        let transform_op: Arc<str> = match &transform_op_node.node {
            Value::String(s) => s.to_owned().into(),
            _ => {
                return Err(ParseError::InappropriateTypeForDirectiveArgument(
                    "@transform".to_owned(),
                    "op".to_owned(),
                    transform_op_node.pos,
                ))
            }
        };

        let mut transform_value: SmallVec<[OperatorArgument; 2]> = match value
            .node
            .get_argument("value")
        {
            None => SmallVec::new(),
            Some(content) => match &content.node {
                Value::List(value_contents) => {
                    let mut values = SmallVec::new();
                    for v in value_contents {
                        match v {
                            Value::String(operand) => {
                                let operator_argument = parse_operand(operand).map_err(|e| {
                                        match e {
                                            InvalidOperandError::InvalidPrefix => ParseError::InvalidTransformOperandName(
                                                operand.to_owned(),
                                                format!("Transform argument was expected to start with '$' or '%' but did not: {operand}"),
                                                content.pos,
                                            ),
                                            InvalidOperandError::EmptyName => ParseError::InvalidTransformOperandName(
                                                operand.to_owned(),
                                                format!("Transform argument is empty after '{}' prefix.", operand),
                                                content.pos,
                                            ),
                                            InvalidOperandError::InvalidNameStartChar(name) => ParseError::InvalidTransformOperandName(
                                                operand.to_owned(),
                                                format!("Transform argument names must start with an ASCII letter or underscore character: {name}"),
                                                content.pos
                                            ),
                                            InvalidOperandError::InvalidCharsInName(name) => ParseError::InvalidTransformOperandName(
                                                operand.to_owned(),
                                                format!("Transform argument names must only contain ASCII alphanumerics or underscore characters: {name}"),
                                                content.pos,
                                            ),
                                        }
                                    })?;
                                values.push(operator_argument);
                            }
                            _ => {
                                return Err(ParseError::InappropriateTypeForDirectiveArgument(
                                    "@transform".to_owned(),
                                    "value".to_owned(),
                                    content.pos,
                                ))
                            }
                        }
                    }
                    values
                }
                Value::String(argument_value) => {
                    return Err(ParseError::TransformExpectsListNotString(
                        transform_op.to_string(),
                        argument_value.to_owned(),
                        content.pos,
                    ))
                }
                _ => {
                    return Err(ParseError::InappropriateTypeForDirectiveArgument(
                        "@transform".to_owned(),
                        "value".to_owned(),
                        content.pos,
                    ))
                }
            },
        };

        let operands_span = value.node.get_argument("value").map(|p| &p.pos).unwrap_or(&value.pos);
        let kind = match transform_op.as_ref() {
            "count" => {
                assert_operand_count(&transform_value, 0, operands_span)?;
                TransformOp::Count
            }
            "len" => {
                assert_operand_count(&transform_value, 0, operands_span)?;
                TransformOp::Len
            }
            "abs" => {
                assert_operand_count(&transform_value, 0, operands_span)?;
                TransformOp::Abs
            }
            "+" => {
                assert_operand_count(&transform_value, 1, operands_span)?;
                TransformOp::Add(transform_value.pop().unwrap())
            }
            "+f" => {
                assert_operand_count(&transform_value, 1, operands_span)?;
                TransformOp::AddF(transform_value.pop().unwrap())
            }
            _ => {
                return Err(ParseError::UnsupportedTransformOperator(
                    transform_op.to_string(),
                    transform_op_node.pos,
                ))
            }
        };

        Ok(Self { kind })
    }
}

fn assert_operand_count(
    operands: &[OperatorArgument],
    expected_count: usize,
    pos: &Pos,
) -> Result<(), ParseError> {
    let provided = operands.len();
    if provided != expected_count {
        Err(unexpected_arguments_provided(expected_count, provided, pos))
    } else {
        Ok(())
    }
}

fn unexpected_arguments_provided(
    expected_count: usize,
    actual_count: usize,
    pos: &Pos,
) -> ParseError {
    ParseError::OtherError(
        format!(
            "Transform argument count mismatch: expected {expected_count} but found {actual_count}"
        ),
        pos.to_owned(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum TransformOp {
    Count,
    Len,
    Abs,
    Add(OperatorArgument),
    AddF(OperatorArgument),
}

impl TransformOp {
    /// The exact operation name we parse from `@transform` directives.
    ///
    /// For example: `@transform(op: "+", value: ["$value"])` corresponds to [`TransformOp::Add`],
    /// so `TransformOp::Add.op_name() == "+"`.
    pub(crate) fn op_name(&self) -> &'static str {
        match self {
            TransformOp::Count => "count",
            TransformOp::Len => "len",
            TransformOp::Abs => "abs",
            TransformOp::Add(_) => "+",
            TransformOp::AddF(_) => "+f",
        }
    }
}

/// A Trustfall `@tag` directive.
///
/// For example, the following Trustfall and Rust would be equivalent:
/// ```graphql
/// @tag(name: "%tag_name")
/// ```
///
/// and
///
/// ```ignore
/// TagDirective { name: Some(Arc::new("%tag_name"))}
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TagDirective {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Arc<str>>,
}

impl TryFrom<&Positioned<Directive>> for TagDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        let mut seen_name: bool = false;
        for (arg_name, _) in &value.node.arguments {
            if arg_name.node.as_ref() == "name" {
                if !seen_name {
                    seen_name = true;
                } else {
                    return Err(ParseError::DuplicatedDirectiveArgument(
                        "@tag".to_owned(),
                        arg_name.node.to_string(),
                        arg_name.pos,
                    ));
                }
            } else {
                return Err(ParseError::UnrecognizedDirectiveArgument(
                    "@tag".to_owned(),
                    arg_name.node.to_string(),
                    arg_name.pos,
                ));
            }
        }

        let tag_argument_node = value.node.get_argument("name");
        let parsed_tag_argument = tag_argument_node.map(|tag| match &tag.node {
            Value::String(s) => Ok(s),
            _ => Err(ParseError::InappropriateTypeForDirectiveArgument(
                "@tag".to_owned(),
                "name".to_owned(),
                tag.pos,
            )),
        });

        let tag_argument: Option<Arc<str>> = match parsed_tag_argument {
            None => None,
            Some(s) => Some(s?.to_owned().into()),
        };

        if let Some(tag_name) = tag_argument.as_ref() {
            ensure_name_is_valid(tag_name.as_ref()).map_err(|invalid_chars| {
                ParseError::InvalidTagName(
                    tag_name.to_string(),
                    invalid_chars,
                    tag_argument_node.unwrap().pos,
                )
            })?;
        }

        Ok(Self { name: tag_argument })
    }
}

/// A Trustfall `@optional` directive.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct OptionalDirective {}

impl TryFrom<&Positioned<Directive>> for OptionalDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        if let Some((first_arg_name, _)) = value.node.arguments.first() {
            // Found arguments but this directive doesn't take any.
            return Err(ParseError::UnrecognizedDirectiveArgument(
                "@optional".into(),
                first_arg_name.node.to_string(),
                first_arg_name.pos,
            ));
        }

        Ok(Self {})
    }
}

/// A Trustfall `@fold` directive.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FoldDirective {}

impl TryFrom<&Positioned<Directive>> for FoldDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        if let Some((first_arg_name, _)) = value.node.arguments.first() {
            // Found arguments but this directive doesn't take any.
            return Err(ParseError::UnrecognizedDirectiveArgument(
                "@fold".into(),
                first_arg_name.node.to_string(),
                first_arg_name.pos,
            ));
        }

        Ok(Self {})
    }
}

/// A Trustfall `@recurse` directive.
///
/// For example, the following Trustfall and Rust would be equivalent:
/// ```graphql
/// @recurse(depth: 1)
/// ```
///
/// and
///
/// ```ignore
/// RecurseDirective { depth: NonZeroUsize::new(1usize)}
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct RecurseDirective {
    pub depth: NonZeroUsize,
}

impl TryFrom<&Positioned<Directive>> for RecurseDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        let mut seen_name: bool = false;
        for (arg_name, _) in &value.node.arguments {
            if arg_name.node.as_ref() == "depth" {
                if !seen_name {
                    seen_name = true;
                } else {
                    return Err(ParseError::DuplicatedDirectiveArgument(
                        "@recurse".to_owned(),
                        arg_name.node.to_string(),
                        arg_name.pos,
                    ));
                }
            } else {
                return Err(ParseError::UnrecognizedDirectiveArgument(
                    "@recurse".to_owned(),
                    arg_name.node.to_string(),
                    arg_name.pos,
                ));
            }
        }

        let depth_argument = value.node.get_argument("depth").ok_or_else(|| {
            ParseError::MissingRequiredDirectiveArgument(
                "@recurse".to_owned(),
                "depth".to_owned(),
                value.pos,
            )
        })?;
        let depth = match &depth_argument.node {
            Value::Number(n) => {
                n.as_u64().and_then(|v| NonZeroUsize::new(v as usize)).ok_or_else(|| {
                    ParseError::InappropriateTypeForDirectiveArgument(
                        "@recurse".to_owned(),
                        "depth".to_owned(),
                        depth_argument.pos,
                    )
                })
            }
            _ => Err(ParseError::InappropriateTypeForDirectiveArgument(
                "@recurse".to_owned(),
                "depth".to_owned(),
                depth_argument.pos,
            )),
        }?;

        Ok(Self { depth })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TransformGroup {
    pub tid: Tid,

    pub transform: TransformDirective,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<OutputDirective>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tag: Vec<TagDirective>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<FilterDirective>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retransform: Option<Box<TransformGroup>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FoldGroup {
    pub fold: FoldDirective,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<TransformGroup>,
}

fn ensure_name_is_valid(name: &str) -> Result<(), Vec<char>> {
    let mut invalid_char_iter =
        name.chars().filter(|c| !c.is_ascii_alphanumeric() && *c != '_').peekable();
    if invalid_char_iter.peek().is_some() {
        let mut seen_chars: HashSet<char> = Default::default();
        let mut invalid_chars: Vec<_> = Default::default();
        for c in invalid_char_iter {
            if seen_chars.insert(c) {
                invalid_chars.push(c);
            }
        }
        return Err(invalid_chars);
    }

    Ok(())
}
