use std::fmt;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{de::Visitor, Deserialize, Deserializer};

#[allow(dead_code)]
#[allow(non_snake_case)] // names match the official naming scheme, should use serde rename instead
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CsvMetarReport {
    pub(crate) raw_metar: String,
    pub(crate) station_id: String,

    pub(crate) observation_time: DateTime<Utc>,

    pub(crate) latitude: Option<f64>,
    pub(crate) longitude: Option<f64>,

    pub(crate) temp_c: Option<f64>,
    pub(crate) dewpoint_c: Option<f64>,

    pub(crate) wind_dir_degrees: Option<u16>,
    pub(crate) wind_speed_kt: Option<u16>,
    pub(crate) wind_gust_kt: Option<u16>,

    pub(crate) visibility_statute_mi: Option<f64>,

    pub(crate) altim_in_hg: Option<f64>,
    pub(crate) sea_level_pressure_mb: Option<f64>,

    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) corrected: Option<bool>,
    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) auto: Option<bool>,
    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) auto_station: Option<bool>,
    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) maintenance_indicator_on: Option<bool>,
    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) no_signal: Option<bool>,
    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) lightning_sensor_off: Option<bool>,
    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) freezing_rain_sensor_off: Option<bool>,
    #[serde(deserialize_with = "metar_option_bool_deserializer")]
    pub(crate) present_weather_sensor_off: Option<bool>,

    pub(crate) wx_string: Option<String>,

    // these four pairs of fields have identical names "sky_cover" and "cloud_base_ft_agl"
    // just repeated four times in the CSV file
    pub(crate) sky_cover_1: Option<String>,
    pub(crate) cloud_base_ft_agl_1: Option<i32>,

    pub(crate) sky_cover_2: Option<String>,
    pub(crate) cloud_base_ft_agl_2: Option<i32>,

    pub(crate) sky_cover_3: Option<String>,
    pub(crate) cloud_base_ft_agl_3: Option<i32>,

    pub(crate) sky_cover_4: Option<String>,
    pub(crate) cloud_base_ft_agl_4: Option<i32>,

    pub(crate) flight_category: Option<String>,

    pub(crate) three_hr_pressure_tendency_mb: Option<f64>,
    pub(crate) maxT_c: Option<f64>,
    pub(crate) minT_c: Option<f64>,
    pub(crate) maxT24hr_c: Option<f64>,
    pub(crate) minT24hr_c: Option<f64>,

    pub(crate) precip_in: Option<f64>,
    pub(crate) pcp3hr_in: Option<f64>,
    pub(crate) pcp6hr_in: Option<f64>,
    pub(crate) pcp24hr_in: Option<f64>,
    pub(crate) snow_in: Option<f64>,

    pub(crate) vert_vis_ft: Option<i32>,

    pub(crate) metar_type: Option<String>,
    pub(crate) elevation_m: Option<f64>,
}

pub(crate) fn metar_option_bool_deserializer<'de, D>(
    deserializer: D,
) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TypeDeserializer;

    impl<'de> Visitor<'de> for TypeDeserializer {
        type Value = Option<bool>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("boolean value, in any case")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if s.is_empty() {
                Ok(None)
            } else {
                s.to_lowercase()
                    .parse()
                    .map(Some)
                    .map_err(|_| serde::de::Error::custom("not a valid boolean value"))
            }
        }
    }

    deserializer.deserialize_str(TypeDeserializer)
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct MetarReport {
    pub(crate) station_id: String,
    pub(crate) raw_report: String,
    pub(crate) observation_time: DateTime<Utc>,

    pub(crate) latitude: Option<f64>,
    pub(crate) longitude: Option<f64>,

    pub(crate) wind_speed_kts: Option<u16>,
    pub(crate) wind_direction: Option<u16>,
    pub(crate) wind_gusts_kts: Option<u16>,

    pub(crate) temperature: Option<f64>,
    pub(crate) dewpoint: Option<f64>,

    pub(crate) visibility_unlimited: Option<bool>,
    pub(crate) visibility_minimal: Option<bool>,
    pub(crate) visibility_statute_mi: Option<f64>,

    pub(crate) altimeter_in_hg: Option<f64>,
    pub(crate) sea_level_pressure_mb: Option<f64>,
    pub(crate) wx_string: Option<String>, // TODO: do better here, this is where all the weather symbols are

    pub(crate) cloud_cover: Vec<MetarCloudCover>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct MetarCloudCover {
    pub(crate) sky_cover: String,
    pub(crate) base_altitude: Option<i32>,
}

impl From<CsvMetarReport> for MetarReport {
    fn from(csv_metar: CsvMetarReport) -> Self {
        let mut cloud_cover: Vec<MetarCloudCover> = vec![];
        if let Some(sky_cover) = csv_metar.sky_cover_1 {
            cloud_cover.push(MetarCloudCover {
                sky_cover,
                base_altitude: csv_metar.cloud_base_ft_agl_1,
            })
        }
        if let Some(sky_cover) = csv_metar.sky_cover_2 {
            cloud_cover.push(MetarCloudCover {
                sky_cover,
                base_altitude: csv_metar.cloud_base_ft_agl_2,
            })
        }
        if let Some(sky_cover) = csv_metar.sky_cover_3 {
            cloud_cover.push(MetarCloudCover {
                sky_cover,
                base_altitude: csv_metar.cloud_base_ft_agl_3,
            })
        }
        if let Some(sky_cover) = csv_metar.sky_cover_4 {
            cloud_cover.push(MetarCloudCover {
                sky_cover,
                base_altitude: csv_metar.cloud_base_ft_agl_4,
            })
        }

        let visibility = get_visibility(&csv_metar.raw_metar);
        let visibility_statute_mi = match visibility {
            Visibility::StatuteMiles(visibility_mi) => Some(visibility_mi),
            Visibility::Minimal | Visibility::Unlimited => None,
            Visibility::Unknown => csv_metar.visibility_statute_mi,
        };
        let (visibility_unlimited, visibility_minimal) = match visibility {
            Visibility::StatuteMiles(_) => (Some(false), Some(false)),
            Visibility::Unlimited => (Some(true), Some(false)),
            Visibility::Minimal => (Some(false), Some(true)),
            Visibility::Unknown => (None, None),
        };

        Self {
            station_id: csv_metar.station_id,
            raw_report: csv_metar.raw_metar,
            observation_time: csv_metar.observation_time,
            latitude: csv_metar.latitude,
            longitude: csv_metar.longitude,
            wind_speed_kts: csv_metar.wind_speed_kt,
            wind_direction: csv_metar.wind_dir_degrees,
            wind_gusts_kts: csv_metar.wind_gust_kt,
            temperature: csv_metar.temp_c,
            dewpoint: csv_metar.dewpoint_c,
            visibility_unlimited,
            visibility_minimal,
            visibility_statute_mi,
            altimeter_in_hg: csv_metar.altim_in_hg,
            sea_level_pressure_mb: csv_metar.sea_level_pressure_mb,
            wx_string: csv_metar.wx_string,
            cloud_cover,
        }
    }
}

static METAR_STATION_AND_DATE_PATTERN: &str = r"[A-Z]{4} \d{6}Z ";
static METAR_AUTO_OPTIONAL_MARKER_PATTERN: &str = r"(?:AUTO )?";

//                                     |   direction   |intensity|    gusts    |    units    | unknown |
static METAR_WIND_PATTERN: &str =
    r"(?:(?:VRB|[0-9/]{3})[0-9/]{2}(?:G[0-9/]{2})(?:MPS|KPH|KT))|(?://///) ";

// see Surface Wind: http://www.bom.gov.au/aviation/data/education/metar-speci.pdf
static METAR_WIND_VARIABILITY_PATTERN: &str = r"(?:[0-9/]{3}V[0-9/]{3} )?";

//                                        |raw meters|vis.ok| statute mi |
static METAR_VISIBILITY_PATTERN: &str = r"(?:[0-9]{4}|CAVOK|(?:[0-9/ ]+SM)) ";

lazy_static! {
    static ref METAR_VISIBILITY_CAPTURE_PATTERN: String = METAR_STATION_AND_DATE_PATTERN.to_owned()
        + METAR_AUTO_OPTIONAL_MARKER_PATTERN
        + "(?:"
        + METAR_WIND_PATTERN
        + ")?"
        + METAR_WIND_VARIABILITY_PATTERN
        + "("
        + METAR_VISIBILITY_PATTERN
        + ")";
    static ref METAR_VISIBILITY_CAPTURE_RE: Regex =
        Regex::new(&METAR_VISIBILITY_CAPTURE_PATTERN).unwrap();
}

fn get_visibility(raw_metar: &str) -> Visibility {
    if let Some(capture) = METAR_VISIBILITY_CAPTURE_RE.captures(raw_metar) {
        match capture[1].trim() {
            "0000" => Visibility::Minimal,
            "9999" | "CAVOK" => Visibility::Unlimited,
            "////" | "////SM" => Visibility::Unknown,
            visibility_sm if visibility_sm.ends_with("SM") => {
                let visibility_amount = visibility_sm.strip_suffix("SM").unwrap();

                if let Some((integer_mi, fractional_mi)) = visibility_amount.split_once(' ') {
                    let integer_mi: f64 = integer_mi.parse::<u32>().unwrap().into();
                    if let Some((numerator, denominator)) = fractional_mi.split_once('/') {
                        let numerator: f64 = numerator.parse().unwrap();
                        let denominator: f64 = denominator.parse().unwrap();

                        let total_mi: f64 = integer_mi + (numerator / denominator);
                        Visibility::StatuteMiles(total_mi)
                    } else {
                        unreachable!()
                    }
                } else if let Some((numerator, denominator)) = visibility_amount.split_once('/') {
                    let numerator: f64 = numerator.parse().unwrap();
                    let denominator: f64 = denominator.parse().unwrap();
                    Visibility::StatuteMiles(numerator / denominator)
                } else {
                    let integer_mi: f64 = visibility_amount.parse().unwrap();
                    Visibility::StatuteMiles(integer_mi)
                }
            }
            visibility_meters
                if visibility_meters.len() == 4
                    && visibility_meters.chars().all(|c| c.is_ascii_digit()) =>
            {
                let meters_per_mile: f64 = 1609.34;
                let visibility_meters: f64 = visibility_meters.parse::<u32>().unwrap().into();
                Visibility::StatuteMiles(visibility_meters / meters_per_mile)
            }
            vis => {
                unreachable!("{vis}")
            }
        }
    } else {
        Visibility::Unknown
    }
}

enum Visibility {
    // examples of each:
    StatuteMiles(f64), // 5SM
    Unlimited,         // 9999
    Minimal,           // 0000
    Unknown,           // missing entirely / not found
}
