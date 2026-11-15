#!/bin/sh
# Builds Klattara for Linux, for five kinds of processor, into dist/:
#
#   x86_64    64-bit Intel and AMD PCs
#   i686      32-bit Intel and AMD PCs
#   aarch64   64-bit ARM: Raspberry Pi 4 and 5 (64-bit OS), ARM laptops and servers
#   armv7     32-bit ARM: Raspberry Pi 2 to 4 (32-bit OS), older ARM boards
#   riscv64   64-bit RISC-V
#
# Each package holds the klattara program (linked statically with musl, so it runs
# on any Linux distribution), libklattara.so (glibc 2.17 or newer; 2.27 on RISC-V,
# the first with RISC-V) with klattara.h, the Speech Dispatcher module, install.sh
# and uninstall.sh. Runs on Linux or macOS with Rust, zig and cargo-zigbuild:
#
#   rustup target add x86_64-unknown-linux-musl i686-unknown-linux-musl \
#     aarch64-unknown-linux-musl armv7-unknown-linux-musleabihf riscv64gc-unknown-linux-musl \
#     x86_64-unknown-linux-gnu i686-unknown-linux-gnu aarch64-unknown-linux-gnu \
#     armv7-unknown-linux-gnueabihf riscv64gc-unknown-linux-gnu
#   cargo install cargo-zigbuild      (and zig, from your package manager)
#
#   ./build-linux.sh            all five
#   ./build-linux.sh aarch64    some
set -eu
cd "$(dirname "$0")"
HERE=$(pwd)
ROOT=$(cd ../../.. && pwd)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DIST=$HERE/dist
mkdir -p "$DIST"

for arch in ${*:-x86_64 i686 aarch64 armv7 riscv64}; do
	case $arch in
		x86_64)  musl=x86_64-unknown-linux-musl;       gnu=x86_64-unknown-linux-gnu;       glibc=2.17 ;;
		i686)    musl=i686-unknown-linux-musl;         gnu=i686-unknown-linux-gnu;         glibc=2.17 ;;
		aarch64) musl=aarch64-unknown-linux-musl;      gnu=aarch64-unknown-linux-gnu;      glibc=2.17 ;;
		armv7)   musl=armv7-unknown-linux-musleabihf;  gnu=armv7-unknown-linux-gnueabihf;  glibc=2.17 ;;
		riscv64) musl=riscv64gc-unknown-linux-musl;    gnu=riscv64gc-unknown-linux-gnu;    glibc=2.27 ;;
		*) echo "unknown architecture $arch" >&2; exit 1 ;;
	esac
	echo "== $arch"
	cargo zigbuild --release --locked -p klattara-linux --target "$musl" --manifest-path "$ROOT/Cargo.toml"
	cargo zigbuild --release --locked -p klattara-windows --target "$gnu.$glibc" --manifest-path "$ROOT/Cargo.toml"

	name=klattara-$VERSION-linux-$arch
	out=$DIST/$name
	rm -rf "$out"
	mkdir -p "$out/bin" "$out/lib" "$out/include" "$out/speech-dispatcher"
	cp "$ROOT/target/$musl/release/klattara" "$out/bin/"
	cp "$ROOT/target/$gnu/release/libklattara.so" "$out/lib/"
	cp "$ROOT/src/plat/windows/klattara.h" "$out/include/"
	cp speech-dispatcher/klattara.conf "$out/speech-dispatcher/"
	cp install.sh uninstall.sh klattara.conf.example README.txt "$out/"
	cp "$ROOT/LICENSE" "$ROOT/THIRD-PARTY-NOTICES.txt" "$out/"
	chmod 755 "$out/bin/klattara" "$out/install.sh" "$out/uninstall.sh"
	(cd "$DIST" && tar -czf "$name.tar.gz" "$name")
	echo "   $DIST/$name.tar.gz"
done
