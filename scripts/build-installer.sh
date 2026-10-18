#!/usr/bin/env bash
# Builds dist/PenaDF-Setup-<version>.exe (Windows installer) from Linux/WSL.
#
# Needs NSIS 3 (portable zip from https://nsis.sourceforge.io/Download, unpacked):
#   NSIS=/path/to/nsis-3.13 scripts/build-installer.sh
# makensis.exe runs through WSL's Windows interop; on Linux without interop, install
# the native `makensis` package and set MAKENSIS=makensis.
set -euo pipefail
cd "$(dirname "$0")/.."

scripts/build-windows.sh

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NSIS="${NSIS:-$(ls -d "$HOME"/.local/toolchains/nsis-3* 2>/dev/null | tail -1)}"
MAKENSIS="${MAKENSIS:-$NSIS/Bin/makensis.exe}"
if [[ ! -e "$MAKENSIS" ]] && ! command -v "$MAKENSIS" >/dev/null; then
  echo "makensis not found. Unpack NSIS 3 and set NSIS=..., or set MAKENSIS." >&2
  exit 1
fi

# makensis.exe is a Windows program: give it Windows paths when running under WSL.
winpath() { if command -v wslpath >/dev/null && [[ "$MAKENSIS" == *.exe ]]; then wslpath -w "$1"; else echo "$1"; fi; }

"$MAKENSIS" -V2 \
  -DVERSION="$VERSION" \
  -DDIST="$(winpath "$PWD/dist/PenaDF")" \
  -DOUTFILE="$(winpath "$PWD/dist/PenaDF-Setup-$VERSION.exe")" \
  "$(winpath "$PWD/installer/penadf.nsi")"

ls -l "dist/PenaDF-Setup-$VERSION.exe"

# Release files with fixed names (README links point at releases/latest/download/<name>).
mkdir -p dist/release && rm -f dist/release/*
cp "dist/PenaDF-Setup-$VERSION.exe" dist/release/PenaDF-Setup.exe
cp dist/PenaDF-windows-x64.zip dist/release/
(cd dist/release && sha256sum PenaDF-Setup.exe PenaDF-windows-x64.zip > SHA256SUMS.txt && cat SHA256SUMS.txt)
