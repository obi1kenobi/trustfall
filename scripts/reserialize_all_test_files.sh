#!/usr/bin/env bash
# Used to update the serialization of the test files by:
# - reading the current, backward-compatible format, and
# - writing the same data in the latest format

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Move relative to the top of the repo, so this script can be run from anywhere.
cd "$(git rev-parse --show-toplevel)/trustfall_core"

# We ignore .graphql.ron files since those:
# - are relatively simple, and
# - have specific multiline string formatting that makes GraphQL human-readable
#   and that we want to preserve.
find ./src/resources/test_data/ -name '*.ron' | \
    grep -v '.graphql.ron' | \
    xargs -n 1 \
    sh -c '(cargo run --release reserialize $0 >$0.tmp) && mv $0.tmp $0'
