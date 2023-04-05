# Querying CSV files: aviation weather reports (METAR)

This demo project queries meteorogical data from the US Aviation Weather Center ([https://www.aviationweather.gov/metar](https://www.aviationweather.gov/metar)).

The data is delivered as a CSV file from the following link: [https://aviationweather.gov/adds/dataserver_current/current/metars.cache.csv](https://aviationweather.gov/adds/dataserver_current/current/metars.cache.csv)

## Example: Find the temperature, dewpoint, wind, and clouds in Boston

Query: ([link](trustfall/examples/weather/example_queries/boston_weather.ron))
```graphql
{
    LatestMetarReportForAirport(airport_code: "KBOS") {
        wind_speed_kts @output
        wind_direction @output
        wind_gusts_kts @output
        temperature @output
        dewpoint @output

        cloud_cover @fold {
            sky_cover @output
            base_altitude @output
        }
    }
}
```

To run it:
```
$ cargo run --example weather query ./examples/weather/example_queries/boston_weather.ron
< ... cargo output ... >

{
  "base_altitude": [
    2200
  ],
  "dewpoint": 0.6,
  "sky_cover": [
    "OVC"
  ],
  "temperature": 5.0,
  "wind_direction": 90,
  "wind_gusts_kts": null,
  "wind_speed_kts": 15
}
```

## Example: Which weather stations are reporting 25+ knot winds and gusty conditions?

Query: ([link](trustfall/examples/weather/example_queries/high_winds.ron))
```graphql
{
    MetarReport {
        station_id @output
        latitude @output
        longitude @output

        wind_speed_kts @output @filter(op: ">", value: ["$min_wind"])
        wind_direction @output
        wind_gusts_kts @output @filter(op: "is_not_null")
        temperature @output
        dewpoint @output

        cloud_cover @fold {
            sky_cover @output
            base_altitude @output
        }
    }
}
```
with arguments `{ "min_wind": 25 }`.

To run it:
```
$ cargo run --example weather query ./examples/weather/example_queries/high_winds.ron
< ... cargo output ... >

{
  "base_altitude": [
    3200
  ],
  "dewpoint": -7.0,
  "latitude": 44.9,
  "longitude": -91.87,
  "sky_cover": [
    "OVC"
  ],
  "station_id": "KLUM",
  "temperature": 1.2,
  "wind_direction": 240,
  "wind_gusts_kts": 33,
  "wind_speed_kts": 26
}

{
  "base_altitude": [
    3200
  ],
  "dewpoint": -6.0,
  "latitude": 44.12,
  "longitude": -93.25,
  "sky_cover": [
    "OVC"
  ],
  "station_id": "KOWA",
  "temperature": -1.0,
  "wind_direction": 260,
  "wind_gusts_kts": 33,
  "wind_speed_kts": 26
}

< ... more results ... >
```
