use std::{fmt::Display, fmt::Formatter, sync::Arc};

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

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
    use crate::ir::Type;

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
}
