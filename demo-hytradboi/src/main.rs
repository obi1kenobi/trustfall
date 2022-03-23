use std::collections::BTreeMap;
use std::env;
use std::rc::Rc;
use std::sync::Arc;
use std::{cell::RefCell, fs};

use adapter::DemoAdapter;
use serde::Deserialize;
use trustfall_core::ir::TransparentValue;
use trustfall_core::{
    frontend::parse, interpreter::execution::interpret_ir, ir::FieldValue, schema::Schema,
};

#[macro_use]
extern crate lazy_static;

mod adapter;
mod pagers;
mod token;
mod util;

lazy_static! {
    static ref SCHEMA: Schema =
        Schema::parse(fs::read_to_string("./schema.graphql").unwrap()).unwrap();
}

#[derive(Debug, Clone, Deserialize)]
struct InputQuery<'a> {
    query: &'a str,

    args: Arc<BTreeMap<Arc<str>, FieldValue>>,
}

fn execute_query(path: &str) {
    let content = fs::read_to_string(path).unwrap();
    let input_query: InputQuery = ron::from_str(&content).unwrap();

    let adapter = Rc::new(RefCell::new(DemoAdapter::new()));

    let query = parse(&SCHEMA, input_query.query).unwrap();
    let arguments = input_query.args;

    for (index, data_item) in interpret_ir(adapter, query, arguments).unwrap().enumerate() {
        // Use the value variant with an untagged enum serialization, to make the printout cleaner.
        let data_item: BTreeMap<Arc<str>, TransparentValue> =
            data_item.into_iter().map(|(k, v)| (k, v.into())).collect();

        println!("\n{}", serde_json::to_string_pretty(&data_item).unwrap());

        if index == 10 {
            break;
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut reversed_args: Vec<_> = args.iter().map(|x| x.as_str()).rev().collect();

    reversed_args
        .pop()
        .expect("Expected the executable name to be the first argument, but was missing");

    match reversed_args.pop() {
        None => panic!("No command given"),
        Some("query") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                execute_query(path)
            }
        },
        Some(cmd) => panic!("Unrecognized command given: {}", cmd),
    }
}
