use std::collections::BTreeMap;
use std::sync::Arc;
use std::{env, process};

use once_cell::sync::Lazy;
use serde::Deserialize;
use trustfall::{execute_query, FieldValue, Schema, TransparentValue};

use crate::adapter::HackerNewsAdapter;

pub mod adapter;
mod util;
pub mod vertex;

static SCHEMA: Lazy<Schema> = Lazy::new(|| {
    Schema::parse(util::read_file("./examples/hackernews/hackernews.graphql")).unwrap()
});

#[derive(Debug, Clone, Deserialize)]
struct InputQuery<'a> {
    query: &'a str,

    args: BTreeMap<Arc<str>, FieldValue>,
}

fn run_query(path: &str, max_results: Option<usize>) {
    let content = util::read_file(path);
    let input_query: InputQuery = ron::from_str(&content).unwrap();

    let adapter = Arc::new(HackerNewsAdapter::new());

    let query = input_query.query;
    let arguments = input_query.args;

    for data_item in execute_query(&SCHEMA, adapter, query, arguments)
        .expect("not a legal query")
        .take(max_results.unwrap_or(usize::MAX))
    {
        // The default `FieldValue` JSON representation is explicit about its type, so we can get
        // reliable round-trip serialization of types tricky in JSON like integers and floats.
        //
        // The `TransparentValue` type is like `FieldValue` minus the explicit type representation,
        // so it's more like what we'd expect to normally find in JSON.
        let transparent: BTreeMap<_, TransparentValue> =
            data_item.into_iter().map(|(k, v)| (k, v.into())).collect();
        println!("\n{}", serde_json::to_string_pretty(&transparent).unwrap());
    }
}

const USAGE: &str = "\
Commands:
    query <query-file> [<max_results>]  - run the query in the given file over the HackerNews API
                                          optionally: fetching no more than <max_results>

Examples: (paths relative to `trustfall` crate directory)
    Links on the front page (as opposed to text submissions like \"Ask HN\"):
        cargo run --example hackernews query ./examples/hackernews/example_queries/front_page_stories_with_links.ron

    Latest links submitted by users with min 10000 karma
        cargo run --example hackernews query ./examples/hackernews/example_queries/latest_links_by_high_karma_users.ron

    patio11 commenting on his own blog posts
        cargo run --example hackernews query ./examples/hackernews/example_queries/patio11_comments_on_own_blog_posts.ron
";

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut reversed_args: Vec<_> = args.iter().map(|x| x.as_str()).rev().collect();

    reversed_args
        .pop()
        .expect("Expected the executable name to be the first argument, but was missing");

    match reversed_args.pop() {
        None => {
            println!("ERROR: no query file provided\n");
            println!("{USAGE}");
            process::exit(1);
        }
        Some("query") => match reversed_args.pop() {
            None => {
                println!("ERROR: no query file provided\n");
                println!("{USAGE}");
                process::exit(1);
            }
            Some(path) => {
                let max_results = reversed_args.pop().map(|value| match value.parse() {
                    Ok(x) => x,
                    Err(e) => {
                        println!("ERROR: value for 'max_results' was not a number: {e}");
                        println!("{USAGE}");
                        process::exit(1);
                    }
                });
                if !reversed_args.is_empty() {
                    println!("ERROR: unexpected arguments for 'query' command\n");
                    println!("{USAGE}");
                    process::exit(1);
                }
                run_query(path, max_results)
            }
        },
        Some(cmd) => {
            println!("ERROR: unexpected command '{cmd}'\n");
            println!("{USAGE}");
            process::exit(1);
        }
    }
}
