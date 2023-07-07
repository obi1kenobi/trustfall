#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

/// Generate a Trustfall adapter stub implementation for a given schema.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Trustfall schema file from which to generate adapter stub.
    ///
    /// Usually a file with a ".graphql" or ".gql" extension.
    #[arg(short, long, value_name = "FILE")]
    schema: PathBuf,

    /// Target directory the generated adapter stubs will be placed.
    ///
    /// All stub code will be contained in the "adapter" subdirectory of this path,
    /// which will be created if it does not exist.
    ///
    /// If any of the generated stub files have the same name as existing files,
    /// the existing files will be overwritten.
    #[arg(short, long, value_name = "DIR")]
    target: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let schema_is_file = cli.schema.is_file();
    let target_is_file = cli.target.is_file();

    if !schema_is_file && target_is_file {
        anyhow::bail!(
            "you might have reversed the arguments: schema path {} is not a file and target path {} is a file",
            cli.schema.display(), cli.target.display(),
        );
    }
    if !schema_is_file {
        anyhow::bail!(
            "schema path {} does not point to a file",
            cli.schema.display()
        );
    }
    if target_is_file {
        anyhow::bail!(
            "target path {} points to a file but should be a directory",
            cli.target.display()
        );
    }

    let schema_text =
        std::fs::read_to_string(cli.schema).context("failed to read the schema file")?;

    let target = &cli.target;
    std::fs::create_dir_all(target).context("failed to create target directory")?;

    trustfall_stubgen::generate_rust_stub(&schema_text, target)
}
