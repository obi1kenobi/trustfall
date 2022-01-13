#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Move relative to the top of the repo, so this script can be run from anywhere.
cd "$(git rev-parse --show-toplevel)/trustfall_core"

find ./src/resources/test_data/valid_queries -name '*.graphql-parsed.ron' | \
    xargs -n 1 \
    sh -c 'cargo run frontend $0 >"$(dirname $0)/$(basename $0 | cut -d'.' -f1).ir.ron"'

find ./src/resources/test_data/execution_errors -name '*.graphql-parsed.ron' | \
    xargs -n 1 \
    sh -c 'cargo run frontend $0 >"$(dirname $0)/$(basename $0 | cut -d'.' -f1).ir.ron"'
