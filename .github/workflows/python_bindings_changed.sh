#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

TRUSTFALL_CORE_CHANGES="$(git shortlog origin/main..HEAD ./trustfall_core/ | wc -l)"
PYTRUSTFALL_CORE_CHANGES="$(git shortlog origin/main..HEAD ./pytrustfall/ | wc -l)"

if [[ "$TRUSTFALL_CORE_CHANGES" != 0 || "$PYTRUSTFALL_CORE_CHANGES" != 0 ]]; then
    echo 1
else
    echo 0
fi
