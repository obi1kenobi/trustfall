use std::{
    cell::RefCell,
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{BufWriter, Write},
    rc::Rc,
    sync::Arc,
};

use feed_rs::{model::Feed, parser};
use serde::Deserialize;
use trustfall::{
    FieldValue, Schema, execute_query,
};

#[macro_use]
extern crate lazy_static;

use crate::adapter::FeedAdapter;

mod adapter;

const PCGAMER_FEED_URI: &str =
    "https://airedale.futurecdn.net/feeds/en_feed_96a4cb95.rss-fse?nb_results=50&site=pcgamer";
const WIRED_FEED_URI: &str = "https://www.wired.com/feed";
const PCGAMER_FEED_LOCATION: &str = "/tmp/feeds-pcgamer.xml";
const WIRED_FEED_LOCATION: &str = "/tmp/feeds-wired.xml";

lazy_static! {
    static ref SCHEMA: Schema =
        Schema::parse(fs::read_to_string("./src/feeds.graphql").unwrap()).unwrap();
}

#[derive(Debug, Clone, Deserialize)]
struct InputQuery<'a> {
    query: &'a str,

    args: BTreeMap<Arc<str>, FieldValue>,
}

fn refresh_data() {
    let data = [
        (PCGAMER_FEED_URI, PCGAMER_FEED_LOCATION),
        (WIRED_FEED_URI, WIRED_FEED_LOCATION),
    ];
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
    let content = fs::read_to_string(path).unwrap();
    let input_query: InputQuery = ron::from_str(&content).unwrap();

    let data = read_feed_data();
    let adapter = Rc::new(RefCell::new(FeedAdapter::new(&data)));

    let query = input_query.query;
    let arguments = input_query.args;

    for data_item in execute_query(&SCHEMA, adapter, query, arguments).unwrap() {
        println!("\n{}", serde_json::to_string_pretty(&data_item).unwrap());
    }
}

fn read_feed_data() -> Vec<Feed> {
    let data = [
        (PCGAMER_FEED_URI, PCGAMER_FEED_LOCATION),
        (WIRED_FEED_URI, WIRED_FEED_LOCATION),
    ];

    data.iter()
        .map(|(feed_uri, feed_file)| {
            let data_bytes = fs::read(feed_file).unwrap();
            feed_rs::parser::parse_with_uri(data_bytes.as_slice(), Some(feed_uri)).unwrap()
        })
        .collect()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut reversed_args: Vec<_> = args.iter().map(|x| x.as_str()).rev().collect();

    reversed_args
        .pop()
        .expect("Expected the executable name to be the first argument, but was missing");

    match reversed_args.pop() {
        None => panic!("No command given"),
        Some("refresh") => refresh_data(),
        Some("query") => match reversed_args.pop() {
            None => panic!("No filename provided"),
            Some(path) => {
                assert!(reversed_args.is_empty());
                run_query(path)
            }
        },
        Some(cmd) => panic!("Unrecognized command given: {cmd}"),
    }
}
