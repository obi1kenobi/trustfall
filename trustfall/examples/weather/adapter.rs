use trustfall::{
    provider::{
        BasicAdapter, ContextIterator, ContextOutcomeIterator, EdgeParameters, TrustfallEnumVertex,
        VertexIterator,
    },
    FieldValue,
};

use crate::metar::{MetarCloudCover, MetarReport};

#[derive(Debug, Clone)]
pub(crate) struct MetarAdapter<'a> {
    data: &'a [MetarReport],
}

impl<'a> MetarAdapter<'a> {
    pub(crate) fn new(data: &'a [MetarReport]) -> Self {
        Self { data }
    }
}

#[derive(Debug, Clone, Copy, TrustfallEnumVertex)]
pub(crate) enum Vertex<'a> {
    MetarReport(&'a MetarReport),
    CloudCover(&'a MetarCloudCover),
}

impl<'a> From<&'a MetarReport> for Vertex<'a> {
    fn from(v: &'a MetarReport) -> Self {
        Self::MetarReport(v)
    }
}

impl<'a> From<&'a MetarCloudCover> for Vertex<'a> {
    fn from(v: &'a MetarCloudCover) -> Self {
        Self::CloudCover(v)
    }
}

macro_rules! non_float_field {
    ($iter: ident, $variant: path, $field: ident) => {
        Box::new($iter.map(|ctx| {
            let value = match ctx.active_vertex() {
                None => FieldValue::Null,
                Some(vertex) => match vertex {
                    $variant(m) => m.$field.clone().into(),
                    _ => unreachable!(),
                },
            };
            (ctx, value)
        }))
    };
}

macro_rules! float_field {
    ($iter: ident, $variant: path, $field: ident) => {
        Box::new($iter.map(|ctx| {
            let value = match ctx.active_vertex() {
                None => FieldValue::Null,
                Some(vertex) => match vertex {
                    $variant(m) => m.$field.clone().try_into().unwrap(),
                    _ => unreachable!(),
                },
            };
            (ctx, value)
        }))
    };
}

impl<'a> BasicAdapter<'a> for MetarAdapter<'a> {
    type Vertex = Vertex<'a>;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            "MetarReport" => Box::new(self.data.iter().map(|x| x.into())),
            "LatestMetarReportForAirport" => {
                let station_code = parameters["airport_code"].as_str().unwrap().to_string();
                let iter = self
                    .data
                    .iter()
                    .filter(move |&x| x.station_id == station_code)
                    .map(|x| x.into());
                Box::new(iter)
            }
            _ => unreachable!(),
        }
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
        match type_name {
            "MetarReport" => {
                match property_name {
                    // TODO: implement __typename
                    "stationId" => non_float_field!(contexts, Vertex::MetarReport, station_id),
                    "rawReport" => non_float_field!(contexts, Vertex::MetarReport, raw_report),
                    "observationTime" => {
                        non_float_field!(contexts, Vertex::MetarReport, observation_time)
                    }
                    "latitude" => float_field!(contexts, Vertex::MetarReport, latitude),
                    "longitude" => float_field!(contexts, Vertex::MetarReport, longitude),
                    "windSpeedKts" => {
                        non_float_field!(contexts, Vertex::MetarReport, wind_speed_kts)
                    }
                    "windDirection" => {
                        non_float_field!(contexts, Vertex::MetarReport, wind_direction)
                    }
                    "windGustsKts" => {
                        non_float_field!(contexts, Vertex::MetarReport, wind_gusts_kts)
                    }
                    "temperature" => float_field!(contexts, Vertex::MetarReport, temperature),
                    "dewpoint" => float_field!(contexts, Vertex::MetarReport, dewpoint),
                    "visibilityUnlimited" => {
                        non_float_field!(contexts, Vertex::MetarReport, visibility_unlimited)
                    }
                    "visibilityMinimal" => {
                        non_float_field!(contexts, Vertex::MetarReport, visibility_minimal)
                    }
                    "visibilityStatuteMi" => {
                        float_field!(contexts, Vertex::MetarReport, visibility_statute_mi)
                    }
                    "altimeterInHg" => {
                        float_field!(contexts, Vertex::MetarReport, altimeter_in_hg)
                    }
                    "seaLevelPressureMb" => {
                        float_field!(contexts, Vertex::MetarReport, sea_level_pressure_mb)
                    }
                    unknown_field_name => unreachable!("{}", unknown_field_name),
                }
            }
            "MetarCloudCover" => match property_name {
                "skyCover" => non_float_field!(contexts, Vertex::CloudCover, sky_cover),
                "baseAltitude" => {
                    non_float_field!(contexts, Vertex::CloudCover, base_altitude)
                }
                unknown_field_name => unreachable!("{}", unknown_field_name),
            },
            _ => unreachable!(),
        }
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
        match (type_name, edge_name) {
            ("MetarReport", "cloudCover") => {
                assert!(parameters.is_empty());

                Box::new(contexts.map(|ctx| {
                    let neighbors: VertexIterator<'a, Self::Vertex> = match ctx.active_vertex() {
                        Some(vertex) => match vertex {
                            &Vertex::MetarReport(metar) => {
                                Box::new(metar.cloud_cover.iter().map(|c| c.into()))
                            }
                            _ => unreachable!(),
                        },
                        None => Box::new(std::iter::empty()),
                    };
                    (ctx, neighbors)
                }))
            }
            _ => unreachable!(),
        }
    }

    #[allow(unused_variables)]
    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        unimplemented!("no types in our schema have subtypes")
    }
}
