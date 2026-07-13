#!/usr/bin/env bash

set -euo pipefail

export HIS_HOME="$PWD/.his"
readonly HIS_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"

cargo build --target-dir "$HIS_TARGET_DIR"
exec "$HIS_TARGET_DIR/debug/his" "$@"
