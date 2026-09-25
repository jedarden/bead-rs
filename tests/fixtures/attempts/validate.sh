#!/usr/bin/env bash
# Validate old/new attempt-resolution fixtures in monolithic and sharded modes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$REPO_ROOT"
cargo test --test checkpoint_fixture_conformance
