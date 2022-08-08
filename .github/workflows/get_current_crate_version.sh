#!/usr/bin/env bash

# Script requirements:
# - curl
# - jq

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Go to the repo root directory.
cd "$(git rev-parse --show-toplevel)"

# The first argument should be the name of a crate.
CRATE_NAME="$1"

cargo metadata --format-version 1 | \
    jq --arg crate_name "$CRATE_NAME" --exit-status -r \
        '.packages[] | select(.name == $crate_name) | .version'
