#!/bin/sh
# Builds Klattara for Linux and FreeBSD, into dist/:
#
#   x86_64           64-bit Intel and AMD PCs
#   i686             32-bit Intel and AMD PCs
#   aarch64          64-bit ARM: Raspberry Pi 4 and 5 (64-bit OS), ARM laptops and servers
#   armv7            32-bit ARM: Raspberry Pi 2 to 4 (32-bit OS), older ARM boards
#   riscv64          64-bit RISC-V
#   ppc64le          64-bit PowerPC, little-endian: IBM POWER8 and newer, Raptor Talos
#   s390x            IBM Z and LinuxONE mainframes
#   loongarch64      Loongson 3A5000 and newer
#   freebsd-x86_64   FreeBSD on 64-bit Intel and AMD
#   freebsd-aarch64  FreeBSD on 64-bit ARM
#   freebsd-i686     FreeBSD on 32-bit Intel and AMD
#
# Each package holds the klattara program, libklattara.so with klattara.h, the
# Speech Dispatcher module, install.sh and uninstall.sh. On Linux the program is
# linked statically with musl, so it runs on any distribution (on s390x, which
# Rust has no musl for, it needs glibc 2.17), and the library needs glibc 2.17 or
# newer (2.19 on PowerPC, 2.27 on RISC-V and 2.36 on LoongArch, the first with each). FreeBSD
# builds need FreeBSD 14 or newer. Runs on Linux or macOS with Rust, zig and
# cargo-zigbuild:
#
#   rustup target add <the targets named below>
#   cargo install cargo-zigbuild      (and zig, from your package manager)
#
#   ./build-linux.sh            all of them
#   ./build-linux.sh aarch64    some
set -eu
cd "$(dirname "$0")"
HERE=$(pwd)
ROOT=$(cd ../../.. && pwd)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DIST=$HERE/dist
mkdir -p "$DIST"

ALL="x86_64 i686 aarch64 armv7 riscv64 ppc64le s390x loongarch64 freebsd-x86_64 freebsd-aarch64 freebsd-i686"
for arch in ${*:-$ALL}; do
	# The program's target and the library's, with the glibc it is built against.
	case $arch in
		x86_64)          cli=x86_64-unknown-linux-musl;        lib=x86_64-unknown-linux-gnu.2.17 ;;
		i686)            cli=i686-unknown-linux-musl;          lib=i686-unknown-linux-gnu.2.17 ;;
		aarch64)         cli=aarch64-unknown-linux-musl;       lib=aarch64-unknown-linux-gnu.2.17 ;;
		armv7)           cli=armv7-unknown-linux-musleabihf;   lib=armv7-unknown-linux-gnueabihf.2.17 ;;
		riscv64)         cli=riscv64gc-unknown-linux-musl;     lib=riscv64gc-unknown-linux-gnu.2.27 ;;
		ppc64le)         cli=powerpc64le-unknown-linux-musl;   lib=powerpc64le-unknown-linux-gnu.2.19 ;;
		s390x)           cli=s390x-unknown-linux-gnu.2.17;     lib=s390x-unknown-linux-gnu.2.17 ;;
		loongarch64)     cli=loongarch64-unknown-linux-musl;   lib=loongarch64-unknown-linux-gnu.2.36 ;;
		freebsd-x86_64)  cli=x86_64-unknown-freebsd;           lib=$cli ;;
		freebsd-aarch64) cli=aarch64-unknown-freebsd;          lib=$cli ;;
		freebsd-i686)    cli=i686-unknown-freebsd;             lib=$cli ;;
		*) echo "unknown architecture $arch" >&2; exit 1 ;;
	esac
	echo "== $arch"
	cargo zigbuild --release --locked -p klattara-linux --target "$cli" --manifest-path "$ROOT/Cargo.toml"
	cargo zigbuild --release --locked -p klattara-windows --target "$lib" --manifest-path "$ROOT/Cargo.toml"
	musl=${cli%%.[0-9]*}
	gnu=${lib%%.[0-9]*}
	case $arch in freebsd-*) name=klattara-$VERSION-$arch ;; *) name=klattara-$VERSION-linux-$arch ;; esac
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
