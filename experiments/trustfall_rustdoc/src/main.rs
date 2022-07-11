pub mod adapter;

use std::{
    cell::RefCell, collections::BTreeMap, fs::File, io::Read, path::PathBuf, rc::Rc, sync::Arc,
};

use adapter::RustdocAdapter;
use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use rustdoc_types::Crate;
use trustfall_core::{frontend::parse, interpreter::execution::interpret_ir, schema::Schema};

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
}

#[derive(Args)]
struct QuerySubcommand {
    #[clap(value_parser)]
    rustdoc_json_output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Query(query) => handle_query(query)?,
    }

    Ok(())
}

fn handle_query(query: QuerySubcommand) -> anyhow::Result<()> {
    let rustdoc_json_output = query.rustdoc_json_output;

    // Parsing JSON after fully reading a file into memory is much faster than
    // parsing directly from a file, even if buffered:
    // https://github.com/serde-rs/json/issues/160
    let mut s = String::new();
    File::open(&rustdoc_json_output)
        .with_context(|| {
            format!(
                "Failed to open rustdoc JSON output file {:?}",
                rustdoc_json_output
            )
        })?
        .read_to_string(&mut s)
        .with_context(|| {
            format!(
                "Failed to read rustdoc JSON output file {:?}",
                rustdoc_json_output
            )
        })?;

    let rustdoc_root: Crate = serde_json::from_str(&s).with_context(|| {
        format!(
            "Failed to parse rustdoc JSON output file {:?}",
            rustdoc_json_output
        )
    })?;

    let schema_text = include_str!("rustdoc.graphql");
    let schema = Schema::parse(schema_text).with_context(|| "Schema is not valid.")?;
    let adapter = Rc::new(RefCell::new(RustdocAdapter::new(Rc::new(rustdoc_root))));

    let query = r#"
{
    Crate {
        root @output
        crate_version @output

        item {
            ... on Struct {
                visibilityLimit @filter(op: "=", value: ["$public"]) @output
                name @output
                attrs @output
                struct_type @output

                span_: span @optional {
                    filename @output
                    begin_line @output
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
        println!("  {:?}", result);
    }

    Ok(())
}
