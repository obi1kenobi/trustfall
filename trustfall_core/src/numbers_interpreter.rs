use std::{collections::BTreeSet, sync::Arc};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
    project_property,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NumbersToken {
    Neither(NeitherNumber), // zero and one
    Prime(PrimeNumber),
    Composite(CompositeNumber),
}

trait Number {
    fn typename(&self) -> &'static str;

    fn value(&self) -> i64;

    fn name(&self) -> Option<&'static str> {
        match self.value() {
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
pub(crate) struct NeitherNumber(i64);

impl Number for NeitherNumber {
    fn typename(&self) -> &'static str {
        "Neither"
    }

    fn value(&self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PrimeNumber(i64);

impl Number for PrimeNumber {
    fn typename(&self) -> &'static str {
        "Prime"
    }

    fn value(&self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CompositeNumber(i64, BTreeSet<i64>);

impl Number for CompositeNumber {
    fn typename(&self) -> &'static str {
        "Composite"
    }

    fn value(&self) -> i64 {
        self.0
    }
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
            let factors: BTreeSet<i64> = primes
                .iter()
                .copied()
                .filter(|prime| num % *prime == 0)
                .collect();
            factors
        }
        _ => unreachable!(),
    }
}

fn make_number_token(primes: &mut BTreeSet<i64>, num: i64) -> NumbersToken {
    if num >= 2 {
        generate_primes_up_to(primes, num);
    }
    let factors = get_factors(primes, num);
    match factors.len() {
        0 => NumbersToken::Neither(NeitherNumber(num)),
        1 if factors.contains(&num) => NumbersToken::Prime(PrimeNumber(num)),
        _ => NumbersToken::Composite(CompositeNumber(num, factors)),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NumbersAdapter;

#[allow(unused_variables)]
impl Adapter<'static> for NumbersAdapter {
    type DataToken = NumbersToken;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken>> {
        let mut primes = btreeset![2, 3];
        match edge.as_ref() {
            "Zero" => Box::new(std::iter::once(make_number_token(&mut primes, 0))),
            "One" => Box::new(std::iter::once(make_number_token(&mut primes, 1))),
            "Two" => Box::new(std::iter::once(make_number_token(&mut primes, 2))),
            "Four" => Box::new(std::iter::once(make_number_token(&mut primes, 4))),
            "Number" | "NumberImplicitNullDefault" => {
                let parameters = &parameters.unwrap().0;
                let min_value = parameters
                    .get("min")
                    .and_then(FieldValue::as_i64)
                    .unwrap_or(0);
                let max_value = parameters.get("max").and_then(FieldValue::as_i64).unwrap();

                if min_value > max_value {
                    Box::new(std::iter::empty())
                } else {
                    Box::new(
                        (min_value..=max_value)
                            .map(move |n| make_number_token(&mut primes, n))
                            .collect_vec()
                            .into_iter(),
                    )
                }
            }
            _ => unimplemented!("{}", edge),
        }
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)>> {
        project_property! {
            data_contexts, current_type_name, field_name, [
                {
                    Number | Prime | Composite,
                    NumbersToken::Neither | NumbersToken::Prime | NumbersToken::Composite, [
                        (value, token, {
                            FieldValue::Int64(token.value())
                        }),
                        (name, token, {
                            match token.name() {
                                None => FieldValue::Null,
                                Some(x) => FieldValue::String(x.to_string()),
                            }
                        }),
                        (vowelsInName, token, {
                            match token.vowels_in_name() {
                                None => FieldValue::Null,
                                Some(v) => FieldValue::List(v.into_iter().map(FieldValue::String).collect_vec()),
                            }
                        }),
                        (__typename, token, {
                            token.typename().into()
                        })
                    ],
                }
            ]
        }
    }

    #[allow(clippy::type_complexity)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
            Item = (
                DataContext<Self::DataToken>,
                Box<dyn Iterator<Item = Self::DataToken>>,
            ),
        >,
    > {
        let mut primes = btreeset![2, 3];
        match (edge_name.as_ref(), current_type_name.as_ref()) {
            ("predecessor", "Number" | "Prime" | "Composite") => {
                Box::new(data_contexts.map(move |ctx| {
                    let neighbors: Box<dyn Iterator<Item = Self::DataToken>> = match &ctx
                        .current_token
                    {
                        None => Box::new(std::iter::empty()),
                        Some(token) => {
                            let value = match &token {
                                NumbersToken::Neither(inner) => inner.value(),
                                NumbersToken::Prime(inner) => inner.value(),
                                NumbersToken::Composite(inner) => inner.value(),
                            };
                            if value > 0 {
                                Box::new(std::iter::once(make_number_token(&mut primes, value - 1)))
                            } else {
                                Box::new(std::iter::empty())
                            }
                        }
                    };

                    (ctx, neighbors)
                }))
            }
            ("successor", "Number" | "Prime" | "Composite") => {
                Box::new(data_contexts.map(move |ctx| {
                    let neighbors: Box<dyn Iterator<Item = NumbersToken>> = match &ctx.current_token
                    {
                        None => Box::new(std::iter::empty()),
                        Some(token) => {
                            let value = match &token {
                                NumbersToken::Neither(inner) => inner.value(),
                                NumbersToken::Prime(inner) => inner.value(),
                                NumbersToken::Composite(inner) => inner.value(),
                            };
                            Box::new(std::iter::once(make_number_token(&mut primes, value + 1)))
                        }
                    };

                    (ctx, neighbors)
                }))
            }
            ("multiple", "Number" | "Prime" | "Composite") => {
                Box::new(data_contexts.map(move |ctx| {
                    let neighbors: Box<dyn Iterator<Item = NumbersToken>> = match &ctx.current_token
                    {
                        None | Some(NumbersToken::Neither(..)) => Box::new(std::iter::empty()),
                        Some(NumbersToken::Prime(token)) => {
                            let value = token.0;
                            let mut local_primes = primes.clone();

                            let max_multiple =
                                parameters.as_ref().unwrap().0["max"].as_i64().unwrap();

                            // We're only outputting composite numbers only,
                            // and the initial number is prime.
                            let start_multiple = 2;

                            Box::new((start_multiple..=max_multiple).map(move |mult| {
                                let next_value = value * mult;
                                make_number_token(&mut local_primes, next_value)
                            }))
                        }
                        Some(NumbersToken::Composite(token)) => {
                            let value = token.0;
                            let mut local_primes = primes.clone();

                            let max_multiple =
                                parameters.as_ref().unwrap().0["max"].as_i64().unwrap();
                            Box::new((1..=max_multiple).map(move |mult| {
                                let next_value = value * mult;
                                make_number_token(&mut local_primes, next_value)
                            }))
                        }
                    };

                    (ctx, neighbors)
                }))
            }
            ("primeFactor", "Composite") => Box::new(data_contexts.map(move |ctx| {
                let neighbors: Box<dyn Iterator<Item = NumbersToken>> = match &ctx.current_token {
                    None => Box::new(std::iter::empty()),
                    Some(NumbersToken::Composite(token)) => {
                        let factors = &token.1;
                        Box::new(
                            factors
                                .iter()
                                .map(|n| make_number_token(&mut primes, *n))
                                .collect_vec()
                                .into_iter(),
                        )
                    }
                    _ => unreachable!("primeFactor Composite {:?}", ctx.current_token),
                };

                (ctx, neighbors)
            })),
            ("divisor", "Composite") => Box::new(data_contexts.map(move |ctx| {
                let neighbors: Box<dyn Iterator<Item = NumbersToken>> = match &ctx.current_token {
                    None => Box::new(std::iter::empty()),
                    Some(NumbersToken::Composite(token)) => {
                        let value = token.0;
                        if value <= 0 {
                            Box::new(std::iter::empty())
                        } else {
                            Box::new(
                                (1..value)
                                    .filter_map(|maybe_divisor| {
                                        if value % maybe_divisor == 0 {
                                            Some(make_number_token(&mut primes, maybe_divisor))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect_vec()
                                    .into_iter(),
                            )
                        }
                    }
                    _ => unreachable!("divisor Composite {:?}", ctx.current_token),
                };

                (ctx, neighbors)
            })),
            _ => {
                unreachable!(
                    "Unexpected edge {} on vertex type {}",
                    &edge_name, &current_type_name
                );
            }
        }
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)>> {
        match (current_type_name.as_ref(), coerce_to_type_name.as_ref()) {
            ("Number", "Prime") => Box::new(data_contexts.map(move |context| {
                let token = &context.current_token;
                let can_coerce = matches!(token, Some(NumbersToken::Prime(..)));
                (context, can_coerce)
            })),
            ("Number", "Composite") => Box::new(data_contexts.map(move |context| {
                let token = &context.current_token;
                let can_coerce = matches!(token, Some(NumbersToken::Composite(..)));
                (context, can_coerce)
            })),
            _ => unimplemented!(
                "Unexpected coercion attempted: {} {}",
                current_type_name,
                coerce_to_type_name
            ),
        }
    }
}
