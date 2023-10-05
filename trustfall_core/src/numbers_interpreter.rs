use std::{collections::BTreeSet, sync::Arc};

use itertools::Itertools;
use maplit::btreeset;
use serde::{Deserialize, Serialize};

use crate::{
    interpreter::{
        self,
        helpers::{resolve_coercion_with, resolve_neighbors_with, resolve_property_with},
        Adapter, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo, Typename,
        VertexIterator, AsVertex,
    },
    ir::{EdgeParameters, FieldValue},
    schema::Schema,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumbersVertex {
    Neither(NeitherNumber), // zero and one
    Prime(PrimeNumber),
    Composite(CompositeNumber),
    Letter(Letter),
}

impl Typename for NumbersVertex {
    fn typename(&self) -> &'static str {
        match self {
            NumbersVertex::Neither(x) => x.typename(),
            NumbersVertex::Prime(x) => x.typename(),
            NumbersVertex::Composite(x) => x.typename(),
            NumbersVertex::Letter(..) => "Letter",
        }
    }
}

fn number_name(number: i64) -> Option<&'static str> {
    match number {
        0 => Some("zero"),
        1 => Some("one"),
        2 => Some("two"),
        3 => Some("three"),
        4 => Some("four"),
        5 => Some("five"),
        6 => Some("six"),
        7 => Some("seven"),
        8 => Some("eight"),
        9 => Some("nine"),
        10 => Some("ten"),
        11 => Some("eleven"),
        12 => Some("twelve"),
        13 => Some("thirteen"),
        14 => Some("fourteen"),
        15 => Some("fifteen"),
        16 => Some("sixteen"),
        17 => Some("seventeen"),
        18 => Some("eighteen"),
        19 => Some("nineteen"),
        20 => Some("twenty"),
        _ => None,
    }
}

trait Number {
    fn typename(&self) -> &'static str;

    fn value(&self) -> i64;

    fn name(&self) -> Option<&str>;

    fn vowels_in_name(&self) -> Option<Vec<String>> {
        self.name().map(|name| {
            name.chars()
                .filter_map(|x| match x {
                    'a' | 'e' | 'i' | 'o' | 'u' => Some(x.to_string()),
                    _ => None,
                })
                .collect_vec()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeitherNumber(i64);

impl Number for NeitherNumber {
    fn typename(&self) -> &'static str {
        "Neither"
    }

    fn value(&self) -> i64 {
        self.0
    }

    fn name(&self) -> Option<&str> {
        number_name(self.value())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimeNumber(i64);

impl Number for PrimeNumber {
    fn typename(&self) -> &'static str {
        "Prime"
    }

    fn value(&self) -> i64 {
        self.0
    }

    fn name(&self) -> Option<&str> {
        number_name(self.value())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositeNumber(i64, BTreeSet<i64>);

impl Number for CompositeNumber {
    fn typename(&self) -> &'static str {
        "Composite"
    }

    fn value(&self) -> i64 {
        self.0
    }

    fn name(&self) -> Option<&str> {
        number_name(self.value())
    }
}

impl Number for NumbersVertex {
    fn typename(&self) -> &'static str {
        match self {
            NumbersVertex::Neither(x) => x.typename(),
            NumbersVertex::Prime(x) => x.typename(),
            NumbersVertex::Composite(x) => x.typename(),
            _ => unreachable!(),
        }
    }

    fn value(&self) -> i64 {
        match self {
            NumbersVertex::Neither(x) => x.value(),
            NumbersVertex::Prime(x) => x.value(),
            NumbersVertex::Composite(x) => x.value(),
            _ => unreachable!(),
        }
    }

    fn name(&self) -> Option<&str> {
        if let Self::Letter(l) = self {
            Some(&l.name)
        } else {
            let number = self.value();
            number_name(number)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Letter {
    name: String,
}

fn generate_primes_up_to(primes: &mut BTreeSet<i64>, max_bound: i64) {
    if max_bound < 2 {
        return;
    }
    primes.insert(2);
    primes.insert(3);

    let mut current_max = *primes.iter().last().unwrap();
    while current_max < max_bound {
        current_max += 2;
        let is_prime = primes.iter().all(|prime| current_max % *prime != 0);
        if is_prime {
            primes.insert(current_max);
        }
    }
}

fn get_factors(primes: &BTreeSet<i64>, num: i64) -> BTreeSet<i64> {
    match num {
        0 | 1 => Default::default(),
        x if x < 0 => {
            let mut pos_factors = get_factors(primes, -num);
            pos_factors.insert(-1);
            pos_factors
        }
        x if x >= 2 => {
            let factors: BTreeSet<i64> =
                primes.iter().copied().filter(|prime| num % *prime == 0).collect();
            factors
        }
        _ => unreachable!(),
    }
}

fn make_number_vertex(primes: &mut BTreeSet<i64>, num: i64) -> NumbersVertex {
    if num >= 2 {
        generate_primes_up_to(primes, num);
    }
    let factors = get_factors(primes, num);
    match factors.len() {
        0 => NumbersVertex::Neither(NeitherNumber(num)),
        1 if factors.contains(&num) => NumbersVertex::Prime(PrimeNumber(num)),
        _ => NumbersVertex::Composite(CompositeNumber(num, factors)),
    }
}

#[derive(Debug, Clone)]
pub struct NumbersAdapter {
    schema: Schema,
}

impl NumbersAdapter {
    pub fn new() -> Self {
        Self {
            schema: Schema::parse(include_str!(
                "../../trustfall_core/test_data/schemas/numbers.graphql"
            ))
            .expect("schema is not valid"),
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

impl Default for NumbersAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unused_variables)]
impl<'a> Adapter<'a> for NumbersAdapter {
    type Vertex = NumbersVertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
        let mut primes = btreeset![2, 3];
        match edge_name.as_ref() {
            "Zero" => Box::new(std::iter::once(make_number_vertex(&mut primes, 0))),
            "One" => Box::new(std::iter::once(make_number_vertex(&mut primes, 1))),
            "Two" => Box::new(std::iter::once(make_number_vertex(&mut primes, 2))),
            "Four" => Box::new(std::iter::once(make_number_vertex(&mut primes, 4))),
            "Number" | "NumberImplicitNullDefault" => {
                let min_value = parameters["min"].as_i64().unwrap_or(0);
                let max_value = parameters["max"].as_i64().unwrap();

                if min_value > max_value {
                    Box::new(std::iter::empty())
                } else {
                    Box::new(
                        (min_value..=max_value)
                            .map(move |n| make_number_vertex(&mut primes, n))
                            .collect_vec()
                            .into_iter(),
                    )
                }
            }
            _ => unimplemented!("{edge_name}"),
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex>>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, FieldValue> {
        if property_name.as_ref() == "__typename" {
            return interpreter::helpers::resolve_typename(contexts, &self.schema, type_name);
        }

        match (type_name.as_ref(), property_name.as_ref()) {
            ("Number" | "Prime" | "Composite" | "Neither", "value") => {
                resolve_property_with(contexts, |vertex| vertex.value().into())
            }
            ("Number" | "Prime" | "Composite" | "Neither" | "Named" | "Letter", "name") => {
                resolve_property_with(contexts, |vertex| vertex.name().into())
            }
            ("Number" | "Prime" | "Composite" | "Neither", "vowelsInName") => {
                resolve_property_with(contexts, |vertex| vertex.vowels_in_name().into())
            }
            (type_name, property_name) => {
                unreachable!("failed to resolve type {type_name} property {property_name}")
            }
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex>>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Self::Vertex>> {
        let mut primes = btreeset![2, 3];
        let parameters = parameters.clone();
        match (type_name.as_ref(), edge_name.as_ref()) {
            ("Number" | "Prime" | "Composite" | "Neither", "predecessor") => {
                resolve_neighbors_with(contexts, move |vertex| {
                    let value = match &vertex {
                        NumbersVertex::Neither(inner) => inner.value(),
                        NumbersVertex::Prime(inner) => inner.value(),
                        NumbersVertex::Composite(inner) => inner.value(),
                        _ => unreachable!("{vertex:?}"),
                    };
                    if value > 0 {
                        Box::new(std::iter::once(make_number_vertex(&mut primes, value - 1)))
                    } else {
                        Box::new(std::iter::empty())
                    }
                })
            }
            ("Number" | "Prime" | "Composite" | "Neither", "successor") => {
                resolve_neighbors_with(contexts, move |vertex| {
                    let value = match &vertex {
                        NumbersVertex::Neither(inner) => inner.value(),
                        NumbersVertex::Prime(inner) => inner.value(),
                        NumbersVertex::Composite(inner) => inner.value(),
                        _ => unreachable!("{vertex:?}"),
                    };
                    Box::new(std::iter::once(make_number_vertex(&mut primes, value + 1)))
                })
            }
            ("Number" | "Prime" | "Composite" | "Neither", "multiple") => {
                resolve_neighbors_with(contexts, move |vertex| {
                    match vertex {
                        NumbersVertex::Neither(..) => Box::new(std::iter::empty()),
                        NumbersVertex::Prime(vertex) => {
                            let value = vertex.0;
                            let mut local_primes = primes.clone();

                            let max_multiple = parameters["max"].as_i64().unwrap();

                            // We're only outputting composite numbers only,
                            // and the initial number is prime.
                            let start_multiple = 2;

                            Box::new((start_multiple..=max_multiple).map(move |mult| {
                                let next_value = value * mult;
                                make_number_vertex(&mut local_primes, next_value)
                            }))
                        }
                        NumbersVertex::Composite(vertex) => {
                            let value = vertex.0;
                            let mut local_primes = primes.clone();

                            let max_multiple = parameters["max"].as_i64().unwrap();
                            Box::new((1..=max_multiple).map(move |mult| {
                                let next_value = value * mult;
                                make_number_vertex(&mut local_primes, next_value)
                            }))
                        }
                        _ => unreachable!("{vertex:?}"),
                    }
                })
            }
            ("Composite", "primeFactor") => {
                resolve_neighbors_with(contexts, move |vertex| match vertex {
                    NumbersVertex::Composite(vertex) => {
                        let factors = &vertex.1;
                        Box::new(
                            factors
                                .iter()
                                .map(|n| make_number_vertex(&mut primes, *n))
                                .collect_vec()
                                .into_iter(),
                        )
                    }
                    _ => unreachable!("{vertex:?}"),
                })
            }
            ("Composite", "divisor") => {
                resolve_neighbors_with(contexts, move |vertex| match vertex {
                    NumbersVertex::Composite(vertex) => {
                        let value = vertex.0;
                        if value <= 0 {
                            Box::new(std::iter::empty())
                        } else {
                            Box::new(
                                (1..value)
                                    .filter_map(|maybe_divisor| {
                                        if value % maybe_divisor == 0 {
                                            Some(make_number_vertex(&mut primes, maybe_divisor))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect_vec()
                                    .into_iter(),
                            )
                        }
                    }
                    _ => unreachable!("{vertex:?}"),
                })
            }
            _ => {
                unreachable!("Unexpected edge {} on vertex type {}", &edge_name, &type_name);
            }
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex>>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, bool> {
        match (type_name.as_ref(), coerce_to_type.as_ref()) {
            ("Number" | "Named", "Prime") => {
                resolve_coercion_with(contexts, |vertex| matches!(vertex, NumbersVertex::Prime(..)))
            }
            ("Number" | "Named", "Composite") => resolve_coercion_with(contexts, |vertex| {
                matches!(vertex, NumbersVertex::Composite(..))
            }),
            ("Number" | "Named", "Neither") => resolve_coercion_with(contexts, |vertex| {
                matches!(vertex, NumbersVertex::Composite(..))
            }),
            ("Named", "Letter") => resolve_coercion_with(contexts, |vertex| {
                matches!(vertex, NumbersVertex::Letter(..))
            }),
            ("Named", "Number") => resolve_coercion_with(contexts, |vertex| {
                matches!(
                    vertex,
                    NumbersVertex::Prime(..)
                        | NumbersVertex::Composite(..)
                        | NumbersVertex::Neither(..)
                )
            }),
            _ => unimplemented!("Unexpected coercion attempted: {} {}", type_name, coerce_to_type),
        }
    }
}
