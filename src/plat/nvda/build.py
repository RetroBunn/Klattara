"""Builds the Klattara NVDA add-on.

Builds klattara.dll for 64-bit and 32-bit NVDA, runs the driver tests against
each, and packs the add-on into dist/klattara-<version>.nvda-addon, ready to
install by opening it.

Usage, from the project folder:
    python src/plat/nvda/build.py
"""

import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
ADDON = ROOT / "src" / "plat" / "nvda" / "addon"
DIST = ROOT / "dist"
CARGO_BUILD = ["cargo", "build", "--release", "-p", "klattara-windows"]
# NVDA is 32-bit before 2026 and 64-bit from then on. For each: the add-on's
# folder for the DLL, how to build it, where it lands, and how the Python
# launcher starts a Python of the same width to test it.
BUILDS = [
    ("x64", CARGO_BUILD, ROOT / "target" / "release" / "klattara.dll", "-3-64"),
    (
        "x86",
        [*CARGO_BUILD, "--target", "i686-pc-windows-msvc"],
        ROOT / "target" / "i686-pc-windows-msvc" / "release" / "klattara.dll",
        "-3-32",
    ),
]


def version():
    for line in (ADDON / "manifest.ini").read_text(encoding="utf-8").splitlines():
        key, _, value = line.partition("=")
        if key.strip() == "version":
            return value.strip()
    raise SystemExit("manifest.ini has no version")


def main():
    for architecture, build, _, python in BUILDS:
        subprocess.run(build, cwd=ROOT, check=True)
        tests = subprocess.run(["py", python, str(Path(__file__).with_name("test_driver.py"))], cwd=ROOT)
        if tests.returncode != 0:
            raise SystemExit(f"The {architecture} driver tests failed, so no add-on was built.")
    DIST.mkdir(exist_ok=True)
    target = DIST / f"klattara-{version()}.nvda-addon"
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as addon:
        for path in sorted(ADDON.rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts:
                addon.write(path, path.relative_to(ADDON).as_posix())
        for architecture, _, dll, _ in BUILDS:
            addon.write(dll, f"synthDrivers/klattara/{architecture}/klattara.dll")
        addon.write(ROOT / "LICENSE", "LICENSE")
        addon.write(ROOT / "THIRD-PARTY-NOTICES.txt", "THIRD-PARTY-NOTICES.txt")
    print(f"Built {target}")


if __name__ == "__main__":
    main()
