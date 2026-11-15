#!/bin/sh
# Builds Klattara.app, with the Klattara voices extension inside it.
#
#   ./build.sh            build into build/
#   ./build.sh install    build, put it in /Applications, register the voices
#
# Signed with the team in project.yml (a free Personal Team is enough). macOS
# lists a third-party voice only when it is signed with a real Apple
# certificate, not ad-hoc, and only when its audio component is sandbox safe.
set -eu
cd "$(dirname "$0")"
ROOT=$(cd ../../.. && pwd)
ACTION=${1:-}

# The engine and its C interface, as one universal static library.
for target in aarch64-apple-darwin x86_64-apple-darwin; do
	MACOSX_DEPLOYMENT_TARGET=14.0 cargo build --release -p klattara-macos --target "$target" \
		--manifest-path "$ROOT/Cargo.toml"
done
mkdir -p lib
lipo -create "$ROOT/target/aarch64-apple-darwin/release/libklattara.a" \
	"$ROOT/target/x86_64-apple-darwin/release/libklattara.a" -output lib/libklattara.a

xcodegen generate
xcodebuild -project Klattara.xcodeproj -scheme Klattara -configuration Release \
	-derivedDataPath build/dd -destination generic/platform=macOS -allowProvisioningUpdates build
APP=build/dd/Build/Products/Release/Klattara.app
echo "built $APP"

if [ "$ACTION" = install ]; then
	LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
	pkill -x KlattaraVoices 2>/dev/null || true
	pkill -x Klattara 2>/dev/null || true
	rm -rf /Applications/Klattara.app
	cp -R "$APP" /Applications/
	# Only the installed copy should be known, not the one in build/.
	pluginkit -r "$APP/Contents/PlugIns/KlattaraVoices.appex" 2>/dev/null || true
	"$LSREGISTER" -u "$APP" 2>/dev/null || true
	"$LSREGISTER" -f /Applications/Klattara.app
	pluginkit -a /Applications/Klattara.app/Contents/PlugIns/KlattaraVoices.appex 2>/dev/null || true
	# The system's voice loader backs off after failures and keeps the voice
	# list it last read; restarting it (launchd starts it again) makes it read
	# the new extension at once.
	killall axassetsd 2>/dev/null || true
	open /Applications/Klattara.app
	echo "installed /Applications/Klattara.app"
fi
