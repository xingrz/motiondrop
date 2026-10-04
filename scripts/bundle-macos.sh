#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked
bundle="dist/MotionDrop.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "${CARGO_TARGET_DIR:-target}/release/motiondrop" "$bundle/Contents/MacOS/MotionDrop"
cp packaging/Info.plist "$bundle/Contents/Info.plist"
cp LICENSE "$bundle/Contents/Resources/LICENSE"
python3 scripts/bundle-metadata.py "$bundle"
codesign --force --deep --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
printf 'Built: %s/%s\n' "$PWD" "$bundle"
