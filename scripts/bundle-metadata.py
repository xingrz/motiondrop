"""Populate bundle version and redistribute available dependency license files."""
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

bundle = Path(sys.argv[1])
host = subprocess.check_output(["rustc", "-vV"], text=True).split("host: ")[1].splitlines()[0]
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--format-version=1", "--locked", "--filter-platform", host
]))
packages = {p["id"]: p for p in metadata["packages"]}
root = metadata["resolve"]["root"]
version = packages[root]["version"]
plist_path = bundle / "Contents/Info.plist"
with plist_path.open("rb") as stream:
    info = plistlib.load(stream)
info["CFBundleShortVersionString"] = version.split("-")[0]
info["CFBundleVersion"] = version.split("-")[0]
with plist_path.open("wb") as stream:
    plistlib.dump(info, stream)

nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
pending = [root]
seen = set()
notices = bundle / "Contents/Resources/ThirdPartyLicenses"
notices.mkdir(parents=True, exist_ok=True)
index = []
while pending:
    package_id = pending.pop()
    if package_id in seen:
        continue
    seen.add(package_id)
    pending.extend(nodes[package_id]["dependencies"])
    if package_id == root:
        continue
    package = packages[package_id]
    source = Path(package["manifest_path"]).parent
    name = package["name"] + "-" + package["version"]
    destination = notices / name
    destination.mkdir(exist_ok=True)
    for file in source.iterdir():
        if file.is_file() and file.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "AUTHORS")):
            shutil.copy2(file, destination / file.name)
    license_file = package.get("license_file")
    if license_file and (source / license_file).is_file():
        shutil.copy2(source / license_file, destination / Path(license_file).name)
    index.append(f"{name}: {package.get('license') or 'See package license'}")
(notices / "INDEX.txt").write_text("\n".join(sorted(index)) + "\n")
