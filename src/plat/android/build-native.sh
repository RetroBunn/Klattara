#!/bin/sh
# Builds libklattara.so for every Android processor type, into
# app/src/main/jniLibs/<abi>/, and checks each one loads on the oldest Android it
# is for: Android 4.0 (API 14) for the 32-bit ones, 5.0 (API 21, where 64-bit
# Android starts) for the 64-bit ones.
#
#   ./build-native.sh          all four
#   ./build-native.sh arm64-v8a armeabi-v7a
#
# Needs Rust with the Android targets
#   rustup target add armv7-linux-androideabi aarch64-linux-android i686-linux-android x86_64-linux-android
# and the Android NDK (ANDROID_NDK_HOME, or the newest under $ANDROID_HOME/ndk).
set -eu
cd "$(dirname "$0")"
HERE=$(pwd)
ROOT=$(cd ../../.. && pwd)
SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}
NDK=${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)}
[ -d "$NDK" ] || { echo "No Android NDK: set ANDROID_NDK_HOME" >&2; exit 1; }
case "$(uname -s)" in Darwin) HOST=darwin-x86_64 ;; Linux) HOST=linux-x86_64 ;; *) HOST=windows-x86_64 ;; esac
BIN=$NDK/toolchains/llvm/prebuilt/$HOST/bin
# The NDK's oldest API: the stubs linked against. A 32-bit library linked here
# still loads on 4.0, which check_old_android.py proves symbol by symbol.
API=21

ABIS=${*:-armeabi-v7a arm64-v8a x86 x86_64}
for abi in $ABIS; do
	case $abi in
		armeabi-v7a) rust=armv7-linux-androideabi; clang=armv7a-linux-androideabi; oldest=14 ;;
		arm64-v8a)   rust=aarch64-linux-android;   clang=aarch64-linux-android;   oldest=21 ;;
		x86)         rust=i686-linux-android;      clang=i686-linux-android;      oldest=14 ;;
		x86_64)      rust=x86_64-linux-android;    clang=x86_64-linux-android;    oldest=21 ;;
		*) echo "unknown ABI $abi" >&2; exit 1 ;;
	esac
	CC="$BIN/$clang$API-clang"
	linker_var=CARGO_TARGET_$(echo "$rust" | tr 'a-z-' 'A-Z_')_LINKER
	env "$linker_var=$CC" cargo build --release --locked -p klattara-android --target "$rust" \
		--manifest-path "$ROOT/Cargo.toml"
	out="$HERE/app/src/main/jniLibs/$abi"
	mkdir -p "$out"
	work=$(mktemp -d)
	"$CC" -O2 -fPIC -fvisibility=hidden -Wall -Wextra -c jni/klattara_jni.c -o "$work/klattara_jni.o"
	"$CC" -shared -o "$out/libklattara.so" "$work/klattara_jni.o" \
		"$ROOT/target/$rust/release/libklattara_engine.a" \
		-Wl,-soname,libklattara.so -Wl,--exclude-libs,ALL -Wl,--gc-sections -Wl,--no-undefined \
		-Wl,--hash-style=both -Wl,-z,max-page-size=16384 -Wl,--build-id=sha1 \
		-ldl -llog -lunwind -lm -lc
	rm -rf "$work"
	"$BIN/llvm-strip" --strip-unneeded "$out/libklattara.so"
	python3 tools/check_old_android.py "$out/libklattara.so" "$abi" "$oldest" "$BIN/llvm-nm"
	ls -l "$out/libklattara.so"
done
