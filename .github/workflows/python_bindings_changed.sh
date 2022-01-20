#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

CURRENT_BRANCH_NAME="${GITHUB_REF#refs/heads/}"
if [[ "$CURRENT_BRANCH_NAME" == "main" ]]; then
    # When deciding if the Python bindings have been updated on `main`,
    # compare against the previous tip of `main`. Otherwise, compare against `main`.
    COMPARISON_TARGET="origin/main^"
else
    COMPARISON_TARGET="origin/main"
fi

TRUSTFALL_CORE_CHANGES="$(git shortlog ${COMPARISON_TARGET}..HEAD ./trustfall_core/ | wc -l)"
PYTRUSTFALL_CORE_CHANGES="$(git shortlog ${COMPARISON_TARGET}..HEAD ./pytrustfall/ | wc -l)"

if [[ "$TRUSTFALL_CORE_CHANGES" != 0 || "$PYTRUSTFALL_CORE_CHANGES" != 0 ]]; then
    echo 1
else
    echo 0
fi
