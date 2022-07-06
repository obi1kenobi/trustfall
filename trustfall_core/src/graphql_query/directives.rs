use std::{collections::HashSet, convert::TryFrom, num::NonZeroUsize, sync::Arc};

use async_graphql_parser::{types::Directive, Positioned};
use async_graphql_value::Value;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ir::{Operation, TransformationKind};

use super::error::ParseError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatorArgument {
    VariableRef(Arc<str>),
    TagRef(Arc<str>),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FilterDirective {
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

        let mut parsed_args: SmallVec<[OperatorArgument; 2]> = if let Some(value_argument) =
            value.node.get_argument("value")
        {
            let value_list = match &value_argument.node {
                Value::List(list) => Ok(list),
                _ => Err(ParseError::InappropriateTypeForDirectiveArgument(
                    "@filter".to_owned(),
                    "value".to_owned(),
                    value_argument.pos,
                )),
            }?;
            value_list
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => {
                            let name = if s.starts_with('$') || s.starts_with('%') {
                                s.split_at(1).1
                            } else {
                                return Err(ParseError::OtherError(
                                    format!("Filter argument was expected to start with '$' or '%' but did not: {}", s),
                                    value_argument.pos,
                                ));
                            };

                            // Empty names handled above already.
                            assert!(!name.is_empty());

                            if name.chars().next().unwrap().is_numeric()
                                || name.chars().any(|c| !c.is_ascii_alphanumeric() && c != '_')
                            {
                                return Err(ParseError::OtherError(
                                    format!("Filter argument names must start with an ASCII letter or underscore character: {}", name),
                                    value_argument.pos,
                                ));
                            }

                            if s.starts_with('$') {
                                Ok(OperatorArgument::VariableRef(name.into()))
                            } else if s.starts_with('%') {
                                Ok(OperatorArgument::TagRef(name.into()))
                            } else {
                                unreachable!()
                            }
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
                value
                    .node
                    .get_argument("value")
                    .map_or(value.pos, |arg| arg.pos),
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
            ">=" => Ok(Operation::GreaterThanOrEqual(
                (),
                parsed_args.pop().unwrap(),
            )),
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct OutputDirective {
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

        Ok(Self {
            name: output_argument,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct TransformDirective {
    pub kind: TransformationKind,
}

impl TryFrom<&Positioned<Directive>> for TransformDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        let mut seen_op: bool = false;
        for (arg_name, _) in &value.node.arguments {
            if arg_name.node.as_ref() == "op" {
                if !seen_op {
                    seen_op = true;
                } else {
                    return Err(ParseError::DuplicatedDirectiveArgument(
                        "@transform".to_owned(),
                        arg_name.node.to_string(),
                        arg_name.pos,
                    ));
                }
            } else {
                return Err(ParseError::UnrecognizedDirectiveArgument(
                    "@transform".to_owned(),
                    arg_name.node.to_string(),
                    arg_name.pos,
                ));
            }
        }

        let transform_argument_node = value.node.get_argument("op").ok_or_else(|| {
            ParseError::MissingRequiredDirectiveArgument(
                "@transform".to_owned(),
                "op".to_owned(),
                value.pos,
            )
        })?;

        let transform_argument: Arc<str> = match &transform_argument_node.node {
            Value::String(s) => s.to_owned().into(),
            _ => {
                return Err(ParseError::InappropriateTypeForDirectiveArgument(
                    "@transform".to_owned(),
                    "op".to_owned(),
                    transform_argument_node.pos,
                ))
            }
        };

        let kind = match transform_argument.as_ref() {
            "count" => TransformationKind::Count,
            _ => {
                return Err(ParseError::UnsupportedTransformOperator(
                    transform_argument.to_string(),
                    transform_argument_node.pos,
                ))
            }
        };

        Ok(Self { kind })
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct OptionalDirective {}

impl TryFrom<&Positioned<Directive>> for OptionalDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        if let Some((first_arg_name, _)) = value.node.arguments.get(0) {
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct FoldDirective {}

impl TryFrom<&Positioned<Directive>> for FoldDirective {
    type Error = ParseError;

    fn try_from(value: &Positioned<Directive>) -> Result<Self, Self::Error> {
        if let Some((first_arg_name, _)) = value.node.arguments.get(0) {
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
            Value::Number(n) => n
                .as_u64()
                .and_then(|v| NonZeroUsize::new(v as usize))
                .ok_or_else(|| {
                    ParseError::InappropriateTypeForDirectiveArgument(
                        "@recurse".to_owned(),
                        "depth".to_owned(),
                        depth_argument.pos,
                    )
                }),
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
    let mut invalid_char_iter = name
        .chars()
        .filter(|c| !c.is_ascii_alphanumeric() && *c != '_')
        .peekable();
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
