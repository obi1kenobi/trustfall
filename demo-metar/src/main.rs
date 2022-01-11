use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::{cell::RefCell, fs};

use serde::Deserialize;
use trustfall_core::{
    frontend::parse, interpreter::execution::interpret_ir, ir::FieldValue, schema::Schema,
};

use crate::{
    adapter::MetarAdapter,
    metar::{CsvMetarReport, MetarReport},
};

#[macro_use]
extern crate lazy_static;

mod adapter;
mod metar;

lazy_static! {
    static ref SCHEMA: Schema =
        Schema::parse(fs::read_to_string("./src/metar_weather.graphql").unwrap()).unwrap();
}

const METAR_DOC_URL: &str =
    "https://aviationweather.gov/adds/dataserver_current/current/metars.cache.csv";
const METAR_DOC_LOCATION: &str = "/tmp/metars-clean.cache.csv";
const METAR_DOC_HEADER_ROW: &str = "\
raw_text,station_id,observation_time,latitude,longitude,temp_c,dewpoint_c,\
wind_dir_degrees,wind_speed_kt,wind_gust_kt,visibility_statute_mi,\
altim_in_hg,sea_level_pressure_mb,corrected,auto,auto_station,\
maintenance_indicator_on,no_signal,lightning_sensor_off,freezing_rain_sensor_off,\
present_weather_sensor_off,wx_string,\
sky_cover,cloud_base_ft_agl,sky_cover,cloud_base_ft_agl,\
sky_cover,cloud_base_ft_agl,sky_cover,cloud_base_ft_agl,\
flight_category,three_hr_pressure_tendency_mb,\
maxT_c,minT_c,maxT24hr_c,minT24hr_c,precip_in,pcp3hr_in,pcp6hr_in,pcp24hr_in,\
snow_in,vert_vis_ft,metar_type,elevation_m";

#[derive(Debug, Clone, Deserialize)]
struct InputQuery<'a> {
    query: &'a str,

    args: Arc<HashMap<Arc<str>, FieldValue>>,
}

fn read_metar_data() -> Vec<MetarReport> {
    let data_file = File::open(METAR_DOC_LOCATION).unwrap();
    let mut reader = BufReader::new(data_file);

    let mut buf = String::new();

    // strip the CSV prefix and the header row
    let prefix_len = 6;
    for _ in 0..prefix_len {
        reader.read_line(&mut buf).unwrap();

        match buf.as_str().trim() {
            "No errors" | "No warnings" | "data source=metars" | METAR_DOC_HEADER_ROW => {}
            data => match data.split_once(' ') {
                Some((left, right)) if right == "ms" || right == "results" => {
                    assert!(left.chars().all(|x| x.is_digit(10)));
                }
                _ => unreachable!(),
            },
        }

        buf.truncate(0);
    }

    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(reader);

    let metars: Vec<MetarReport> = csv_reader
        .deserialize::<CsvMetarReport>()
        .map(|x| x.unwrap().into())
        .collect();

    metars
}

fn execute_query(path: &str) {
    let content = fs::read_to_string(path).unwrap();
    let input_query: InputQuery = ron::from_str(&content).unwrap();

    let data = read_metar_data();
    let adapter = Rc::new(RefCell::new(MetarAdapter::new(&data)));

    let query = parse(&SCHEMA, input_query.query).unwrap();
    let arguments = input_query.args;

    for data_item in interpret_ir(adapter, query, arguments).unwrap() {
        println!("\n{}", serde_json::to_string_pretty(&data_item).unwrap());
    }
}

fn refresh_data() {
    let all_data = reqwest::blocking::get(METAR_DOC_URL)
        .unwrap()
        .text()
        .unwrap();
    let write_file_path = METAR_DOC_LOCATION.to_owned() + "-temp";

    let write_file = File::create(&write_file_path).unwrap();
    let mut buf_writer = BufWriter::new(write_file);

    for line in all_data.lines() {
        if line.contains("AUTO NIL") {
            continue;
        }
        buf_writer.write_all(line.as_bytes()).unwrap();
        buf_writer.write_all("\n".as_bytes()).unwrap();
    }
    drop(buf_writer);

    // We finished writing successfully, so overwrite the cache file location.
    fs::rename(write_file_path, METAR_DOC_LOCATION).unwrap();
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
                execute_query(path)
            }
        },
        Some(cmd) => panic!("Unrecognized command given: {}", cmd),
    }
}
