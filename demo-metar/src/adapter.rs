use std::{iter, sync::Arc};

use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
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

#[derive(Debug, Clone, Copy)]
pub(crate) enum Token<'a> {
    MetarReport(&'a MetarReport),
    CloudCover(&'a MetarCloudCover),
}

impl<'a> From<&'a MetarReport> for Token<'a> {
    fn from(v: &'a MetarReport) -> Self {
        Self::MetarReport(v)
    }
}

impl<'a> From<&'a MetarCloudCover> for Token<'a> {
    fn from(v: &'a MetarCloudCover) -> Self {
        Self::CloudCover(v)
    }
}

macro_rules! non_float_field {
    ($iter: ident, $variant: path, $field: ident) => {
        Box::new($iter.map(|ctx| {
            let value = match &ctx.current_token {
                None => FieldValue::Null,
                Some(token) => match token {
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
            let value = match &ctx.current_token {
                None => FieldValue::Null,
                Some(token) => match token {
                    $variant(m) => m.$field.clone().try_into().unwrap(),
                    _ => unreachable!(),
                },
            };
            (ctx, value)
        }))
    };
}

impl<'a> Adapter<'a> for MetarAdapter<'a> {
    type DataToken = Token<'a>;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'a> {
        match edge.as_ref() {
            "MetarReport" => Box::new(self.data.iter().map(|x| x.into())),
            "LatestMetarReportForAirport" => {
                let station_code = match parameters
                    .as_ref()
                    .unwrap()
                    .0
                    .get("airport_code")
                    .unwrap()
                    .clone()
                {
                    FieldValue::String(s) => s,
                    _ => unreachable!(),
                };
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

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)> + 'a> {
        match current_type_name.as_ref() {
            "MetarReport" => {
                match field_name.as_ref() {
                    // TODO: implement __typename
                    "stationId" => non_float_field!(data_contexts, Token::MetarReport, station_id),
                    "rawReport" => non_float_field!(data_contexts, Token::MetarReport, raw_report),
                    "observationTime" => {
                        non_float_field!(data_contexts, Token::MetarReport, observation_time)
                    }
                    "latitude" => float_field!(data_contexts, Token::MetarReport, latitude),
                    "longitude" => float_field!(data_contexts, Token::MetarReport, longitude),
                    "windSpeedKts" => {
                        non_float_field!(data_contexts, Token::MetarReport, wind_speed_kts)
                    }
                    "windDirection" => {
                        non_float_field!(data_contexts, Token::MetarReport, wind_direction)
                    }
                    "windGustsKts" => {
                        non_float_field!(data_contexts, Token::MetarReport, wind_gusts_kts)
                    }
                    "temperature" => float_field!(data_contexts, Token::MetarReport, temperature),
                    "dewpoint" => float_field!(data_contexts, Token::MetarReport, dewpoint),
                    "visibilityUnlimited" => {
                        non_float_field!(data_contexts, Token::MetarReport, visibility_unlimited)
                    }
                    "visibilityMinimal" => {
                        non_float_field!(data_contexts, Token::MetarReport, visibility_minimal)
                    }
                    "visibilityStatuteMi" => {
                        float_field!(data_contexts, Token::MetarReport, visibility_statute_mi)
                    }
                    "altimeterInHg" => {
                        float_field!(data_contexts, Token::MetarReport, altimeter_in_hg)
                    }
                    "seaLevelPressureMb" => {
                        float_field!(data_contexts, Token::MetarReport, sea_level_pressure_mb)
                    }
                    unknown_field_name => unreachable!("{}", unknown_field_name),
                }
            }
            "MetarCloudCover" => {
                match field_name.as_ref() {
                    // TODO: implement __typename
                    "skyCover" => non_float_field!(data_contexts, Token::CloudCover, sky_cover),
                    "baseAltitude" => {
                        non_float_field!(data_contexts, Token::CloudCover, base_altitude)
                    }
                    unknown_field_name => unreachable!("{}", unknown_field_name),
                }
            }
            _ => unreachable!(),
        }
    }

    #[allow(clippy::type_complexity)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
        _edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
                Item = (
                    DataContext<Self::DataToken>,
                    Box<dyn Iterator<Item = Self::DataToken> + 'a>,
                ),
            > + 'a,
    > {
        match (current_type_name.as_ref(), edge_name.as_ref()) {
            ("MetarReport", "cloudCover") => {
                assert!(parameters.is_none());

                Box::new(data_contexts.map(|ctx| {
                    let neighbors: Box<dyn Iterator<Item = Self::DataToken> + 'a> =
                        match &ctx.current_token {
                            Some(token) => match token {
                                &Token::MetarReport(metar) => {
                                    Box::new(metar.cloud_cover.iter().map(|c| c.into()))
                                }
                                _ => unreachable!(),
                            },
                            None => Box::new(iter::empty()),
                        };
                    (ctx, neighbors)
                }))
            }
            _ => unreachable!(),
        }
    }

    #[allow(unused_variables)]
    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'a>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'a> {
        todo!()
    }
}
