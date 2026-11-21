#!/bin/sh
# Builds libttsklattara.so for Android 2.2 to 3.2, ARMv5 and newer ("armeabi"),
# into app/src/legacyPlugin/jniLibs/armeabi/ (and armeabi-v7a/, the same file:
# Android 2.x on an ARMv7 phone may take only that folder): the engine, the engine plugin those
# Android versions load (jni/legacy_plugin.c) and the app's JNI bridge, in one
# library, linked against Android 2.2's own libraries with --no-undefined, so a
# function Android 2.2 lacks fails the build.
#
# Needs Linux (x86_64), NDK r10e (the last to build for Android 2.2 on ARMv5; set
# ANDROID_NDK_R10E), Rust with the arm-linux-androideabi target, and a C++
# compiler for the layout test. On a Mac, build-legacy-docker.sh runs this in a
# container.
set -eu
cd "$(dirname "$0")"
HERE=$(pwd)
ROOT=$(cd ../../.. && pwd)
NDK=${ANDROID_NDK_R10E:?set ANDROID_NDK_R10E to NDK r10e}
TC=$NDK/toolchains/arm-linux-androideabi-4.9/prebuilt/linux-x86_64/bin
SYSROOT=$NDK/platforms/android-8/arch-arm
CC="$TC/arm-linux-androideabi-gcc --sysroot=$SYSROOT"
OUT=$HERE/app/src/legacyPlugin/jniLibs/armeabi
# Apart from the host's own builds, which may be another system's.
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target/legacy}

# The engine for ARMv5, Android's oldest 32-bit ARM. Its static library is
# linked below, so Rust's own linker is never run.
cargo build --release --locked -p klattara-android --target arm-linux-androideabi --manifest-path "$ROOT/Cargo.toml"
work=$(mktemp -d)
# Rust's prebuilt compiler_builtins for this target carries some of compiler-rt's
# helpers (division, overflow checks) compiled for ARMv7 with VFPv3, which an ARMv5
# or ARMv6 phone cannot run. Take those out of a copy of the archive, so the same
# helpers come from the NDK's libgcc, built for ARMv5.
ENGINE=$work/libklattara_engine.a
cp "$CARGO_TARGET_DIR/arm-linux-androideabi/release/libklattara_engine.a" "$ENGINE"
newer=$("$TC/arm-linux-androideabi-readelf" -A "$ENGINE" | awk '
	/^File: / { member = $2; sub(/.*\(/, "", member); sub(/\)$/, "", member) }
	/Tag_CPU_arch: / && $2 != "v5TE" && $2 != "v4T" && $2 != "v5T" { print member }' | sort -u)
if [ -n "$newer" ]; then
	echo "taking $(echo "$newer" | wc -l) ARMv7 helpers out of the engine archive, for libgcc's ARMv5 ones"
	"$TC/arm-linux-androideabi-ar" d "$ENGINE" $newer
fi

FLAGS="-O2 -fPIC -march=armv5te -mtune=xscale -msoft-float -fvisibility=hidden -Wall -Wextra -std=gnu99"
$CC $FLAGS -c jni/legacy_plugin.c -o "$work/legacy_plugin.o"
$CC $FLAGS -c jni/klattara_jni.c -o "$work/klattara_jni.o"
mkdir -p "$OUT"
$CC -shared -o "$OUT/libttsklattara.so" "$work/legacy_plugin.o" "$work/klattara_jni.o" "$ENGINE" \
	-Wl,-soname,libttsklattara.so -Wl,--no-undefined -Wl,--gc-sections -Wl,--exclude-libs,ALL \
	-Wl,-z,noexecstack -lgcc -llog -ldl -lm -lc

# Every instruction must run on ARMv5 (see tools/check_armv5.awk), checked while
# the function names are still there.
bad=$("$TC/arm-linux-androideabi-objdump" -d "$OUT/libttsklattara.so" | awk -f tools/check_armv5.awk)
if [ -n "$bad" ]; then
	echo "libttsklattara.so has instructions an ARMv5 phone cannot run:" >&2
	echo "$bad" | head -20 >&2
	exit 1
fi
"$TC/arm-linux-androideabi-strip" --strip-unneeded "$OUT/libttsklattara.so"

# And it must say so: ARMv5TE, Thumb-1 at most. (Tag_FP_arch is there because
# the NDK's libgcc was built with --with-fpu=vfp; the instructions are checked above.)
attrs=$("$TC/arm-linux-androideabi-readelf" -A "$OUT/libttsklattara.so")
if ! echo "$attrs" | grep -q "Tag_CPU_arch: v5TE" || echo "$attrs" | grep -q "Thumb-2"; then
	echo "libttsklattara.so is not marked ARMv5:" >&2
	echo "$attrs" >&2
	exit 1
fi

# Old Android will not load a library with text relocations.
if "$TC/arm-linux-androideabi-readelf" -d "$OUT/libttsklattara.so" | grep -q TEXTREL; then
	echo "libttsklattara.so has text relocations" >&2
	exit 1
fi
for sym in getTtsEngine Java_com_dgomeze42_klattara_KlattaraNative_speak; do
	"$TC/arm-linux-androideabi-nm" -D --defined-only "$OUT/libttsklattara.so" | grep -q " $sym$" \
		|| { echo "libttsklattara.so does not export $sym" >&2; exit 1; }
done
rm -rf "$work"
mkdir -p "$OUT/../armeabi-v7a"
cp "$OUT/libttsklattara.so" "$OUT/../armeabi-v7a/libttsklattara.so"
echo "libttsklattara.so: runs on ARMv5, and links against Android 2.2 (API 8) with nothing missing"
ls -l "$OUT/libttsklattara.so"
