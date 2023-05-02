use std::{collections::BTreeMap, sync::Arc};

use serde::Deserialize;

use super::TryIntoStruct;
use crate::ir::FieldValue;

#[test]
fn deserialize_simple() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Output {
        foo: i64,
        bar: String,
    }

    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("foo") => FieldValue::Int64(42),
        Arc::from("bar") => FieldValue::String("the answer to everything".to_string()),
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(
        Output {
            foo: 42,
            bar: "the answer to everything".to_string(),
        },
        output_value
    );
}

#[test]
fn deserialize_list() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Output {
        foo: Vec<Vec<i64>>,
        bar: Vec<String>,
    }

    let vec_int = vec![vec![1, 2], vec![], vec![3, 4]];
    let vec_str: Vec<String> = vec!["one".into(), "".into(), "two".into(), "three".into()];

    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("foo") => FieldValue::List(vec_int.clone().into_iter().map(|x| FieldValue::List(x.into_iter().map(Into::into).collect())).collect()),
        Arc::from("bar") => FieldValue::List(vec_str.clone().into_iter().map(Into::into).collect()),
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(
        Output {
            foo: vec_int,
            bar: vec_str,
        },
        output_value
    );
}

#[test]
fn deserialize_option() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Output {
        foo: Vec<Option<i64>>,
        bar: Option<String>,
    }

    let vec_int = vec![Some(1), None, Some(2), Some(3)];
    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("foo") => FieldValue::List(vec_int.clone().into_iter().map(Into::into).collect()),
        Arc::from("bar") => FieldValue::Null,
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(
        Output {
            foo: vec_int,
            bar: None,
        },
        output_value
    );
}

#[test]
fn deserialize_extra_keys_in_query_result() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Output {
        foo: i64,
        bar: String,
    }

    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("foo") => FieldValue::Int64(42),
        Arc::from("bar") => FieldValue::String("the answer to everything".to_string()),
        Arc::from("extra") => FieldValue::Null,
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(
        Output {
            foo: 42,
            bar: "the answer to everything".to_string(),
        },
        output_value
    );
}

#[test]
fn deserialize_serde_rename() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Output {
        #[serde(rename = "renamed_foo")]
        foo: i64,

        #[serde(alias = "renamed_bar")]
        bar: String,
    }

    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("renamed_foo") => FieldValue::Int64(42),
        Arc::from("renamed_bar") => FieldValue::String("the answer to everything".to_string()),
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(
        Output {
            foo: 42,
            bar: "the answer to everything".to_string(),
        },
        output_value
    );
}

#[test]
fn deserialize_narrower_types() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Output {
        i32: i32,
        i16: i16,
        i8: i8,
        u32: u32,
        u16: u16,
        u8: u8,
    }

    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("i32") => FieldValue::Int64(-32),
        Arc::from("i16") => FieldValue::Int64(-16),
        Arc::from("i8") => FieldValue::Int64(8),
        Arc::from("u32") => FieldValue::Uint64(32),
        Arc::from("u16") => FieldValue::Uint64(16),
        Arc::from("u8") => FieldValue::Uint64(8),
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(
        Output {
            i32: -32,
            i16: -16,
            i8: 8,
            u32: 32,
            u16: 16,
            u8: 8,
        },
        output_value
    );
}

#[test]
fn deserialize_narrower_type_f32() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Output {
        f32: f32,
    }

    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("f32") => FieldValue::Float64(1.234),
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(Output { f32: 1.234f32 }, output_value);
}

#[test]
fn deserialize_adjust_numeric_type_signedness() {
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Output {
        i64: i64,
        i32: i32,
        i16: i16,
        i8: i8,
        u64: u64,
        u32: u32,
        u16: u16,
        u8: u8,
    }

    let value: BTreeMap<Arc<str>, FieldValue> = btreemap! {
        Arc::from("i64") => FieldValue::Uint64(i64::MAX as u64),
        Arc::from("i32") => FieldValue::Uint64(i32::MAX as u64),
        Arc::from("i16") => FieldValue::Uint64(i16::MAX as u64),
        Arc::from("i8") => FieldValue::Uint64(i8::MAX as u64),
        Arc::from("u64") => FieldValue::Int64(i64::MAX),
        Arc::from("u32") => FieldValue::Int64(i32::MAX.into()),
        Arc::from("u16") => FieldValue::Int64(i16::MAX.into()),
        Arc::from("u8") => FieldValue::Int64(i8::MAX.into()),
    };

    let output_value = value
        .try_into_struct::<Output>()
        .expect("failed to create struct");
    assert_eq!(
        Output {
            i64: i64::MAX,
            i32: i32::MAX,
            i16: i16::MAX,
            i8: i8::MAX,
            u64: i64::MAX as u64,
            u32: i32::MAX as u32,
            u16: i16::MAX as u16,
            u8: i8::MAX as u8,
        },
        output_value
    );
}
