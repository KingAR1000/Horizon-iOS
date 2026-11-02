#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/prepare.sh
python3 scripts/build-preparation-kit.py
export IPHONEOS_DEPLOYMENT_TARGET=16.0
export SDKROOT="$(xcrun --sdk iphoneos --show-sdk-path)"
cargo build --locked --release --target aarch64-apple-ios --bin horizon-ios
app="$PWD/build/Payload/Horizon.app"
rm -rf "$PWD/build/Payload"
mkdir -p "$app"
cp target/aarch64-apple-ios/release/horizon-ios "$app/Horizon"
cp ios/Info.plist "$app/Info.plist"
cp -R build/PrepareKit "$app/PrepareKit"
cp LICENSE "$app/LICENSE"
cp engine/THIRD_PARTY_NOTICES.md "$app/UPSTREAM_THIRD_PARTY_NOTICES.md"
cp THIRD_PARTY_NOTICES.md "$app/THIRD_PARTY_NOTICES.md"
# Preserve bundled font and vendored-library license texts in the distributed app.
mkdir -p "$app/Licenses"
find engine/third_party engine/crates/horizon-theme build/vendor/ristretto_intrinsics build/vendor/sysinfo -type f \( -iname '*license*' -o -iname '*notice*' -o -name 'OFL*' \) -print0 |
while IFS= read -r -d '' file; do
    name="${file//\//_}"
    cp "$file" "$app/Licenses/$name"
done
if [[ -d "${CARGO_HOME:-$HOME/.cargo}/registry/src" ]]; then
    find "${CARGO_HOME:-$HOME/.cargo}/registry/src" -type f \( -iname 'license*' -o -iname 'notice*' -o -iname 'copying*' \) -print0 |
    while IFS= read -r -d '' file; do
        relative="${file#*/registry/src/}"
        mkdir -p "$app/Licenses/$(dirname "$relative")"
        cp "$file" "$app/Licenses/$relative"
    done
fi
plutil -lint "$app/Info.plist"
xcrun vtool -show-build "$app/Horizon"
# A Cargo executable has no provisioning profile or code signature. Sign after download.
if codesign -d "$app" 2>/dev/null; then codesign --remove-signature "$app"; fi
(cd build && COPYFILE_DISABLE=1 /usr/bin/zip -qry Horizon-unsigned.ipa Payload)
unzip -t build/Horizon-unsigned.ipa
