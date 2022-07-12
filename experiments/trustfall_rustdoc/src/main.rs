pub mod adapter;

use std::{
    cell::RefCell, collections::BTreeMap, fs::File, io::Read, path::PathBuf, rc::Rc, sync::Arc,
    time::Instant,
};

use adapter::RustdocAdapter;
use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use rustdoc_types::Crate;
use trustfall_core::{frontend::parse, interpreter::execution::interpret_ir, ir::TransparentValue};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a query against rustdoc JSON output.
    Query(QuerySubcommand),

    /// Diff a rustdoc JSON output of a crate against
    /// the rustdoc of a prior version of the same crate,
    /// to look for breaking changes.
    Diff(DiffSubcommand),
}

#[derive(Args)]
struct QuerySubcommand {
    #[clap(value_parser)]
    rustdoc_json_output: PathBuf,
}

#[derive(Args)]
struct DiffSubcommand {
    #[clap(value_parser)]
    current_rustdoc: PathBuf,

    #[clap(value_parser)]
    previous_rustdoc: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Query(query) => handle_query(query)?,
        Commands::Diff(diff) => handle_diff(diff)?,
    }

    Ok(())
}

fn handle_diff(diff: DiffSubcommand) -> anyhow::Result<()> {
    let current_doc = load_rustdoc_from_file(&diff.current_rustdoc)?;
    let previous_doc = load_rustdoc_from_file(&diff.previous_rustdoc)?;

    let schema = RustdocAdapter::schema();
    let adapter = Rc::new(RefCell::new(RustdocAdapter::new(
        Rc::new(current_doc),
        Some(Rc::new(previous_doc)),
    )));

    let struct_missing_query = r#"
{
    CrateDiff {
        previous {
            item {
                ... on Struct {
                    visibility_limit @filter(op: "=", value: ["$public"]) @output
                    name @output @tag
                    struct_type @output @tag

                    span_: span @optional {
                        filename @output
                        begin_line @output
                    }
                }
            }
        }
        current @fold @transform(op: "count") @filter(op: "=", value: ["$zero"]) {
            item {
                ... on Struct {
                    visibility_limit @filter(op: "=", value: ["$public"])
                    name @filter(op: "=", value: ["%name"])
                    struct_type @filter(op: "=", value: ["%struct_type"])
                }
            }
        }
    }
}
"#;
    let mut struct_missing_args = BTreeMap::new();
    struct_missing_args.insert(Arc::from("public"), "public".into());
    struct_missing_args.insert(Arc::from("zero"), 0.into());

    let struct_field_missing_query = r#"
{
    CrateDiff {
        previous {
            item {
                ... on Struct {
                    visibility_limit @filter(op: "=", value: ["$public"])
                    struct_name: name @output @tag
                    struct_type @output @tag

                    field {
                        field_name: name @output @tag
                        visibility_limit @filter(op: "=", value: ["$public"])

                        span_: span @optional {
                            filename @output
                            begin_line @output
                        }
                    }
                }
            }
        }
        current @fold @transform(op: "count") @filter(op: "=", value: ["$zero"]) {
            item {
                ... on Struct {
                    visibility_limit @filter(op: "=", value: ["$public"])
                    name @filter(op: "=", value: ["%struct_name"])
                    struct_type @filter(op: "=", value: ["%struct_type"])

                    field {
                        name @filter(op: "=", value: ["%field_name"])
                        visibility_limit @filter(op: "=", value: ["$public"])
                    }
                }
            }
        }
    }
}
"#;
    let mut struct_field_missing_args = BTreeMap::new();
    struct_field_missing_args.insert(Arc::from("public"), "public".into());
    struct_field_missing_args.insert(Arc::from("zero"), 0.into());

    let enum_missing_query = r#"
{
    CrateDiff {
        previous {
            item {
                ... on Enum {
                    visibility_limit @filter(op: "=", value: ["$public"]) @output
                    name @output @tag

                    span_: span @optional {
                        filename @output
                        begin_line @output
                    }
                }
            }
        }
        current @fold @transform(op: "count") @filter(op: "=", value: ["$zero"]) {
            item {
                ... on Enum {
                    visibility_limit @filter(op: "=", value: ["$public"])
                    name @filter(op: "=", value: ["%name"])
                }
            }
        }
    }
}
"#;
    let mut enum_missing_args = BTreeMap::new();
    enum_missing_args.insert(Arc::from("public"), "public".into());
    enum_missing_args.insert(Arc::from("zero"), 0.into());

    let enum_variant_missing_query = r#"
{
    CrateDiff {
        previous {
            item {
                ... on Enum {
                    visibility_limit @filter(op: "=", value: ["$public"]) @output
                    enum_name: name @output @tag

                    variant {
                        variant_name: name @output @tag

                        span_: span @optional {
                            filename @output
                            begin_line @output
                        }
                    }
                }
            }
        }
        current @fold @transform(op: "count") @filter(op: "=", value: ["$zero"]) {
            item {
                ... on Enum {
                    visibility_limit @filter(op: "=", value: ["$public"])
                    name @filter(op: "=", value: ["%enum_name"])

                    variant {
                        name @filter(op: "=", value: ["%variant_name"])
                    }
                }
            }
        }
    }
}
"#;
    let mut enum_variant_missing_args = BTreeMap::new();
    enum_variant_missing_args.insert(Arc::from("public"), "public".into());
    enum_variant_missing_args.insert(Arc::from("zero"), 0.into());

    let queries = [
        ("struct missing", struct_missing_query, struct_missing_args),
        (
            "struct field missing",
            struct_field_missing_query,
            struct_field_missing_args,
        ),
        ("enum missing", enum_missing_query, enum_missing_args),
        (
            "enum variant missing",
            enum_variant_missing_query,
            enum_variant_missing_args,
        ),
    ];

    for (query_name, query, args) in queries.into_iter() {
        let start_instant = Instant::now();
        let indexed_query = parse(&schema, query).with_context(|| "Not a valid query.")?;
        let results_iter = interpret_ir(adapter.clone(), indexed_query, Arc::from(args))
            .with_context(|| "Query execution error.")?;

        let max_n = 5;
        println!("> Query results (max {max_n}): {query_name}");
        for result in results_iter.take(max_n) {
            let pretty_result: BTreeMap<Arc<str>, TransparentValue> =
                result.into_iter().map(|(k, v)| (k, v.into())).collect();
            println!("{}", serde_json::to_string_pretty(&pretty_result)?);
        }
        let end_instant = Instant::now();
        let total_time = end_instant - start_instant;

        println!("< Results done: {:.2}s\n", total_time.as_secs_f32());
    }

    Ok(())
}

fn handle_query(query: QuerySubcommand) -> anyhow::Result<()> {
    let rustdoc_root = load_rustdoc_from_file(&query.rustdoc_json_output)?;

    let current_crate = Rc::new(rustdoc_root);
    let schema = RustdocAdapter::schema();
    let adapter = Rc::new(RefCell::new(RustdocAdapter::new(current_crate, None)));

    let query = r#"
{
    Crate {
        item {
            ... on Enum {
                visibility_limit @filter(op: "=", value: ["$public"])
                name @output

                variant_: variant @fold {
                    name @output
                }
            }
        }
    }
}
"#;
    let mut args = BTreeMap::new();
    args.insert(Arc::from("public"), "public".into());

    let indexed_query = parse(&schema, query).with_context(|| "Not a valid query.")?;
    let results_iter = interpret_ir(adapter, indexed_query, Arc::from(args))
        .with_context(|| "Query execution error.")?;

    for result in results_iter.take(5) {
        let pretty_result: BTreeMap<Arc<str>, TransparentValue> =
            result.into_iter().map(|(k, v)| (k, v.into())).collect();
        println!("{}", serde_json::to_string_pretty(&pretty_result)?);
    }

    Ok(())
}

fn load_rustdoc_from_file(path: &PathBuf) -> anyhow::Result<Crate> {
    // Parsing JSON after fully reading a file into memory is much faster than
    // parsing directly from a file, even if buffered:
    // https://github.com/serde-rs/json/issues/160
    let mut s = String::new();
    File::open(path)
        .with_context(|| format!("Failed to open rustdoc JSON output file {:?}", path))?
        .read_to_string(&mut s)
        .with_context(|| format!("Failed to read rustdoc JSON output file {:?}", path))?;

    serde_json::from_str(&s)
        .with_context(|| format!("Failed to parse rustdoc JSON output file {:?}", path))
}
