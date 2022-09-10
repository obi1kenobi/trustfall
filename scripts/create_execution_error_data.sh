#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

for INPUT_FILE in "$@"; do
    echo "> Starting on file $INPUT_FILE"

    DIR_NAME="$(dirname "$INPUT_FILE")"
    STUB_NAME="$(basename "$INPUT_FILE" | cut -d'.' -f1)"

    PARSED_FILE="$DIR_NAME/$STUB_NAME.graphql-parsed.ron"
    IR_FILE="$DIR_NAME/$STUB_NAME.ir.ron"
    EXECUTION_ERROR_FILE="$DIR_NAME/$STUB_NAME.exec-error.ron"

    cargo run parse "$INPUT_FILE" >"$PARSED_FILE"
    cargo run frontend "$PARSED_FILE" >"$IR_FILE"
    cargo run trace "$IR_FILE" >"$EXECUTION_ERROR_FILE"
done
