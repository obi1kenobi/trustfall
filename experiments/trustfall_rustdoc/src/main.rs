use std::{path::PathBuf, fs::File, io::Read};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use rustdoc_types::Crate;

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
        .with_context(|| format!("Failed to open rustdoc JSON output file {:?}", rustdoc_json_output))?
        .read_to_string(&mut s)
        .with_context(|| format!("Failed to read rustdoc JSON output file {:?}", rustdoc_json_output))?;

    let rustdoc_root: Crate = serde_json::from_str(&s)
        .with_context(|| format!("Failed to parse rustdoc JSON output file {:?}", rustdoc_json_output))?;

    Ok(())
}
