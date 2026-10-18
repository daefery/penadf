#!/usr/bin/env bash
# Builds the Windows release (dist/PenaDF/) from Linux or WSL.
#
# Needs: rustup target x86_64-pc-windows-gnullvm, and llvm-mingw unpacked somewhere.
#   LLVM_MINGW=/path/to/llvm-mingw-<ver>-ucrt-ubuntu-22.04-x86_64 scripts/build-windows.sh
# On Windows itself, run scripts/build-windows.ps1 instead.
set -euo pipefail
cd "$(dirname "$0")/.."

LLVM_MINGW="${LLVM_MINGW:-$(ls -d "$HOME"/.local/toolchains/llvm-mingw-*-x86_64 2>/dev/null | tail -1)}"
if [[ ! -x "$LLVM_MINGW/bin/x86_64-w64-mingw32-clang" ]]; then
  echo "llvm-mingw not found. Download it from https://github.com/mstorsjo/llvm-mingw/releases and set LLVM_MINGW." >&2
  exit 1
fi
TARGET=x86_64-pc-windows-gnullvm
rustup target add "$TARGET" >/dev/null

export PATH="$LLVM_MINGW/bin:$PATH"
export CC_x86_64_pc_windows_gnullvm=x86_64-w64-mingw32-clang
export AR_x86_64_pc_windows_gnullvm=llvm-ar
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=x86_64-w64-mingw32-clang

cargo build --release --target "$TARGET"

OUT=dist/PenaDF
rm -rf "$OUT" && mkdir -p "$OUT"
cp "target/$TARGET/release/penadf.exe" "$OUT/PenaDF.exe"
cp vendor/pdfium/windows/pdfium.dll "$OUT/"
scripts/make_notices.sh "$OUT/THIRD-PARTY-NOTICES.txt"
cp LICENSE "$OUT/LICENSE.txt"
cp README.md "$OUT/README.txt"
(cd dist && rm -f PenaDF-windows-x64.zip && python3 -m zipfile -c PenaDF-windows-x64.zip PenaDF)
ls -l "$OUT" dist/PenaDF-windows-x64.zip
