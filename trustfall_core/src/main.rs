#![forbid(unsafe_code)]
#![forbid(unused_lifetimes)]

#[macro_use]
extern crate maplit;

#[macro_use]
extern crate lazy_static;

mod filesystem_interpreter;
mod frontend;
mod graphql_query;
mod interpreter;
mod ir;
mod nullables_interpreter;
mod numbers_interpreter;
mod schema;
mod util;

use std::{
    cell::RefCell, collections::BTreeMap, convert::TryInto, env, fmt::Debug, fs, rc::Rc, sync::Arc,
};

use async_graphql_parser::{parse_query, parse_schema};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use trustfall_core::{interpreter::error::QueryArgumentsError, schema::error::InvalidSchemaError};

use crate::{
    filesystem_interpreter::{FilesystemInterpreter, FilesystemToken},
    graphql_query::error::ParseError,
    graphql_query::query::parse_document,
    interpreter::{
        execution,
        trace::{AdapterTap, Trace},
        Adapter,
    },
    nullables_interpreter::NullablesAdapter,
    numbers_interpreter::{NumbersAdapter, NumbersToken},
    schema::Schema,
    util::{
        TestGraphQLQuery, TestIRQuery, TestIRQueryResult, TestInterpreterOutputTrace,
        TestParsedGraphQLQuery, TestParsedGraphQLQueryResult,
    },
};

fn get_schema_by_name(schema_name: &str) -> Schema {
    let schema_text =
        fs::read_to_string(format!("src/resources/schemas/{}.graphql", schema_name,)).unwrap();
    let schema_document = parse_schema(schema_text).unwrap();
    Schema::new(schema_document).unwrap()
}

fn serialize_to_ron<S: Serialize>(s: &S) -> String {
    let mut buf = Vec::new();
    let mut config = ron::ser::PrettyConfig::new();
    config.new_line = "\n".to_string();
    config.indentor = "  ".to_string();
    let mut serializer = ron::ser::Serializer::new(&mut buf, Some(config), true).unwrap();

    s.serialize(&mut serializer).unwrap();
    String::from_utf8(buf).unwrap()
}

fn parse(path: &str) {
    let input_data = fs::read_to_string(path).unwrap();
    let test_query: TestGraphQLQuery = ron::from_str(&input_data).unwrap();

    let arguments = test_query.arguments;
    let result: TestParsedGraphQLQueryResult = parse_query(test_query.query)
        .map_err(ParseError::from)
        .and_then(|doc| parse_document(&doc))
        .map(move |query| TestParsedGraphQLQuery {
            schema_name: test_query.schema_name,
            query,
            arguments,
        });

    println!("{}", serialize_to_ron(&result));
}

fn frontend(path: &str) {
    let input_data = fs::read_to_string(path).unwrap();
    let test_query_result: TestParsedGraphQLQueryResult = ron::from_str(&input_data).unwrap();
    let test_query = test_query_result.unwrap();

    let schema = get_schema_by_name(test_query.schema_name.as_str());

    let arguments = test_query.arguments;
    let ir_query_result = frontend::make_ir_for_query(&schema, &test_query.query);
    let result: TestIRQueryResult = ir_query_result.map(move |ir_query| TestIRQuery {
        schema_name: test_query.schema_name,
        ir_query,
        arguments,
    });

    println!("{}", serialize_to_ron(&result));
}

fn check_fuzzed(path: &str, schema_name: &str) {
    let schema = get_schema_by_name(schema_name);

    let query_string = fs::read_to_string(path).unwrap();

    let query = match frontend::parse(&schema, query_string.as_str()) {
        Ok(query) => query,
        Err(e) => {
            println!("{}", serialize_to_ron(&e));
            return;
        }
    };

    println!("{}", serialize_to_ron(&query));
}

fn trace_with_adapter<'a, AdapterT>(adapter: AdapterT, test_query: TestIRQuery)
where
    AdapterT: Adapter<'a> + Clone + 'a,
    AdapterT::DataToken: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de> AdapterT::DataToken: Deserialize<'de>,
{
    let query = Arc::new(test_query.ir_query.clone().try_into().unwrap());
    let arguments: Arc<BTreeMap<_, _>> = Arc::new(
        test_query
            .arguments
            .iter()
            .map(|(k, v)| (Arc::from(k.to_owned()), v.clone()))
            .collect(),
    );

    let tracer = Rc::new(RefCell::new(Trace::new(
        test_query.ir_query.clone(),
        test_query.arguments,
    )));
    let cloned_adapter = adapter.clone();
    let adapter_tap = Rc::new(RefCell::new(AdapterTap::new(adapter, tracer.clone())));

    let execution_result = execution::interpret_ir(adapter_tap.clone(), query, arguments);
    match execution_result {
        Ok(results_iter) => {
            let results = results_iter.collect_vec();

            let empty_tap = AdapterTap::new(cloned_adapter, tracer);
            let trace = adapter_tap.replace(empty_tap).finish();
            let data = TestInterpreterOutputTrace {
                schema_name: test_query.schema_name,
                trace,
                results,
            };

            println!("{}", serialize_to_ron(&data));
        }
        Err(e) => {
            println!("{}", serialize_to_ron(&e));
        }
    }
}

fn trace(path: &str) {
    let input_data = fs::read_to_string(path).unwrap();
    let test_query_result: TestIRQueryResult = ron::from_str(&input_data).unwrap();
    let test_query = test_query_result.unwrap();

    match test_query.schema_name.as_str() {
        "filesystem" => {
            let adapter = FilesystemInterpreter::new(".".to_owned());
            trace_with_adapter(adapter, test_query);
        }
        "numbers" => {
            let adapter = NumbersAdapter;
            trace_with_adapter(adapter, test_query);
        }
        "nullables" => {
            let adapter = NullablesAdapter;
            trace_with_adapter(adapter, test_query);
        }
        _ => unreachable!("Unknown schema name: {}", test_query.schema_name),
    };
}

fn reserialize(path: &str) {
    let input_data = fs::read_to_string(path).unwrap();

    let (prefix, last_extension) = path.rsplit_once('.').unwrap();
    assert_eq!(last_extension, "ron");

    let output_data = match prefix.rsplit_once('.') {
        Some((_, "graphql")) => {
            let test_query: TestGraphQLQuery = ron::from_str(&input_data).unwrap();
            serialize_to_ron(&test_query)
        }
        Some((_, "graphql-parsed" | "parse-error")) => {
            let test_query_result: TestParsedGraphQLQueryResult =
                ron::from_str(&input_data).unwrap();
            serialize_to_ron(&test_query_result)
        }
        Some((_, "ir" | "frontend-error")) => {
            let test_query_result: TestIRQueryResult = ron::from_str(&input_data).unwrap();
            serialize_to_ron(&test_query_result)
        }
        Some((_, "trace")) => {
            if let Ok(test_trace) =
                ron::from_str::<TestInterpreterOutputTrace<NumbersToken>>(&input_data)
            {
                serialize_to_ron(&test_trace)
            } else if let Ok(test_trace) =
                ron::from_str::<TestInterpreterOutputTrace<FilesystemToken>>(&input_data)
            {
                serialize_to_ron(&test_trace)
            } else {
                unreachable!()
            }
        }
        Some((_, "schema-error")) => {
            let schema_error: InvalidSchemaError = ron::from_str(&input_data).unwrap();
            serialize_to_ron(&schema_error)
        }
        Some((_, "exec-error")) => {
            let exec_error: QueryArgumentsError = ron::from_str(&input_data).unwrap();
            serialize_to_ron(&exec_error)
        }
        Some((_, ext)) => unreachable!("{}", ext),
        None => unreachable!("{}", path),
    };

    println!("{}", output_data);
}

fn schema_error(path: &str) {
    let schema_text = fs::read_to_string(path).unwrap();

    let result = Schema::parse(schema_text);
    match result {
        Err(e) => {
            println!("{}", serialize_to_ron(&e))
        }
        Ok(_) => unreachable!("{}", path),
    }
}

fn corpus_graphql(path: &str, schema_name: &str) {
    let input_data = fs::read_to_string(path).unwrap();

    let (prefix, last_extension) = path.rsplit_once('.').unwrap();
    assert_eq!(last_extension, "ron");

    let output_data = match prefix.rsplit_once('.') {
        Some((_, "graphql")) => {
            let test_query: TestGraphQLQuery = ron::from_str(&input_data).unwrap();
            if test_query.schema_name != schema_name {
                return;
            }
            test_query.query.replace("    ", " ")
        }
        Some((_, ext)) => unreachable!("{}", ext),
        None => unreachable!("{}", path),
    };

    println!("{}", output_data);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut reversed_args: Vec<_> = args.iter().map(|x| x.as_str()).rev().collect();

    reversed_args
        .pop()
        .expect("Expected the executable name to be the first argument, but was missing");

    match reversed_args.pop() {
        None => panic!("No command given"),
        Some("parse") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                parse(path)
            }
        },
        Some("frontend") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                frontend(path)
            }
        },
        Some("trace") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                trace(path)
            }
        },
        Some("schema_error") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                schema_error(path)
            }
        },
        Some("reserialize") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                reserialize(path)
            }
        },
        Some("corpus_graphql") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                let schema_name = reversed_args.pop().expect("schema name");

                assert!(reversed_args.is_empty());
                corpus_graphql(path, schema_name)
            }
        },
        Some("check_fuzzed") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                let schema_name = reversed_args.pop().expect("schema name");

                assert!(reversed_args.is_empty());
                check_fuzzed(path, schema_name)
            }
        },
        Some(cmd) => panic!("Unrecognized command given: {}", cmd),
    }
}
