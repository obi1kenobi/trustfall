#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

for INPUT_FILE in "$@"; do
    echo "> Starting on file $INPUT_FILE"

    DIR_NAME="$(dirname "$INPUT_FILE")"
    STUB_NAME="$(basename "$INPUT_FILE" | cut -d'.' -f1)"

    PARSE_ERROR_FILE="$DIR_NAME/$STUB_NAME.parse-error.ron"

    cargo run parse "$INPUT_FILE" >"$PARSE_ERROR_FILE"
done
