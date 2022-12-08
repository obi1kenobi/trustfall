use std::{env, fs};

mod schema_inference;

#[macro_use]
extern crate maplit;

use crate::schema_inference::infer_schema_from_query;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut reversed_args: Vec<_> = args.iter().map(|x| x.as_str()).rev().collect();

    reversed_args
        .pop()
        .expect("Expected the executable name to be the first argument, but was missing");

    match reversed_args.pop() {
        None => panic!("No command given"),
        Some("make_schema") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                make_schema(path)
            }
        },
        Some(cmd) => panic!("Unrecognized command given: {cmd}"),
    }
}

fn make_schema(path: &str) {
    let input_query = fs::read_to_string(path).unwrap();
    match infer_schema_from_query(&input_query) {
        Ok(schema) => println!("{schema}"),
        Err(e) => println!("Error: {e}"),
    }
}
