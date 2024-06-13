#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Move relative to the top of the repo, so this script can be run from anywhere.
cd "$(git rev-parse --show-toplevel)/trustfall_testbin"

find ../trustfall_core/test_data/tests/parse_errors -name '*.graphql.ron' | \
    xargs -n 1 \
    sh -c 'cargo run parse $0 >"$(dirname $0)/$(basename $0 | cut -d'.' -f1).parse-error.ron"'
