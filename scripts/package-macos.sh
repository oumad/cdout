#!/usr/bin/env bash
# Build the native cdout as a universal (Apple silicon + Intel) .app and DMG.
# macOS only. Output: dist-native/cdout-native-<version>-universal.dmg
#
# The app is ad-hoc signed (no Developer ID), like the Tauri build: the
# first launch needs right-click > Open, or
#   xattr -dr com.apple.quarantine "/Applications/cdout.app"
set -euo pipefail

for tool in cargo rustup lipo codesign hdiutil plutil; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done

repo="$(cd "$(dirname "$0")/.." && pwd)"
crate="$repo/src-tauri/crates/cdout-native"
version="$(grep -m1 '^version' "$crate/Cargo.toml" | cut -d'"' -f2)"
[ -n "$version" ] || { echo "could not read the version" >&2; exit 1; }

cd "$repo/src-tauri"
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build -p cdout-native --release --target aarch64-apple-darwin
cargo build -p cdout-native --release --target x86_64-apple-darwin

# Assemble in a fresh staging dir; only the finished DMG is copied out.
stage="$(mktemp -d)"
app="$stage/cdout.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/cdout-native" \
  target/aarch64-apple-darwin/release/cdout-native \
  target/x86_64-apple-darwin/release/cdout-native
lipo -info "$app/Contents/MacOS/cdout-native"
cp icons/icon.icns "$app/Contents/Resources/cdout.icns"
sed "s/@VERSION@/$version/g" "$crate/macos/Info.plist" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

codesign --force --sign - --entitlements "$crate/macos/entitlements.plist" "$app"
codesign --verify --strict --verbose=2 "$app"

dmg="$stage/cdout-native-$version-universal.dmg"
hdiutil create -volname "cdout" -srcfolder "$app" -ov -format UDZO "$dmg"
hdiutil verify "$dmg"

mkdir -p "$repo/dist-native"
cp "$dmg" "$repo/dist-native/"
echo "built $repo/dist-native/$(basename "$dmg")"
