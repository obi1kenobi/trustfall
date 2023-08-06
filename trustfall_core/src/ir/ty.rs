use core::fmt::{self, Formatter};
use std::fmt::Display;

use async_graphql_parser::types::{
    BaseType::{self, List, Named},
    Type as GQLType,
};
use async_graphql_value::Name;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    ty: GQLType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InnerType<'a> {
    NameOfType(&'a str),
    ListInnerType(Type),
}

/// A backing-storage independent immutable representation of a GraphQL type.
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
    /// ```
    pub fn new(ty: &str) -> Option<Type> {
        Some(Type { ty: GQLType::new(ty)? })
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
    pub fn new_named_type(base_type_name: &str, nullable: bool) -> Type {
        Type { ty: GQLType { base: BaseType::Named(Name::new(base_type_name)), nullable } }
    }

    /// Creates a new list [`Type`] from an individual [`Type`].
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
    pub fn new_list_type(inner_type: Type, nullable: bool) -> Type {
        Type { ty: GQLType { base: BaseType::List(Box::new(inner_type.ty)), nullable } }
    }

    /// Returns a new type that is the same as this one, but with the passed nullability.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let nullable_ty = Type::new("Int").unwrap();
    /// assert_eq!(nullable_ty.is_nullable(), true);
    /// let non_nullable_ty = nullable_ty.with_nullability(false);
    /// assert_eq!(non_nullable_ty.is_nullable(), false);
    ///
    /// // The original type is unchanged.
    /// assert_eq!(nullable_ty.is_nullable(), true);
    /// ```
    pub fn with_nullability(&self, nullable: bool) -> Type {
        Type { ty: GQLType { base: self.ty.base.clone(), nullable } }
    }

    /// Returns whether this type is nullable, at the top level, see example.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::Type;
    ///
    /// let nullable_ty = Type::new("[Int!]").unwrap();
    /// assert_eq!(nullable_ty.is_nullable(), true); // the list is nullable
    ///
    /// let nullable_ty = Type::new("Int!").unwrap();
    /// assert_eq!(nullable_ty.is_nullable(), false); // the `Int` is nonnullable
    /// ```
    pub fn is_nullable(&self) -> bool {
        self.ty.nullable
    }

    /// Returns an [`InnerType`] which represents the inner value of the type.
    /// If the type is a list, the inner type is the type of the list's elements.
    ///
    /// # Example
    /// ```
    /// use trustfall_core::ir::ty::{*};
    ///
    /// let individual_ty = Type::new("Int!").unwrap();
    /// assert!(matches!(individual_ty.value(), InnerType::NameOfType("Int")));
    ///
    /// let list_ty = Type::new("[Int!]!").unwrap();
    /// let inner_ty_for_assert = Type::new("Int!").unwrap();
    /// assert!(matches!(list_ty.value(), InnerType::ListInnerType(inner_ty) if inner_ty == inner_ty_for_assert));
    /// ```
    pub fn value(&self) -> InnerType<'_> {
        match &self.ty.base {
            Named(n) => InnerType::NameOfType(n),
            List(ty) => InnerType::ListInnerType(Type { ty: (**ty).clone() }),
        }
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
        let mut value = &self.ty.base;
        while let BaseType::List(l) = value {
            value = &l.base;
        }

        match value {
            Named(n) => n,
            List(_) => unreachable!("while loop should not have stopped on a list"),
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ty)
    }
}

impl Serialize for Type {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.ty.to_string())
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

impl<'a> PartialEq<&'a Type> for Type {
    fn eq(&self, other: &&'a Type) -> bool {
        self.ty == other.ty
    }
}

pub(crate) fn from_type(ty: &GQLType) -> Type {
    Type { ty: ty.clone() }
}
