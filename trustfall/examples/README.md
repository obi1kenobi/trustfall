# Querying different data sources with Trustfall in Rust

This directory contains examples of using Trustfall in Rust projects that query a variety of sources:
- [CSV files](#querying-csv-files-aviation-weather-reports-metar)
- [REST APIs](#querying-apis-hackernews)
- [RSS and Atom feeds](#querying-rss-and-atom-feeds).

Each project comes with example queries in an `example_queries` directory.

## Querying CSV files: aviation weather reports (METAR)

The [weather](weather/) demo project queries meteorogical data from the US Aviation Weather Center: [https://www.aviationweather.gov/metar](https://www.aviationweather.gov/metar)

This data is available as a CSV file. The demo project downloads and parses this file, then runs Trustfall queries on it.

## Querying APIs: HackerNews

The [hackernews](hackernews/) demo project queries the HackerNews REST API endpoints.

Each Trustfall query is turned into a sequence of API calls that execute against the live HackerNews API endpoints.

There is no local data ingestion step in this demo — we don't ETL the data into a local database or save it as local files. That functionality *could* be added as an optimization, but is not *required* to use Trustfall on an API.

## Querying RSS and Atom feeds

The [feeds](feeds/) demo project runs Trustfall queries over the feeds of PCGamer and Wired magazines.

It downloads the feed contents (as XML data files), then parses those files and runs Trustfall queries on them.
