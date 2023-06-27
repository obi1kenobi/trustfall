#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Get the absolute path of the repo.
REPO="$(git rev-parse --show-toplevel)"

for INPUT_FILE in "$@"; do
    echo "> Starting on file $INPUT_FILE"

    DIR_NAME="$(dirname "$INPUT_FILE")"
    STUB_NAME="$(basename "$INPUT_FILE" | cut -d'.' -f1)"

    PARSE_ERROR_FILE="$DIR_NAME/$STUB_NAME.parse-error.ron"

    MANIFEST_PATH="$REPO/trustfall_testbin/Cargo.toml"

    cargo run --manifest-path "$MANIFEST_PATH" parse "$INPUT_FILE" >"$PARSE_ERROR_FILE"
done
