"""Reject release tags that do not identify the package being built."""
import json
import os
import subprocess

metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--no-deps", "--format-version=1", "--locked"
]))
package = next(p for p in metadata["packages"] if p["name"] == "motiondrop")
expected = "v" + package["version"]
actual = os.environ["RELEASE_TAG"]
if actual != expected:
    raise SystemExit(f"Release tag {actual!r} must match package version {expected!r}")
