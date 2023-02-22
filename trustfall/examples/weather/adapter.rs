use trustfall::{
    provider::{
        field_property, resolve_neighbors_with, resolve_property_with, BasicAdapter,
        ContextIterator, ContextOutcomeIterator, EdgeParameters, TrustfallEnumVertex,
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
            "MetarReport" => match property_name {
                "station_id" => {
                    resolve_property_with(contexts, field_property!(as_metar_report, station_id))
                }
                "raw_report" => {
                    resolve_property_with(contexts, field_property!(as_metar_report, raw_report))
                }
                "observation_time" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, observation_time),
                ),
                "latitude" => {
                    resolve_property_with(contexts, field_property!(as_metar_report, latitude))
                }
                "longitude" => {
                    resolve_property_with(contexts, field_property!(as_metar_report, longitude))
                }
                "wind_speed_kts" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, wind_speed_kts),
                ),
                "wind_direction" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, wind_direction),
                ),
                "wind_gusts_kts" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, wind_gusts_kts),
                ),
                "temperature" => {
                    resolve_property_with(contexts, field_property!(as_metar_report, temperature))
                }
                "dewpoint" => {
                    resolve_property_with(contexts, field_property!(as_metar_report, dewpoint))
                }
                "visibility_unlimited" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, visibility_unlimited),
                ),
                "visibility_minimal" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, visibility_minimal),
                ),
                "visibility_statute_mi" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, visibility_statute_mi),
                ),
                "altimeter_in_hg" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, altimeter_in_hg),
                ),
                "sea_level_pressure_mb" => resolve_property_with(
                    contexts,
                    field_property!(as_metar_report, sea_level_pressure_mb),
                ),
                unknown_field_name => unreachable!("unknown field name: {unknown_field_name}"),
            },
            "MetarCloudCover" => match property_name {
                "sky_cover" => {
                    resolve_property_with(contexts, field_property!(as_cloud_cover, sky_cover))
                }
                "base_altitude" => {
                    resolve_property_with(contexts, field_property!(as_cloud_cover, base_altitude))
                }
                unknown_field_name => unreachable!("unknown field name: {unknown_field_name}"),
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
            ("MetarReport", "cloud_cover") => {
                assert!(parameters.is_empty());
                resolve_neighbors_with(contexts, |vertex| {
                    let neighbors = vertex
                        .as_metar_report()
                        .expect("not a MetarReport vertex")
                        .cloud_cover
                        .iter()
                        .map(|c| c.into());
                    Box::new(neighbors)
                })
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
