#!/usr/bin/env bash
# Used to extract all GraphQL queries from all test data into bare GraphQL files
# used to seed a fuzzing corpus.

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Move relative to the top of the repo, so this script can be run from anywhere.
cd "$(git rev-parse --show-toplevel)/trustfall_core"

target_dir="$1"
schema_name="$2"

cargo_cmd="(cargo run --release corpus_graphql \$0 $2 >\$0.tmp)"
mv_cmd="mv \$0.tmp $target_dir/\$(basename \$0 | cut -d'.' -f1).graphql"

find ./src/resources/test_data/ -name '*.graphql.ron' | \
    xargs -n 1 \
    sh -c "$cargo_cmd && $mv_cmd"
