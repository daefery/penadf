#!/usr/bin/env bash
# Writes THIRD-PARTY-NOTICES.txt: licences of every Rust crate in the Windows build
# (via cargo-about, which also fails if a crate uses a licence not listed in about.toml)
# followed by PDFium and the libraries bundled inside pdfium.dll.
#   cargo install cargo-about --features cli
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-THIRD-PARTY-NOTICES.txt}"

cargo about generate --fail installer/notices.hbs -o "$OUT.tmp"
{
  cat "$OUT.tmp"
  echo
  echo "=============================================================================="
  echo "PDFium (pdfium.dll), build 7881 from https://github.com/bblanchon/pdfium-binaries"
  echo "and the libraries it bundles"
  echo "=============================================================================="
  echo
  echo "--- pdfium-binaries packaging ---"
  cat vendor/pdfium/LICENSE
  for f in vendor/pdfium/licenses/*; do
    echo
    echo "--- $(basename "$f") ---"
    cat "$f"
  done
} > "$OUT"
rm -f "$OUT.tmp"
echo "wrote $OUT ($(wc -l < "$OUT") lines)"
