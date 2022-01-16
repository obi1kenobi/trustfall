#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Go to the pytrustfall directory.
cd "$(git rev-parse --show-toplevel)/pytrustfall"

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [["$CURRENT_BRANCH" != 'main']]; then
    echo >&2 "Not publishing since not on main branch: $CURRENT_BRANCH"
    exit 0
fi

LONG_HASH="$(git rev-parse HEAD)"
SHORT_HASH="$(git rev-parse --short HEAD)"
PYTHON_PACKAGE_VERSION="$(python -c 'import toml; print(toml.load("Cargo.toml")["package"]["version"])')"

TAG_NAME="pytrustfall-v${PYTHON_PACKAGE_VERSION}-${SHORT_HASH}"
echo -n "$TAG_NAME"
