use core::fmt::{self, Formatter};
use std::{fmt::Display, sync::Arc};

use async_graphql_parser::types::{BaseType, Type as GQLType};
use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

/// A backing-storage independent immutable representation of a GraphQL type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    base: Arc<str>,
    modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Modifiers {
    mask: u64, // space for ~30 levels of list nesting
}

impl Modifiers {
    const NON_NULLABLE_MASK: u64 = 1;
    const LIST_MASK: u64 = 2;
    const MAX_LIST_DEPTH: u64 = 30;
    // represents the most far left list bit that can be set before adding a new list will overflow
    const MAX_LIST_DEPTH_MASK: u64 = Self::LIST_MASK << ((Self::MAX_LIST_DEPTH - 1) * 2); // - 1 because we start shifted over once.

    fn nullable(&self) -> bool {
        (self.mask & Self::NON_NULLABLE_MASK) == 0
    }

    fn is_list(&self) -> bool {
        (self.mask & Self::LIST_MASK) != 0
    }

    fn as_list(&self) -> Option<Modifiers> {
        self.is_list().then_some(Modifiers { mask: self.mask >> 2 })
    }

    fn at_max_list_depth(&self) -> bool {
        (self.mask & (Self::MAX_LIST_DEPTH_MASK)) == (Self::MAX_LIST_DEPTH_MASK)
    }
}

impl Type {
    /// Creates a new [`Type`] from a string.
    /// Returns `None` if the string is not a valid GraphQL type.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let ty = Type::new("[String!]!").unwrap();
    /// assert_eq!(ty.to_string(), "[String!]!");
    ///
    /// assert_eq!(Type::new("[String!]").unwrap().to_string(), "[String!]");
    /// ```
    pub fn new(ty: &str) -> Option<Self> {
        Some(Self::from_type(&GQLType::new(ty)?))
    }

    /// Creates an individual [`Type`], not a list.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let nullable = false;
    /// let ty = Type::new_named_type("String", nullable);
    ///
    /// assert_eq!(ty.to_string(), "String!");
    /// assert_eq!(ty, Type::new("String!").unwrap());
    /// ```
    pub fn new_named_type(base_type_name: &str, nullable: bool) -> Self {
        Self {
            base: base_type_name.to_string().into(),
            modifiers: Modifiers { mask: if nullable { 0 } else { Modifiers::NON_NULLABLE_MASK } },
        }
    }

    /// Creates a new list layer on a [`Type`].
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let inner_nullable = false;
    /// let inner_ty = Type::new_named_type("String", inner_nullable);
    ///
    /// let outer_nullable = true;
    /// let ty = Type::new_list_type(inner_ty, outer_nullable);
    ///
    /// assert_eq!(ty.to_string(), "[String!]");
    /// assert_eq!(ty, Type::new("[String!]").unwrap());
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
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let nullable_ty = Type::new("Int").unwrap();
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
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let nullable_ty = Type::new("[Int!]").unwrap();
    /// assert_eq!(nullable_ty.nullable(), true); // the list is nullable
    ///
    /// let nullable_ty = Type::new("Int!").unwrap();
    /// assert_eq!(nullable_ty.nullable(), false); // the `Int` is nonnullable
    /// ```
    pub fn nullable(&self) -> bool {
        self.modifiers.nullable()
    }

    /// Returns whether the type is a list or not.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let non_null_int_arr = Type::new("[Int!]").unwrap();
    /// assert_eq!(non_null_int_arr.is_list(), true);
    ///
    /// let non_null_int = Type::new("Int!").unwrap();
    /// assert_eq!(non_null_int.is_list(), false);
    /// ```
    pub fn is_list(&self) -> bool {
        self.modifiers.is_list()
    }

    /// Returns the type inside the outermost list of this type if it is a list, otherwise returns `None`.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let non_null_int_arr = Type::new("[Int!]").unwrap();
    /// let non_null_int = Type::new("Int!").unwrap();
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
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let int_list_ty = Type::new("[Int!]").unwrap();
    /// assert_eq!(int_list_ty.base_named_type(), "Int");
    ///
    /// let string_ty = Type::new("String!").unwrap();
    /// assert_eq!(string_ty.base_named_type(), "String");
    pub fn base_named_type(&self) -> &str {
        &self.base
    }

    /// Convert a [`GQLType`] to a [`Type`].
    pub(crate) fn from_type(ty: &GQLType) -> Type {
        let mut base = &ty.base;

        let mut mask = if ty.nullable { 0 } else { Modifiers::NON_NULLABLE_MASK };

        let mut i = 0;

        while let BaseType::List(ty_inside_list) = base {
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

        let BaseType::Named(name) = base else {unreachable!("should be impossible to get a non-named type after looping through all list types")};

        Type { base: name.to_string().into(), modifiers: Modifiers { mask } }
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

            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("GraphQL type")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let ty = Type::new(s)
                    .ok_or_else(|| serde::de::Error::custom("not a valid GraphQL type"))?;
                Ok(ty)
            }
        }

        deserializer.deserialize_str(TypeDeserializer)
    }
}

#[cfg(test)]
mod test {
    use crate::ir::ty::Type;

    use super::Modifiers;

    #[test]
    fn max_allowed_nested_lists() {
        let my_str = format!(
            "{}String{}",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize),
            "]".repeat(Modifiers::MAX_LIST_DEPTH as usize)
        );
        let constructed_type = Type::new(&my_str).unwrap().modifiers;
        assert!(constructed_type.at_max_list_depth());
    }

    #[test]
    #[should_panic(expected = "too many nested lists")]
    fn too_many_nested_lists_via_type_new() {
        let my_str = format!(
            "{}String!{}",
            "[".repeat(Modifiers::MAX_LIST_DEPTH as usize + 1),
            "]".repeat(Modifiers::MAX_LIST_DEPTH as usize + 1)
        );
        Type::new(&my_str).unwrap();
    }

    #[test]
    #[should_panic(expected = "too many nested lists")]
    fn too_many_nested_lists_via_new_list_type() {
        let mut constructed_type = Type::new_named_type("String", false);
        for _ in 0..=Modifiers::MAX_LIST_DEPTH + 1 {
            constructed_type = Type::new_list_type(constructed_type, false);
        }
    }
}
