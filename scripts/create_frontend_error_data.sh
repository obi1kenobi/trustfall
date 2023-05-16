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
    FRONTEND_ERROR_FILE="$DIR_NAME/$STUB_NAME.frontend-error.ron"

    MANIFEST_PATH="$REPO/trustfall_testbin/Cargo.toml"

    cargo --manifest-path "$MANIFEST_PATH" run parse "$INPUT_FILE" >"$PARSED_FILE"
    cargo --manifest-path "$MANIFEST_PATH" run frontend "$PARSED_FILE" >"$FRONTEND_ERROR_FILE"
done
