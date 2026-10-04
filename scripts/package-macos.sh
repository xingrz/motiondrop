#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(cargo metadata --no-deps --format-version 1 --locked | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')
arch="${PACKAGE_ARCH:-$(uname -m)}"
archive="MotionDrop-${version}-macos-${arch}.zip"
ditto -c -k --sequesterRsrc --keepParent dist/MotionDrop.app "dist/$archive"
(cd dist && shasum -a 256 "$archive" > "$archive.sha256")
