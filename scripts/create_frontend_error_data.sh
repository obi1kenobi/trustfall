#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Get the absolute path of the repo.
REPO="$(git rev-parse --show-toplevel)"

INPUT_FILES=()

for INPUT_FILE in "$@"; do
    # makes sure we are always using absolute path
    INPUT_FILES+=("$(cd "$(dirname "$1")"; pwd)/$(basename "$1")")
done

# Move relative to the top of the repo, so this script can be run from anywhere.
cd "$REPO/trustfall_testbin"

for INPUT_FILE in $INPUT_FILES; do
    echo "> Starting on file $INPUT_FILE"

    DIR_NAME="$(dirname "$INPUT_FILE")"
    STUB_NAME="$(basename "$INPUT_FILE" | cut -d'.' -f1)"

    PARSED_FILE="$DIR_NAME/$STUB_NAME.graphql-parsed.ron"
    FRONTEND_ERROR_FILE="$DIR_NAME/$STUB_NAME.frontend-error.ron"

    MANIFEST_PATH="$REPO/trustfall_testbin/Cargo.toml"

    cargo run --manifest-path "$MANIFEST_PATH" parse "$INPUT_FILE" >"$PARSED_FILE"
    cargo run --manifest-path "$MANIFEST_PATH" frontend "$PARSED_FILE" >"$FRONTEND_ERROR_FILE"
done
