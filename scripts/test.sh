#!/usr/bin/env bash
# Runs every test against the bundled Linux PDFium build.
set -euo pipefail
cd "$(dirname "$0")/.."
export PENADF_PDFIUM_DIR="$PWD/vendor/pdfium/linux"
cargo test "$@"
