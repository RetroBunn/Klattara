#!/usr/bin/env python3
"""Checks that a libklattara.so imports nothing newer than an Android version.

    check_old_android.py LIBRARY ABI MIN_API LLVM_NM

Every undefined dynamic symbol is looked up in bionic's maps (bionic-android11/):
a symbol marked "introduced=N" (or "introduced-<arch>=N") with N above MIN_API
would stop the library from loading on that Android, so it fails the check. Weak
references (looked up at run time, such as getrandom) are allowed."""
import re, subprocess, sys
from pathlib import Path

lib, abi, min_api, nm = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
arch = {"armeabi-v7a": "arm", "arm64-v8a": "arm64", "x86": "x86", "x86_64": "x86_64"}[abi]
here = Path(__file__).parent / "bionic-android11"
introduced = {}
# Symbols in the base LIBC section have been there since the start; the later
# sections are the Android versions that added them. Private and platform
# sections are not for apps at all.
SECTIONS = {"LIBC": 0, "LIBC_N": 24, "LIBC_O": 26, "LIBC_P": 28, "LIBC_Q": 29, "LIBC_R": 30,
            "LIBC_DEPRECATED": 0, "LIBC_PRIVATE": 999, "LIBC_PLATFORM": 999}
def tagged(tags):
    m = re.search(rf"introduced-{arch}=(\d+)", tags) or re.search(r"introduced=(\d+)", tags)
    return int(m.group(1)) if m else None
for name in ("libc.map.txt", "libm.map.txt", "libdl.map.txt"):
    section_api = 0
    for line in (here / name).read_text().splitlines():
        head = re.match(r"^(LIB\w+)\s*\{(.*)", line)
        if head:
            t = tagged(head.group(2))
            section_api = t if t is not None else SECTIONS.get(head.group(1), 0)
            continue
        m = re.match(r"^\s+([A-Za-z_]\w*);(.*)", line)
        if not m:
            continue
        sym, tags = m.group(1), m.group(2)
        # "# arm x86": only on those architectures.
        only = re.search(r"#\s*((?:arm64|arm|x86_64|x86|mips64|mips)(?:\s+(?:arm64|arm|x86_64|x86|mips64|mips))*)\s*$", tags)
        if only and arch not in only.group(1).split():
            continue
        t = tagged(tags)
        api = t if t is not None else section_api
        introduced[sym] = min(api, introduced.get(sym, 999))

out = subprocess.run([nm, "-D", "--undefined-only", lib], capture_output=True, text=True, check=True).stdout
bad = []
for line in out.splitlines():
    parts = line.split()
    kind, sym = parts[-2], parts[-1].split("@")[0]
    if kind in ("w", "v"):
        continue
    api = introduced.get(sym)
    if api is None:
        bad.append(f"{sym} (not in Android's C library at all)")
    elif api > min_api:
        bad.append(f"{sym} (from API {api})")
if bad:
    print(f"{lib} ({abi}) would not load on API {min_api}: " + ", ".join(bad))
    sys.exit(1)
print(f"{abi}: loads on Android API {min_api} and up ({len(out.splitlines())} system functions checked)")
