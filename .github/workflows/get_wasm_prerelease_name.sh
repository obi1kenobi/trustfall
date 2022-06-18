#!/usr/bin/env bash

# Fail on first error, on undefined variables, and on failures in pipelines.
set -euo pipefail

# Go to the trustfall_wasm directory.
cd "$(git rev-parse --show-toplevel)/trustfall_wasm"

CURRENT_BRANCH="${GITHUB_REF#refs/heads/}"
if [[ "$CURRENT_BRANCH" != 'wasm_support' ]]; then
    echo >&2 "Not publishing since not on main branch: $CURRENT_BRANCH"
    exit 0
fi

pip install toml 1>&2

LONG_HASH="$(git rev-parse HEAD)"
SHORT_HASH="$(git rev-parse --short HEAD)"
PYTHON_PACKAGE_VERSION="$(python -c 'import toml; print(toml.load("Cargo.toml")["package"]["version"])')"

TAG_NAME="trustfall_wasm-v${PYTHON_PACKAGE_VERSION}-${SHORT_HASH}"
echo -n "$TAG_NAME"
