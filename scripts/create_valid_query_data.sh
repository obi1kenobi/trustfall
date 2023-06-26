#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Get the absolute path of the repo.
REPO="$(git rev-parse --show-toplevel)"

for INPUT_FILE in "$@"; do
    echo "> Starting on file $INPUT_FILE"

    DIR_NAME="$(dirname "$INPUT_FILE")"
    STUB_NAME="$(basename "$INPUT_FILE" | cut -d'.' -f1)"

    PARSED_FILE="$DIR_NAME/$STUB_NAME.graphql-parsed.ron"
    IR_FILE="$DIR_NAME/$STUB_NAME.ir.ron"
    TRACE_FILE="$DIR_NAME/$STUB_NAME.trace.ron"
    OUTPUTS_FILE="$DIR_NAME/$STUB_NAME.output.ron"

    MANIFEST_PATH="$REPO/trustfall_testbin/Cargo.toml"

    cargo run --manifest-path "$MANIFEST_PATH" parse "$INPUT_FILE" >"$PARSED_FILE"
    cargo run --manifest-path "$MANIFEST_PATH" frontend "$PARSED_FILE" >"$IR_FILE"
    cargo run --manifest-path "$MANIFEST_PATH" outputs "$IR_FILE" >"$OUTPUTS_FILE"
    cargo run --manifest-path "$MANIFEST_PATH" trace "$IR_FILE" >"$TRACE_FILE"
done
