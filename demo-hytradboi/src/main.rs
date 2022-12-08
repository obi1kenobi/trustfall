use std::collections::BTreeMap;
use std::env;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{cell::RefCell, fs};

use adapter::DemoAdapter;
use serde::Deserialize;
use trustfall_core::ir::TransparentValue;
use trustfall_core::{
    frontend::parse, interpreter::execution::interpret_ir, ir::FieldValue, schema::Schema,
};

#[macro_use]
extern crate lazy_static;

mod actions_parser;
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

    let max_results = 20usize;

    println!("Executing query:");
    println!("{}", input_query.query.trim());

    // Printing "prettily" (without the enum wrapper that captures the value type)
    // unfortunately takes a bit of ceremony at the moment.
    println!("\nQuery args:");
    println!(
        "{:?}",
        arguments
            .as_ref()
            .clone()
            .into_iter()
            .map(|(k, v)| (
                k,
                serde_json::to_string_pretty(&TransparentValue::from(v)).unwrap()
            ))
            .collect::<BTreeMap<_, _>>()
    );

    println!("\nGetting max {max_results} results to avoid exhausting rate limit budgets.");

    let mut total_query_duration: Duration = Default::default();
    let mut current_instant = Instant::now();
    for (index, data_item) in interpret_ir(adapter, query, arguments).unwrap().enumerate() {
        let next_item_duration = current_instant.elapsed();
        total_query_duration += next_item_duration;

        // Use the value variant with an untagged enum serialization, to make the printout cleaner.
        let data_item: BTreeMap<Arc<str>, TransparentValue> =
            data_item.into_iter().map(|(k, v)| (k, v.into())).collect();

        let result_number = index + 1;
        println!(
            "\nResult {result_number} fetched in {next_item_duration:?}, {}",
            serde_json::to_string_pretty(&data_item).unwrap()
        );

        // Uncomment the following line when recording the shell session,
        // to ensure each result gets at least one frame in the output.
        // Otherwise, all results get dumped in the shell all at once.
        // std::thread::sleep(Duration::from_millis(16));

        // Safety valve: we're using rate-limited APIs.
        // Don't exhaust entire API call budget at once!
        if result_number == max_results {
            println!(
                "\nFetched {max_results} results in {total_query_duration:?}; \
                terminating iteration to avoid exhausting rate limit budget."
            );
            break;
        }

        current_instant = Instant::now();
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
        Some(cmd) => panic!("Unrecognized command given: {cmd}"),
    }
}
