use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{BufWriter, Write},
    process,
    sync::{Arc, OnceLock},
};

use feed_rs::{model::Feed, parser};
use serde::Deserialize;
use trustfall::{execute_query, FieldValue, Schema, TransparentValue};

use crate::adapter::FeedAdapter;

mod adapter;
mod util;

const PCGAMER_FEED_URI: &str =
    "https://airedale.futurecdn.net/feeds/en_feed_96a4cb95.rss-fse?nb_results=50&site=pcgamer";
const WIRED_FEED_URI: &str = "https://www.wired.com/feed";
const PCGAMER_FEED_LOCATION: &str = "/tmp/feeds-pcgamer.xml";
const WIRED_FEED_LOCATION: &str = "/tmp/feeds-wired.xml";

static SCHEMA: OnceLock<Schema> = OnceLock::new();

fn get_schema() -> &'static Schema {
    SCHEMA.get_or_init(|| Schema::parse(util::read_file("./examples/feeds/feeds.graphql")).expect("valid schema"))
}

#[derive(Debug, Clone, Deserialize)]
struct InputQuery<'a> {
    query: &'a str,

    args: BTreeMap<Arc<str>, FieldValue>,
}

fn refresh_data() {
    let data = [(PCGAMER_FEED_URI, PCGAMER_FEED_LOCATION), (WIRED_FEED_URI, WIRED_FEED_LOCATION)];
    for (uri, location) in data {
        let all_data = reqwest::blocking::get(uri).unwrap().bytes().unwrap();
        let write_file_path = location.to_owned() + "-temp";

        let write_file = File::create(&write_file_path).unwrap();
        let mut buf_writer = BufWriter::new(write_file);
        buf_writer.write_all(all_data.as_ref()).unwrap();
        drop(buf_writer);

        // Ensure the feed data parses successfully.
        parser::parse(all_data.as_ref()).unwrap();

        // We finished writing successfully, so overwrite the cache file location.
        fs::rename(write_file_path, location).unwrap();
    }
}

fn run_query(path: &str) {
    let content = util::read_file(path);
    let input_query: InputQuery = ron::from_str(&content).unwrap();

    let data = read_feed_data();
    let adapter = Arc::new(FeedAdapter::new(&data));
    let schema = get_schema();

    let query = input_query.query;
    let variables = input_query.args;

    for data_item in execute_query(schema, adapter, query, variables).expect("not a legal query") {
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

fn read_feed_data() -> Vec<Feed> {
    let data = [(PCGAMER_FEED_URI, PCGAMER_FEED_LOCATION), (WIRED_FEED_URI, WIRED_FEED_LOCATION)];

    data.iter()
        .map(|(feed_uri, feed_file)| {
            let data_bytes = fs::read(feed_file).unwrap_or_else(|_| {
                refresh_data();
                fs::read(feed_file).expect("failed to read feed file")
            });

            feed_rs::parser::parse_with_uri(data_bytes.as_slice(), Some(feed_uri)).unwrap()
        })
        .collect()
}

const USAGE: &str = "\
Commands:
    refresh             - download feed data, overwriting any previously-downloaded data
    query <query-file>  - run the query in the given file over the downloaded feed data

Examples: (paths relative to `trustfall` crate directory)
    Extract titles and all links in each feed entry:
        cargo run --example feeds query ./examples/feeds/example_queries/feed_links.ron

    Find PCGamer game reviews:
        cargo run --example feeds query ./examples/feeds/example_queries/game_reviews.ron
";

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut reversed_args: Vec<_> = args.iter().map(|x| x.as_str()).rev().collect();

    reversed_args
        .pop()
        .expect("Expected the executable name to be the first argument, but was missing");

    match reversed_args.pop() {
        None => {
            println!("{USAGE}");
            process::exit(1);
        }
        Some("refresh") => {
            refresh_data();
            println!("Data refreshed successfully!");
        }
        Some("query") => match reversed_args.pop() {
            None => {
                println!("ERROR: no query file provided\n");
                println!("{USAGE}");
                process::exit(1);
            }
            Some(path) => {
                if !reversed_args.is_empty() {
                    println!("ERROR: 'query' command takes only a single filename argument\n");
                    println!("{USAGE}");
                    process::exit(1);
                }
                run_query(path)
            }
        },
        Some(cmd) => {
            println!("ERROR: unexpected command '{cmd}'\n");
            println!("{USAGE}");
            process::exit(1);
        }
    }
}
