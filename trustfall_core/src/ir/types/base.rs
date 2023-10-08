use std::{fmt::Display, fmt::Formatter, sync::Arc};

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use crate::ir::FieldValue;

/// A representation of a Trustfall type, independent of which parser or query syntax we're using.
/// Equivalent in expressiveness to GraphQL types, but not explicitly tied to a GraphQL library.
#[derive(Clone, PartialEq, Eq)]
pub struct Type {
    base: Arc<str>,
    modifiers: Modifiers,
}

impl std::fmt::Debug for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!("Type (represents {self})"))
            .field("base", &self.base)
            .field("modifiers", &self.modifiers)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Modifiers {
    mask: u64, // space for ~30 levels of list nesting
}

impl Modifiers {
    const NON_NULLABLE_MASK: u64 = 1;
    const LIST_MASK: u64 = 2;
    const MAX_LIST_DEPTH: u64 = 30;

    /// Represents the leftmost list bit that can be set before adding a new list will overflow.
    /// `(Self::MAX_LIST_DEPTH - 1)` because we start shifted over once.
    const MAX_LIST_DEPTH_MASK: u64 = Self::LIST_MASK << ((Self::MAX_LIST_DEPTH - 1) * 2);

    #[inline]
    fn nullable(&self) -> bool {
        (self.mask & Self::NON_NULLABLE_MASK) == 0
    }

    #[inline]
    fn is_list(&self) -> bool {
        (self.mask & Self::LIST_MASK) != 0
    }

    #[inline]
    fn as_list(&self) -> Option<Modifiers> {
        self.is_list().then_some(Modifiers { mask: self.mask >> 2 })
    }

    #[inline]
    fn at_max_list_depth(&self) -> bool {
        (self.mask & Self::MAX_LIST_DEPTH_MASK) == Self::MAX_LIST_DEPTH_MASK
    }
}

#[derive(Debug, Clone)]
pub struct TypeParseError {
    invalid_type: String,
}

impl Display for TypeParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not a valid Type representation", &self.invalid_type)
    }
}

impl std::error::Error for TypeParseError {}

impl Type {
    /// Parses a string type representation into a new [`Type`].
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let ty = Type::parse("[String!]!").unwrap();
    /// assert_eq!(ty.to_string(), "[String!]!");
    ///
    /// assert_eq!(Type::parse("[String!]").unwrap().to_string(), "[String!]");
    /// ```
    pub fn parse(ty: &str) -> Result<Self, TypeParseError> {
        async_graphql_parser::types::Type::new(ty)
            .ok_or_else(|| TypeParseError { invalid_type: ty.to_string() })
            .map(|ty| Self::from_type(&ty))
    }

    /// Creates an individual [`Type`], not a list.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let nullable = false;
    /// let ty = Type::new_named_type("String", nullable);
    ///
    /// assert_eq!(ty.to_string(), "String!");
    /// assert_eq!(ty, Type::parse("String!").unwrap());
    /// ```
    pub fn new_named_type(base_type: &str, nullable: bool) -> Self {
        Self {
            base: base_type.to_string().into(),
            modifiers: Modifiers { mask: if nullable { 0 } else { Modifiers::NON_NULLABLE_MASK } },
        }
    }

    /// Creates a new list layer on a [`Type`].
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let inner_nullable = false;
    /// let inner_ty = Type::new_named_type("String", inner_nullable);
    ///
    /// let outer_nullable = true;
    /// let ty = Type::new_list_type(inner_ty, outer_nullable);
    ///
    /// assert_eq!(ty.to_string(), "[String!]");
    /// assert_eq!(ty, Type::parse("[String!]").unwrap());
    /// ```
    pub fn new_list_type(inner_type: Self, nullable: bool) -> Self {
        if inner_type.modifiers.at_max_list_depth() {
            panic!("too many nested lists");
        }

        let mut new_mask = (inner_type.modifiers.mask << 2) | Modifiers::LIST_MASK;

        if !nullable {
            new_mask |= Modifiers::NON_NULLABLE_MASK;
        }

        Self { base: inner_type.base, modifiers: Modifiers { mask: new_mask } }
    }

    /// Returns a new type that is the same as this one, but with the passed nullability.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let nullable_ty = Type::parse("Int").unwrap();
    /// assert_eq!(nullable_ty.nullable(), true);
    /// let non_nullable_ty = nullable_ty.with_nullability(false);
    /// assert_eq!(non_nullable_ty.nullable(), false);
    ///
    /// // The original type is unchanged.
    /// assert_eq!(nullable_ty.nullable(), true);
    /// ```
    pub fn with_nullability(&self, nullable: bool) -> Self {
        let mut new = self.clone();
        if nullable {
            new.modifiers.mask &= !Modifiers::NON_NULLABLE_MASK;
        } else {
            new.modifiers.mask |= Modifiers::NON_NULLABLE_MASK;
        }
        new
    }

    /// Returns whether this type is nullable, at the top level, see example.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let nullable_ty = Type::parse("[Int!]").unwrap();
    /// assert_eq!(nullable_ty.nullable(), true); // the list is nullable
    ///
    /// let nullable_ty = Type::parse("Int!").unwrap();
    /// assert_eq!(nullable_ty.nullable(), false); // the `Int` is nonnullable
    /// ```
    pub fn nullable(&self) -> bool {
        self.modifiers.nullable()
    }

    /// Returns whether the type is a list or not.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let non_null_int_arr = Type::parse("[Int!]").unwrap();
    /// assert_eq!(non_null_int_arr.is_list(), true);
    ///
    /// let non_null_int = Type::parse("Int!").unwrap();
    /// assert_eq!(non_null_int.is_list(), false);
    /// ```
    pub fn is_list(&self) -> bool {
        self.modifiers.is_list()
    }

    /// Returns the type inside the outermost list of this type if it is a list, otherwise returns `None`.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let non_null_int_arr = Type::parse("[Int!]").unwrap();
    /// let non_null_int = Type::parse("Int!").unwrap();
    /// assert_eq!(non_null_int_arr.as_list(), Some(non_null_int.clone()));
    /// assert_eq!(non_null_int.as_list(), None);
    /// ```
    pub fn as_list(&self) -> Option<Self> {
        Some(Self { base: Arc::clone(&self.base), modifiers: self.modifiers.as_list()? })
    }

    /// Returns the type of the elements of the first individual type found inside this type.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::Type;
    ///
    /// let int_list_ty = Type::parse("[Int!]").unwrap();
    /// assert_eq!(int_list_ty.base_type(), "Int");
    ///
    /// let string_ty = Type::parse("String!").unwrap();
    /// assert_eq!(string_ty.base_type(), "String");
    pub fn base_type(&self) -> &str {
        &self.base
    }

    /// Convert a [`async_graphql_parser::types::Type`] to a [`Type`].
    pub(crate) fn from_type(ty: &async_graphql_parser::types::Type) -> Type {
        let mut base = &ty.base;

        let mut mask = if ty.nullable { 0 } else { Modifiers::NON_NULLABLE_MASK };

        let mut i = 0;

        while let async_graphql_parser::types::BaseType::List(ty_inside_list) = base {
            mask |= Modifiers::LIST_MASK << i;
            i += 2;
            if i > Modifiers::MAX_LIST_DEPTH * 2 {
                panic!("too many nested lists");
            }
            if !ty_inside_list.nullable {
                mask |= Modifiers::NON_NULLABLE_MASK << i;
            }
            base = &ty_inside_list.base;
        }

        let async_graphql_parser::types::BaseType::Named(name) = base else {
            unreachable!(
                "should be impossible to get a non-named type after looping through all list types"
            )
        };
        Type { base: name.to_string().into(), modifiers: Modifiers { mask } }
    }

    /// For two types, return a type that is a subtype of both, or None if no such type exists.
    /// For example:
    /// ```rust
    /// use trustfall_core::ir::types::Type;
    ///
    /// let left = Type::parse("[String]!").unwrap();
    /// let right = Type::parse("[String!]").unwrap();
    /// let result = left.intersect(&right);
    /// assert_eq!(Some(Type::parse("[String!]!").unwrap()), result);
    ///
    /// let incompatible = Type::parse("[Int]").unwrap();
    /// let result = left.intersect(&incompatible);
    /// assert_eq!(None, result);
    /// ```
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        if self.base_type() != other.base_type() {
            return None;
        }

        self.intersect_impl(other)
    }

    fn intersect_impl(&self, other: &Self) -> Option<Self> {
        let nullable = self.nullable() && other.nullable();

        match (self.as_list(), other.as_list()) {
            (None, None) => Some(Type::new_named_type(self.base_type(), nullable)),
            (Some(left), Some(right)) => {
                left.intersect_impl(&right).map(|inner| Type::new_list_type(inner, nullable))
            }
            _ => None,
        }
    }

    pub(crate) fn equal_ignoring_nullability(&self, other: &Self) -> bool {
        if self.base_type() != other.base_type() {
            return false;
        }

        match (self.as_list(), other.as_list()) {
            (None, None) => true,
            (Some(left), Some(right)) => left.equal_ignoring_nullability(&right),
            _ => false,
        }
    }

    /// Check if the given value is allowed by the specified type.
    ///
    /// In particular, mixed integer types in a list are considered valid for types like `[Int]`.
    /// ```rust
    /// use trustfall_core::ir::{FieldValue, Type};
    ///
    /// let ty = Type::parse("[Int]").unwrap();
    /// let value = FieldValue::List([
    ///     FieldValue::Int64(-1),
    ///     FieldValue::Uint64(1),
    ///     FieldValue::Null,
    /// ].as_slice().into());
    /// assert!(ty.is_valid_value(&value));
    /// ```
    pub fn is_valid_value(&self, value: &FieldValue) -> bool {
        match value {
            FieldValue::Null => {
                // This is a valid value only if this layer is nullable.
                self.nullable()
            }
            FieldValue::Int64(_) | FieldValue::Uint64(_) => {
                // This is a valid value only if the type is Int, ignoring nullability.
                !self.is_list() && self.base_type() == "Int"
            }
            FieldValue::Float64(_) => {
                // This is a valid value only if the type is Float, ignoring nullability.
                !self.is_list() && self.base_type() == "Float"
            }
            FieldValue::String(_) => {
                // This is a valid value only if the type is String, ignoring nullability.
                !self.is_list() && self.base_type() == "String"
            }
            FieldValue::Boolean(_) => {
                // This is a valid value only if the type is Boolean, ignoring nullability.
                !self.is_list() && self.base_type() == "Boolean"
            }
            FieldValue::List(contents) => {
                // This is a valid value only if the type is a list, and all the inner elements
                // are valid instances of the type inside the list.
                if let Some(content_type) = self.as_list() {
                    contents.iter().all(|inner| content_type.is_valid_value(inner))
                } else {
                    false
                }
            }
            FieldValue::Enum(_) => {
                unimplemented!("enum values are not currently supported: {self} {value:?}")
            }
        }
    }

    /// Returns `true` if values of this type can be compared using operators like `<`.
    ///
    /// In Rust terms, this checks for `PartialOrd` on this `Type`.
    ///
    /// Lists (including nested lists) are orderable if the type they contain is orderable.
    /// Lists use lexicographic ordering, i.e. `[1, 2, 3] < [3]`.
    pub(crate) fn is_orderable(&self) -> bool {
        matches!(self.base_type(), "Int" | "Float" | "String")
    }

    /// Check for scalar-only subtyping.
    ///
    /// Scalars don't have an inheritance structure, so they are able to be compared without a schema.
    /// Callers of this function must guarantee that the passed types are either scalars or
    /// (potentially multiply-nested) lists of scalars.
    ///
    /// This function considers types of different names to always be non-equal and unrelated:
    /// neither is a subtype of the other. So given `interface Base` and `type Derived implements Base`,
    /// that means `is_scalar_only_subtype(Base, Derived) == false`, since this function never sees
    /// the definitions of `Base` and `Derived` as those are part of a schema which this function
    /// never gets.
    pub(crate) fn is_scalar_only_subtype(&self, maybe_subtype: &Self) -> bool {
        // If the parent type is non-nullable, all its subtypes must be non-nullable as well.
        // If the parent type is nullable, it can have both nullable and non-nullable subtypes.
        if !self.nullable() && maybe_subtype.nullable() {
            return false;
        }

        // If the base types don't match, there can't be a subtyping relationship here.
        // Recall that callers are required to make sure only scalar / nested-lists-of-scalar types
        // are passed into this function.
        if self.base_type() != maybe_subtype.base_type() {
            return false;
        }

        match (self.as_list(), maybe_subtype.as_list()) {
            (None, None) => true,
            (Some(parent), Some(maybe_subtype)) => parent.is_scalar_only_subtype(&maybe_subtype),
            _ => false,
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // left
        {
            let mut current = Some(self.modifiers.clone());
            while let Some(mods) = current {
                if mods.is_list() {
                    write!(f, "[")?;
                }
                current = mods.as_list();
            }
        }

        write!(f, "{}", self.base)?;

        let mut builder = String::new();

        // right
        {
            let mut current = Some(self.modifiers.clone());
            while let Some(mods) = current {
                if !mods.nullable() {
                    builder.push('!');
                }
                if mods.is_list() {
                    builder.push(']');
                }
                current = mods.as_list();
            }
            write!(f, "{}", builder.chars().rev().collect::<String>())?;
        }

        Ok(())
    }
}

impl Serialize for Type {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Type {
    fn deserialize<D>(deserializer: D) -> Result<Type, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TypeDeserializer;

        impl<'de> Visitor<'de> for TypeDeserializer {
            type Value = Type;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("GraphQL type")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Type::parse(s).map_err(|err| serde::de::Error::custom(err))
            }
        }

        deserializer.deserialize_str(TypeDeserializer)
    }
}

#[cfg(test)]
mod test {
    use itertools::Itertools;

    use crate::ir::{FieldValue, Type};

    use super::Modifiers;

    #[test]
    fn max_allowed_nested_lists_mask_representation() {
        let type_str = format!(
            "{}String{}",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize),
            "]".repeat(Modifiers::MAX_LIST_DEPTH as usize)
        );
        let type_modifiers = Type::parse(&type_str).unwrap().modifiers;
        assert_eq!(
            format!("{:b}", type_modifiers.mask),
            "101010101010101010101010101010101010101010101010101010101010"
        );
    }

    #[test]
    fn max_allowed_nested_lists_is_at_max_list_depth() {
        let type_str = format!(
            "{}String{}",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize),
            "]".repeat(Modifiers::MAX_LIST_DEPTH as usize)
        );
        let type_modifiers = Type::parse(&type_str).unwrap().modifiers;
        assert!(type_modifiers.at_max_list_depth());
    }

    #[test]
    #[should_panic(expected = "too many nested lists")]
    fn too_many_nested_lists_via_type_new() {
        let type_str = format!(
            "{}String!{}",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize + 1),
            "]".repeat(Modifiers::MAX_LIST_DEPTH as usize + 1)
        );
        let _ = Type::parse(&type_str); // will panic during modifier mask creation
    }

    #[test]
    #[should_panic(expected = "too many nested lists")]
    fn too_many_nested_lists_via_new_list_type() {
        let mut constructed_type = Type::new_named_type("String", false);
        for _ in 0..=Modifiers::MAX_LIST_DEPTH {
            constructed_type = Type::new_list_type(constructed_type, false);
        }

        // will panic during new modifier mask creation for new list type
        Type::new_list_type(constructed_type, false);
    }

    #[test]
    fn max_allowed_nested_lists_with_nonnull_on_last_list_mask_representation() {
        let type_str = format!(
            "{}String{}!",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize),
            "]".repeat(Modifiers::MAX_LIST_DEPTH as usize)
        );
        let type_modifiers = Type::parse(&type_str).unwrap().modifiers;
        assert_eq!(
            format!("{:b}", type_modifiers.mask),
            "101010101010101010101010101010101010101010101010101010101011"
        );
    }

    #[test]
    fn max_allowed_nested_lists_with_nonnull_on_innermost_type_mask_representation() {
        let type_str = format!(
            "{}String!{}",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize),
            "]".repeat(Modifiers::MAX_LIST_DEPTH as usize)
        );
        let type_modifiers = Type::parse(&type_str).unwrap().modifiers;
        assert_eq!(
            format!("{:b}", type_modifiers.mask),
            "1101010101010101010101010101010101010101010101010101010101010"
        );
    }

    #[test]
    fn max_allowed_nested_lists_with_non_null_on_every_list_and_inner_type() {
        let type_str = format!(
            "{}String!{}",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize),
            "]!".repeat(Modifiers::MAX_LIST_DEPTH as usize)
        );
        let type_modifiers = Type::parse(&type_str).unwrap().modifiers;
        assert!(type_modifiers.at_max_list_depth());
    }

    #[test]
    fn base_types_equal_ignoring_nullability() {
        let test_data = [
            (Type::parse("String"), Type::parse("String"), true),
            (Type::parse("String!"), Type::parse("String!"), true),
            (Type::parse("Int"), Type::parse("Int!"), true),
            (Type::parse("[String!]"), Type::parse("[String]!"), true),
            (Type::parse("[String]"), Type::parse("[String!]!"), true),
            (Type::parse("String"), Type::parse("Int"), false),
            (Type::parse("String!"), Type::parse("Int!"), false),
            (Type::parse("[String]"), Type::parse("String"), false),
            (Type::parse("[String]!"), Type::parse("String!"), false),
            (Type::parse("[String!]"), Type::parse("String!"), false),
        ];

        for (left, right, expected) in test_data {
            let left = left.expect("not a valid type");
            let right = right.expect("not a valid type");
            assert_eq!(left.equal_ignoring_nullability(&right), expected, "{left} {right}");
            assert_eq!(
                right.equal_ignoring_nullability(&left),
                expected,
                "commutativity violation in: {right} {left}"
            );
        }
    }

    #[test]
    fn null_values_are_only_valid_for_nullable_types() {
        let nullable_types = [
            Type::parse("Int").unwrap(),
            Type::parse("String").unwrap(),
            Type::parse("Boolean").unwrap(),
            Type::parse("[Int!]").unwrap(),
            Type::parse("[[Int!]!]").unwrap(),
        ];
        let non_nullable_types =
            nullable_types.iter().map(|t| t.with_nullability(false)).collect_vec();

        for nullable_type in &nullable_types {
            assert!(nullable_type.is_valid_value(&FieldValue::Null), "{}", nullable_type);
        }
        for non_nullable_type in &non_nullable_types {
            assert!(!non_nullable_type.is_valid_value(&FieldValue::Null), "{}", non_nullable_type);
        }
    }

    #[test]
    fn int_values_are_valid_only_for_int_type_regardless_of_nullability() {
        let matching_types = [Type::parse("Int").unwrap(), Type::parse("Int!").unwrap()];
        let non_matching_types = [
            Type::parse("String").unwrap(),
            Type::parse("[Int!]").unwrap(),
            Type::parse("[Int!]!").unwrap(),
            Type::parse("[[Int!]!]").unwrap(),
        ];
        let values = [
            FieldValue::Int64(-42),
            FieldValue::Int64(0),
            FieldValue::Uint64(0),
            FieldValue::Uint64((i64::MAX as u64) + 1),
        ];

        for value in &values {
            for matching_type in &matching_types {
                assert!(matching_type.is_valid_value(value), "{matching_type} {value:?}",);
            }
            for non_matching_type in &non_matching_types {
                assert!(!non_matching_type.is_valid_value(value), "{non_matching_type} {value:?}",);
            }
        }
    }

    #[test]
    fn string_values_are_valid_only_for_string_type_regardless_of_nullability() {
        let matching_types = [Type::parse("String").unwrap(), Type::parse("String!").unwrap()];
        let non_matching_types = [
            Type::parse("Int").unwrap(),
            Type::parse("[String!]").unwrap(),
            Type::parse("[String!]!").unwrap(),
            Type::parse("[[String!]!]").unwrap(),
        ];
        let values = [
            FieldValue::String("".into()), // empty string is not the same value as null
            FieldValue::String("test string".into()),
        ];

        for value in &values {
            for matching_type in &matching_types {
                assert!(matching_type.is_valid_value(value), "{matching_type} {value:?}",);
            }
            for non_matching_type in &non_matching_types {
                assert!(!non_matching_type.is_valid_value(value), "{non_matching_type} {value:?}",);
            }
        }
    }

    #[test]
    fn boolean_values_are_valid_only_for_boolean_type_regardless_of_nullability() {
        let matching_types = [Type::parse("Boolean").unwrap(), Type::parse("Boolean!").unwrap()];
        let non_matching_types = [
            Type::parse("Int").unwrap(),
            Type::parse("[Boolean!]").unwrap(),
            Type::parse("[Boolean!]!").unwrap(),
            Type::parse("[[Boolean!]!]").unwrap(),
        ];
        let values = [FieldValue::Boolean(false), FieldValue::Boolean(true)];

        for value in &values {
            for matching_type in &matching_types {
                assert!(matching_type.is_valid_value(value), "{matching_type} {value:?}",);
            }
            for non_matching_type in &non_matching_types {
                assert!(!non_matching_type.is_valid_value(value), "{non_matching_type} {value:?}",);
            }
        }
    }

    #[test]
    fn list_types_correctly_check_contents_of_list() {
        let non_nullable_contents_matching_types =
            vec![Type::parse("[Int!]").unwrap(), Type::parse("[Int!]!").unwrap()];
        let nullable_contents_matching_types =
            vec![Type::parse("[Int]").unwrap(), Type::parse("[Int]!").unwrap()];
        let non_matching_types = [
            Type::parse("Int").unwrap(),
            Type::parse("Int!").unwrap(),
            Type::parse("[String!]").unwrap(),
            Type::parse("[String!]!").unwrap(),
            Type::parse("[[String!]!]").unwrap(),
        ];
        let non_nullable_values = [
            FieldValue::List((1..3).map(FieldValue::Int64).collect_vec().into()),
            FieldValue::List((1..3).map(FieldValue::Uint64).collect_vec().into()),
            FieldValue::List(
                vec![
                    // Integer-typed but non-homogeneous FieldValue entries are okay.
                    FieldValue::Int64(-42),
                    FieldValue::Uint64(64),
                ]
                .into(),
            ),
        ];
        let nullable_values = [
            FieldValue::List(
                vec![FieldValue::Int64(1), FieldValue::Null, FieldValue::Int64(2)].into(),
            ),
            FieldValue::List(vec![FieldValue::Null, FieldValue::Uint64(42)].into()),
            FieldValue::List(
                vec![
                    // Integer-typed but non-homogeneous FieldValue entries are okay.
                    FieldValue::Int64(-1),
                    FieldValue::Uint64(1),
                    FieldValue::Null,
                ]
                .into(),
            ),
        ];

        for value in &non_nullable_values {
            // Values without nulls match both the nullable and the non-nullable types.
            for matching_type in &nullable_contents_matching_types {
                assert!(matching_type.is_valid_value(value), "{matching_type} {value:?}",);
            }
            for matching_type in &non_nullable_contents_matching_types {
                assert!(matching_type.is_valid_value(value), "{matching_type} {value:?}",);
            }

            // Regardless of nulls, these types don't match.
            for non_matching_type in &non_matching_types {
                assert!(!non_matching_type.is_valid_value(value), "{non_matching_type} {value:?}",);
            }
        }

        for value in &nullable_values {
            // Nullable values match only the nullable types.
            for matching_type in &nullable_contents_matching_types {
                assert!(matching_type.is_valid_value(value), "{matching_type} {value:?}",);
            }

            // The nullable values don't match the non-nullable types.
            for non_matching_type in &non_nullable_contents_matching_types {
                assert!(!non_matching_type.is_valid_value(value), "{non_matching_type} {value:?}",);
            }

            // Regardless of nulls, these types don't match.
            for non_matching_type in &non_matching_types {
                assert!(!non_matching_type.is_valid_value(value), "{non_matching_type} {value:?}",);
            }
        }
    }
}
