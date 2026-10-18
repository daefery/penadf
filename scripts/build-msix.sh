#!/usr/bin/env bash
# Builds dist/PenaDF-<version>.msix for the Microsoft Store.
#
# The Store signs what it publishes, so Store installs show no SmartScreen warning.
# Upload the UNSIGNED package to Partner Center. For a local install test, pass --test-sign:
# it signs with a throwaway self-signed certificate (never upload that one).
#
#   PENADF_STORE_IDENTITY=12345Fery.PenaDF \
#   PENADF_STORE_PUBLISHER="CN=XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX" \
#   PENADF_STORE_PUBLISHER_NAME="Fery Yundara Putera" \
#   scripts/build-msix.sh [--test-sign]
#
# The three values are on Partner Center > PenaDF > Product identity, after the name is reserved.
# Needs: dist/PenaDF/ (scripts/build-windows.sh) and the winapp CLI
# (https://github.com/microsoft/winappcli/releases, winappcli-x64.zip) run through WSL interop.
set -euo pipefail
cd "$(dirname "$0")/.."

TEST_SIGN=0
[[ "${1:-}" == "--test-sign" ]] && TEST_SIGN=1

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
IDENTITY="${PENADF_STORE_IDENTITY:-PenaDF.PenaDF}"
PUBLISHER="${PENADF_STORE_PUBLISHER:-CN=PenaDF Test}"
PUBLISHER_NAME="${PENADF_STORE_PUBLISHER_NAME:-PenaDF}"
WINAPP="${WINAPP:-$(ls "$HOME"/.local/toolchains/winapp/winapp.exe 2>/dev/null || true)}"
[[ -x "$WINAPP" ]] || { echo "winapp CLI not found; set WINAPP=/path/to/winapp.exe" >&2; exit 1; }
[[ -f dist/PenaDF/PenaDF.exe ]] || { echo "run scripts/build-windows.sh first" >&2; exit 1; }
export WINAPP_CLI_TELEMETRY_OPTOUT=1

# Package layout: the app files plus the manifest and tile images.
LAYOUT=dist/msix-layout
rm -rf "$LAYOUT" && mkdir -p "$LAYOUT"
cp dist/PenaDF/{PenaDF.exe,pdfium.dll,LICENSE.txt,THIRD-PARTY-NOTICES.txt} "$LAYOUT/"
cp -r packaging/msix/Assets "$LAYOUT/"
sed -e "s|@IDENTITY@|$IDENTITY|" -e "s|@PUBLISHER@|$PUBLISHER|" -e "s|@PUBLISHER_NAME@|$PUBLISHER_NAME|" \
    -e "s|@VERSION@|$VERSION|" packaging/msix/AppxManifest.xml > "$LAYOUT/AppxManifest.xml"

OUT="dist/PenaDF-$VERSION.msix"
rm -f "$OUT"
win() { wslpath -w "$1"; }
if [[ $TEST_SIGN == 1 ]]; then
  CERT=dist/penadf-test-cert.pfx
  [[ -f "$CERT" ]] || (cd dist && "$WINAPP" cert generate --publisher "$PUBLISHER" --output "$(win "$PWD/penadf-test-cert.pfx")" >/dev/null)
  "$WINAPP" package "$(win "$PWD/$LAYOUT")" --manifest "$(win "$PWD/$LAYOUT/AppxManifest.xml")" \
    --output "$(win "$PWD/$OUT")" --cert "$(win "$PWD/$CERT")" --skip-pri -q
  echo "TEST-SIGNED (local install only, never upload): $OUT"
else
  "$WINAPP" package "$(win "$PWD/$LAYOUT")" --manifest "$(win "$PWD/$LAYOUT/AppxManifest.xml")" \
    --output "$(win "$PWD/$OUT")" --no-sign --skip-pri -q
  echo "unsigned, for Partner Center upload: $OUT"
fi
ls -l "$OUT"
