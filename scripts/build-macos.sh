#!/usr/bin/env bash
# Builds dist/PenaDF-macOS.dmg: a universal PenaDF.app (Apple Silicon + Intel) in a disk image.
# Runs on macOS (locally or on the GitHub Actions macos runner); needs Xcode command line tools.
#
# The app is ad-hoc signed only (no Apple Developer ID), so the first launch needs
# System Settings > Privacy & Security > Open Anyway. Notarizing needs a paid Apple Developer
# Program membership; see RELEASING.md.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname)" == "Darwin" ]] || { echo "build-macos.sh runs on macOS" >&2; exit 1; }

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
PDFIUM_BUILD=7881   # keep in step with vendor/pdfium (pdfium-render 0.9.4 targets 7881)

# 1. Universal binary.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
mkdir -p dist
lipo -create -output dist/PenaDF-universal \
  target/aarch64-apple-darwin/release/penadf target/x86_64-apple-darwin/release/penadf

# 2. Universal PDFium from the same pdfium-binaries release as the Windows build.
PDFIUM_DIR=dist/pdfium-mac
if [[ ! -f "$PDFIUM_DIR/lib/libpdfium.dylib" ]]; then
  rm -rf "$PDFIUM_DIR" && mkdir -p "$PDFIUM_DIR"
  curl -sSL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$PDFIUM_BUILD/pdfium-mac-univ.tgz" \
    | tar xz -C "$PDFIUM_DIR"
fi

# 3. App bundle.
APP=dist/PenaDF.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Frameworks" "$APP/Contents/Resources"
cp dist/PenaDF-universal "$APP/Contents/MacOS/PenaDF"
cp "$PDFIUM_DIR/lib/libpdfium.dylib" "$APP/Contents/Frameworks/"
cp packaging/macos/PenaDF.icns "$APP/Contents/Resources/"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
scripts/make_notices.sh "$APP/Contents/Resources/THIRD-PARTY-NOTICES.txt"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"

# 4. Ad-hoc signature (required to run on Apple Silicon at all; not a Developer ID).
codesign --force --sign - "$APP/Contents/Frameworks/libpdfium.dylib"
codesign --force --sign - "$APP"
codesign --verify --verbose "$APP"

# 5. Disk image: the app plus an Applications shortcut to drag it onto.
STAGE=dist/dmg-stage
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cp packaging/macos/BACA-SAYA.txt "$STAGE/READ ME - BACA SAYA.txt"
rm -f dist/PenaDF-macOS.dmg
hdiutil create -volname "PenaDF $VERSION" -srcfolder "$STAGE" -ov -format UDZO dist/PenaDF-macOS.dmg
ls -l dist/PenaDF-macOS.dmg
