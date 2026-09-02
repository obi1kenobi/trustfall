//! Differential tests: the async and synchronous routes through the shared kernel must produce
//! byte-for-byte identical results for numbers-schema queries, including `@filter`, `@fold`, and
//! `@recurse`.
//!
//! The streaming [`SyncToAsyncAdapter`] bridge exposes the sync [`NumbersAdapter`] as an
//! [`AsyncAdapter`] without batch-collecting context streams. Results — not laziness — are what
//! must match. **Any panic is a test failure** (not treated as an unimplemented gap).

use std::{collections::BTreeMap, panic::AssertUnwindSafe, sync::Arc};

use futures_util::StreamExt;

use crate::{
    frontend::parse,
    interpreter::{async_test_adapter::SyncToAsyncAdapter, execution::interpret_ir},
    ir::FieldValue,
    numbers_interpreter::NumbersAdapter,
};

use super::engine::interpret_ir as interpret_ir_async;

type Row = BTreeMap<Arc<str>, FieldValue>;

fn sync_results(query: &str) -> Vec<Row> {
    let adapter = Arc::new(NumbersAdapter::new());
    let schema = adapter.schema().clone();
    let indexed = parse(&schema, query).expect("query failed to parse");
    interpret_ir(adapter, indexed, Arc::new(BTreeMap::new()))
        .expect("unexpected arguments error")
        .map(|row| row.expect("infallible adapter"))
        .collect()
}

fn async_results(query: &str) -> Vec<Row> {
    let numbers = NumbersAdapter::new();
    let schema = numbers.schema().clone();
    let adapter = Arc::new(SyncToAsyncAdapter::new(Arc::new(numbers)));
    let indexed = parse(&schema, query).expect("query failed to parse");
    let stream = interpret_ir_async(adapter, indexed, Arc::new(BTreeMap::new()))
        .expect("unexpected arguments error");
    futures_executor::block_on(async {
        stream.map(|row| row.expect("infallible adapter")).collect::<Vec<_>>().await
    })
}

/// Assert the async route matches the sync route on `query`, and that the result is non-empty
/// (so the test actually exercises the pipeline rather than trivially passing on no rows).
fn assert_matches_sync(query: &str) {
    let expected = sync_results(query);
    let actual = async_results(query);
    assert_eq!(expected, actual, "async/sync divergence for query:\n{query}");
    assert!(!expected.is_empty(), "query produced no rows, test is vacuous:\n{query}");
}

#[test]
fn flat_single_output() {
    assert_matches_sync(r#"{ Number(min: 0, max: 10) { value @output } }"#);
}

#[test]
fn flat_multiple_outputs() {
    assert_matches_sync(r#"{ Number(min: 0, max: 8) { value @output name @output } }"#);
}

#[test]
fn single_starting_vertex() {
    assert_matches_sync(r#"{ Two { value @output name @output } }"#);
}

#[test]
fn nested_edge() {
    assert_matches_sync(
        r#"{ Number(min: 0, max: 6) { value @output successor { s: value @output } } }"#,
    );
}

#[test]
fn deeply_nested_edges() {
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 4) {
                value @output
                successor {
                    v2: value @output
                    successor {
                        v3: value @output
                    }
                }
            }
        }"#,
    );
}

#[test]
fn optional_edge_present_and_absent() {
    // `predecessor` is absent for 0 (its predecessor is -1, which is outside the modeled range in
    // some cases), exercising both the present and nonexistent-optional paths.
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 6) {
                value @output
                predecessor @optional {
                    p: value @output
                }
            }
        }"#,
    );
}

#[test]
fn coercion_on_nested_edge() {
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 10) {
                value @output
                successor {
                    ... on Prime {
                        prime: value @output
                    }
                }
            }
        }"#,
    );
}

#[test]
fn coercion_and_optional_combined() {
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 10) {
                value @output
                predecessor @optional {
                    ... on Composite {
                        comp: value @output
                    }
                }
            }
        }"#,
    );
}

// ---------------------------------------------------------------------------
// CORPUS DIFFERENTIAL TEST
// ---------------------------------------------------------------------------
//
// Sweeps every `test_data/tests/valid_queries/*.graphql.ron` file and
// categorises each query as:
//   MATCH   - sync and async produced identical results
//   DIVERGE - the results differ (real engine bug; test FAILS)
//   PANIC   - the async engine panicked (real engine bug; test FAILS)
//
// Panics are never treated as an "unimplemented" success. Feature coverage
// (filter/fold/recurse) is detected from IR/query text for reporting only.
// ---------------------------------------------------------------------------

/// Helper: run the synchronous route for a numbers query with the given arguments.
fn sync_results_with_args(query: &str, arguments: Arc<BTreeMap<Arc<str>, FieldValue>>) -> Vec<Row> {
    let adapter = Arc::new(NumbersAdapter::new());
    let schema = adapter.schema().clone();
    let indexed = parse(&schema, query).expect("query failed to parse");
    interpret_ir(adapter, indexed, arguments)
        .expect("unexpected arguments error")
        .map(|row| row.expect("infallible adapter"))
        .collect()
}

fn format_panic_payload(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

/// Run the async route; any panic during construction *or* collection is returned as `Err`.
fn async_results_with_args(
    query: &str,
    arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
) -> Result<Vec<Row>, String> {
    let numbers = NumbersAdapter::new();
    let schema = numbers.schema().clone();
    let adapter = Arc::new(SyncToAsyncAdapter::new(Arc::new(numbers)));
    let indexed = parse(&schema, query).expect("query failed to parse");

    let stream_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        interpret_ir_async(adapter, indexed, arguments).expect("unexpected arguments error")
    }));

    match stream_result {
        Err(payload) => Err(format!(
            "panic during async pipeline construction: {}",
            format_panic_payload(payload)
        )),
        Ok(stream) => {
            let collect_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                futures_executor::block_on(async {
                    stream.map(|row| row.expect("infallible adapter")).collect::<Vec<_>>().await
                })
            }));
            match collect_result {
                Ok(rows) => Ok(rows),
                Err(payload) => Err(format!(
                    "panic while consuming async result stream: {}",
                    format_panic_payload(payload)
                )),
            }
        }
    }
}

/// Features present in the query text (for reporting only — never used to green-pass panics).
fn query_features(query: &str) -> Vec<&'static str> {
    let mut features = vec![];
    if query.contains("@filter") {
        features.push("@filter");
    }
    if query.contains("@fold") {
        features.push("@fold");
    }
    if query.contains("@recurse") {
        features.push("@recurse");
    }
    if query.contains("@optional") {
        features.push("@optional");
    }
    if query.contains("@tag") {
        features.push("@tag");
    }
    features
}

#[test]
fn corpus_differential_sweep() {
    use std::fs;

    // Locate the test_data directory relative to the crate manifest.
    let base =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data/tests/valid_queries");

    let mut entries: Vec<_> = fs::read_dir(&base)
        .expect("could not read valid_queries directory")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|x| x == "ron").unwrap_or(false)
                && e.path().to_str().map(|s| s.ends_with(".graphql.ron")).unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.path());

    let mut match_count = 0usize;
    let mut divergences: Vec<(String, String, Vec<Row>, Vec<Row>)> = vec![];
    let mut panics: Vec<(String, String, String, Vec<&'static str>)> = vec![];

    // Suppress panic output during the sweep so the log stays clean; panics still fail the test.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    for entry in &entries {
        let path = entry.path();
        let contents = fs::read_to_string(&path).expect("could not read file");
        let test_query: crate::test_types::TestGraphQLQuery =
            ron::from_str(&contents).expect("could not parse TestGraphQLQuery");

        // All corpus queries use the "numbers" schema.
        assert_eq!(test_query.schema_name, "numbers", "unexpected schema in {:?}", path);

        let arguments: Arc<BTreeMap<Arc<str>, FieldValue>> = Arc::new(
            test_query.arguments.into_iter().map(|(k, v)| (Arc::from(k.as_str()), v)).collect(),
        );

        let query = test_query.query.clone();
        let stem =
            path.file_name().unwrap().to_str().unwrap().trim_end_matches(".graphql.ron").to_owned();

        match async_results_with_args(&query, arguments.clone()) {
            Err(msg) => {
                panics.push((stem, query.clone(), msg, query_features(&query)));
            }
            Ok(actual) => {
                let expected = sync_results_with_args(&query, arguments);
                if expected == actual {
                    match_count += 1;
                } else {
                    divergences.push((stem, query.clone(), expected, actual));
                }
            }
        }
    }

    // Restore original panic hook.
    std::panic::set_hook(original_hook);

    let total = entries.len();
    println!(
        "\n=== Async vs Sync Corpus Differential Sweep ===\n\
         Total queries : {total}\n\
         MATCH         : {match_count}\n\
         DIVERGE       : {}\n\
         PANIC         : {}\n",
        divergences.len(),
        panics.len()
    );

    let mut msg = String::new();
    if !divergences.is_empty() {
        msg.push_str(&format!(
            "\n\nFAILURE: {} query/queries diverged between async and sync routes!\n\n",
            divergences.len()
        ));
        for (stem, query, expected, actual) in &divergences {
            msg.push_str(&format!(
                "--- DIVERGE: {stem} ---\n\
                 Schema: numbers\n\
                 Query:\n{query}\n\
                 Expected ({} rows):\n{expected:#?}\n\
                 Actual   ({} rows):\n{actual:#?}\n\n",
                expected.len(),
                actual.len()
            ));
        }
    }
    if !panics.is_empty() {
        msg.push_str(&format!(
            "\n\nFAILURE: {} query/queries panicked on the async route!\n\n",
            panics.len()
        ));
        for (stem, query, panic_msg, features) in &panics {
            msg.push_str(&format!(
                "--- PANIC: {stem} ---\n\
                 Features (from query text): {features:?}\n\
                 Panic: {panic_msg}\n\
                 Query:\n{query}\n\n"
            ));
        }
    }
    if !msg.is_empty() {
        panic!("{msg}");
    }
}

// ---------------------------------------------------------------------------
// ADVERSARIAL HAND-WRITTEN EDGE CASES
// ---------------------------------------------------------------------------
//
// Each test targets a specific structural property of the async pipeline.
// All use `assert_matches_sync` (or the zero-rows variant below where
// an empty result is the *correct* answer).
//
// `assert_matches_sync` asserts both equality AND non-emptiness.
// For queries that are *supposed* to return zero rows, we use
// `assert_empty_matches_sync` which asserts equality but allows empty results.
// ---------------------------------------------------------------------------

/// Assert async and sync produce identical results for `query`.
/// Unlike `assert_matches_sync` this does NOT assert non-emptiness, so it is
/// safe to call on queries that intentionally return zero rows.
fn assert_empty_matches_sync(query: &str) {
    let adapter = Arc::new(NumbersAdapter::new());
    let schema = adapter.schema().clone();
    let indexed = parse(&schema, query).expect("query failed to parse");
    let expected: Vec<Row> = interpret_ir(adapter, indexed, Arc::new(BTreeMap::new()))
        .expect("unexpected arguments error")
        .map(|row| row.expect("infallible adapter"))
        .collect();

    let numbers = NumbersAdapter::new();
    let schema2 = numbers.schema().clone();
    let adapter2 = Arc::new(SyncToAsyncAdapter::new(Arc::new(numbers)));
    let indexed2 = parse(&schema2, query).expect("query failed to parse");
    let stream = interpret_ir_async(adapter2, indexed2, Arc::new(BTreeMap::new()))
        .expect("unexpected arguments error");
    let actual: Vec<Row> = futures_executor::block_on(async {
        stream.map(|r| r.expect("infallible")).collect().await
    });

    assert_eq!(expected, actual, "async/sync divergence for query:\n{query}");
}

// --- 1: Zero starting vertex, single output ---
#[test]
fn adversarial_zero_vertex_value() {
    assert_matches_sync(r#"{ Zero { value @output } }"#);
}

// --- 2: One starting vertex, two outputs ---
#[test]
fn adversarial_one_vertex_two_outputs() {
    assert_matches_sync(r#"{ One { value @output name @output } }"#);
}

// --- 3: Two starting vertex - already typed as Prime, output value ---
#[test]
fn adversarial_two_vertex_value() {
    assert_matches_sync(r#"{ Two { value @output name @output } }"#);
}

// --- 4: Four starting vertex - already typed as Composite, output value ---
#[test]
fn adversarial_four_vertex_value() {
    assert_matches_sync(r#"{ Four { value @output name @output } }"#);
}

// --- 5: Root coercion that ALWAYS FAILS (zero is neither; coerce to Prime -> 0 rows) ---
#[test]
fn adversarial_root_coercion_always_fails_zero_rows() {
    assert_empty_matches_sync(
        r#"{
            Zero {
                ... on Prime {
                    value @output
                }
            }
        }"#,
    );
}

// --- 6: NumberImplicitNullDefault starting vertex ---
#[test]
fn adversarial_number_implicit_null_default_starting_vertex() {
    assert_matches_sync(r#"{ NumberImplicitNullDefault(max: 5) { value @output name @output } }"#);
}

// --- 7: Empty range produces zero rows ---
#[test]
fn adversarial_empty_range_zero_rows() {
    // min > max => empty iterator from the adapter
    assert_empty_matches_sync(r#"{ Number(min: 10, max: 5) { value @output } }"#);
}

// --- 8: Multiple @outputs in alphabetical (deterministic) order ---
#[test]
fn adversarial_three_outputs_ordering() {
    // Outputs are resolved sorted by name; verify the pipeline preserves order correctly.
    assert_matches_sync(
        r#"{
            Number(min: 1, max: 5) {
                value @output
                vowelsInName: vowelsInName @output
                name @output
            }
        }"#,
    );
}

// --- 9: @optional edge where neighbor EXISTS for all vertices ---
#[test]
fn adversarial_optional_always_present() {
    // All numbers >= 1 have a predecessor in the adapter.
    assert_matches_sync(
        r#"{
            Number(min: 1, max: 8) {
                value @output
                predecessor @optional {
                    p: value @output
                }
            }
        }"#,
    );
}

// --- 10: @optional edge where neighbor is ABSENT for exactly one vertex ---
#[test]
fn adversarial_optional_absent_for_zero() {
    // Number 0 has no predecessor; the output row for 0 should have null for p.
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 5) {
                value @output
                predecessor @optional {
                    p: value @output
                }
            }
        }"#,
    );
}

// --- 11: Deeply nested @optional (3 levels) ---
#[test]
fn adversarial_triple_nested_optional() {
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 8) {
                value @output
                predecessor @optional {
                    p1: value @output
                    predecessor @optional {
                        p2: value @output
                        predecessor @optional {
                            p3: value @output
                        }
                    }
                }
            }
        }"#,
    );
}

// --- 12: __typename output on a polymorphic query ---
#[test]
fn adversarial_typename_output_numbers() {
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 6) {
                __typename @output
                value @output
            }
        }"#,
    );
}

// --- 13: Coercion on edge neighbor (successor), only primes pass ---
#[test]
fn adversarial_coercion_on_successor_only_primes() {
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 12) {
                value @output
                successor {
                    ... on Prime {
                        sp: value @output
                    }
                }
            }
        }"#,
    );
}

// --- 14: Optional coercion where the optional is ABSENT (Zero has no predecessor) ---
#[test]
fn adversarial_optional_absent_then_coercion() {
    assert_matches_sync(
        r#"{
            Number(min: 0, max: 10) {
                value @output
                predecessor @optional {
                    ... on Prime {
                        pp: value @output
                    }
                }
            }
        }"#,
    );
}

// --- 15: Alias-driven output names ---
#[test]
fn adversarial_alias_output_names() {
    assert_matches_sync(
        r#"{
            Zero {
                zero: value @output
                successor {
                    one: value @output
                    successor {
                        two: value @output
                    }
                }
            }
        }"#,
    );
}

// --- 16: Number range coercion that always fails (coerce Number->Composite for primes range) ---
#[test]
fn adversarial_coerce_to_composite_for_primes_zero_rows() {
    // Numbers 11-13 are all prime, not composite => coercion to Composite => zero result rows.
    assert_empty_matches_sync(
        r#"{
            Number(min: 11, max: 13) {
                ... on Composite {
                    value @output
                }
            }
        }"#,
    );
}

// --- 17: optional_with_nested_required_edge_semantics corpus case (zero rows) ---
// If @optional edge exists, subsequent required edges apply normally.
// One's predecessor is Zero (0), and Zero has no predecessor, so the inner
// predecessor is missing -> the whole result is discarded -> zero rows.
#[test]
fn adversarial_optional_nested_required_yields_zero_rows() {
    assert_empty_matches_sync(
        r#"{
            One {
                predecessor @optional {
                    predecessor {
                        value @output
                    }
                }
            }
        }"#,
    );
}

// --- 18: nonexistent_optional_with_immediate_coercion corpus case ---
// Zero has no predecessor. The nonexistent optional carries None context into
// the coercion => coercion must return false => None context passes through
// unchanged (outputting null for the coerced fields).
#[test]
fn adversarial_nonexistent_optional_with_coercion() {
    assert_matches_sync(
        r#"{
            Zero {
                zero: value @output
                predecessor @optional {
                    ... on Composite {
                        value @output
                    }
                }
            }
        }"#,
    );
}
