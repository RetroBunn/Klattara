#!/bin/sh
# Builds Klattara for iPhone and iPad: the app, with the Klattara voices
# extension inside it, for VoiceOver and Spoken Content.
#
#   ./build.sh simulator         build for the iOS Simulator
#   ./build.sh device            build for a real iPhone or iPad (signed with the
#                                team in project.yml; with a free Apple account,
#                                connect the device once so Xcode can register it)
#
# Then install with Xcode (Window > Devices and Simulators), or for the
# simulator: xcrun simctl install booted build/dd/Build/Products/Release-iphonesimulator/Klattara.app
# Needs Rust (rustup target add aarch64-apple-ios aarch64-apple-ios-sim
# x86_64-apple-ios), Xcode and xcodegen.
set -eu
cd "$(dirname "$0")"
ROOT=$(cd ../../.. && pwd)
WHERE=${1:-simulator}

build() { IPHONEOS_DEPLOYMENT_TARGET=16.0 cargo build --release -p klattara-macos --target "$1" --manifest-path "$ROOT/Cargo.toml"; }
mkdir -p lib/iphoneos lib/iphonesimulator
if [ "$WHERE" = device ]; then
	build aarch64-apple-ios
	cp "$ROOT/target/aarch64-apple-ios/release/libklattara.a" lib/iphoneos/
	SDK=iphoneos; DEST=generic/platform=iOS
else
	build aarch64-apple-ios-sim
	build x86_64-apple-ios
	lipo -create "$ROOT/target/aarch64-apple-ios-sim/release/libklattara.a" \
		"$ROOT/target/x86_64-apple-ios/release/libklattara.a" -output lib/iphonesimulator/libklattara.a
	SDK=iphonesimulator; DEST="generic/platform=iOS Simulator"
fi

xcodegen generate
xcodebuild -project KlattaraiOS.xcodeproj -scheme Klattara -configuration Release -sdk $SDK \
	-derivedDataPath build/dd -destination "$DEST" -allowProvisioningUpdates build
echo "built build/dd/Build/Products/Release-$SDK/Klattara.app"
