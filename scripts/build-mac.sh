#!/bin/bash
# Mac 用の procon-light.app を作る(Apple Silicon と Intel の両方で動く1つのアプリ)。
#   ./scripts/build-mac.sh
# 要るもの: Rust と cmake。2つの CPU 向けに作るので、先に
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
# できるもの: dist/mac/procon-light.app と dist/mac/procon-light-mac.zip
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT=dist/mac
APP="$OUT/procon-light.app"
rm -rf "$APP" "$OUT/procon-light-mac.zip"
mkdir -p "$APP/Contents/MacOS"
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release --target "$t"
done
lipo -create \
  target/aarch64-apple-darwin/release/procon-light \
  target/x86_64-apple-darwin/release/procon-light \
  -output "$APP/Contents/MacOS/procon-light"
sed "s/VERSION/$VERSION/g" scripts/mac/Info.plist > "$APP/Contents/Info.plist"
# 署名(開発者IDなしの簡易署名)。Apple Silicon では署名が無いと起動できない
codesign --force --deep --sign - "$APP"
(cd "$OUT" && ditto -c -k --keepParent procon-light.app procon-light-mac.zip)
echo "Built: $APP"
