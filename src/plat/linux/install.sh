#!/bin/sh
# Installs Klattara for Linux, and adds it to Speech Dispatcher so Orca and every
# program that speaks through Speech Dispatcher can use it.
#
#   ./install.sh               for you alone, into ~/.local (no password needed)
#   sudo ./install.sh --system for everyone on this computer, into /usr/local
#
# Speech Dispatcher stops finding its modules by itself once its settings name
# any module, so the modules it was finding (espeak-ng and so on) are named too,
# and stay. Every line added is marked, so ./uninstall.sh takes out exactly those.
set -eu
# Speech Dispatcher's own settings: /etc on Linux, /usr/local/etc on FreeBSD.
SYS_SD=/etc/speech-dispatcher
if [ ! -f "$SYS_SD/speechd.conf" ] && [ -f /usr/local/etc/speech-dispatcher/speechd.conf ]; then
	SYS_SD=/usr/local/etc/speech-dispatcher
fi
HERE=$(cd "$(dirname "$0")" && pwd)
MARK="# Added by Klattara's install.sh"

if [ "${1:-}" = "--system" ]; then
	PREFIX=/usr/local
	SD=$SYS_SD
else
	PREFIX=$HOME/.local
	SD=${XDG_CONFIG_HOME:-$HOME/.config}/speech-dispatcher
fi

mkdir -p "$PREFIX/bin" "$PREFIX/lib" "$PREFIX/include"
cp "$HERE/bin/klattara" "$PREFIX/bin/klattara"
chmod 755 "$PREFIX/bin/klattara"
cp "$HERE/lib/libklattara.so" "$PREFIX/lib/libklattara.so"
cp "$HERE/include/klattara.h" "$PREFIX/include/klattara.h"
echo "Installed $PREFIX/bin/klattara, $PREFIX/lib/libklattara.so and $PREFIX/include/klattara.h"

# Klattara's own settings, for you, if there are none yet.
if [ "${1:-}" != "--system" ]; then
	CONF=${XDG_CONFIG_HOME:-$HOME/.config}/klattara
	if [ ! -f "$CONF/klattara.conf" ]; then
		mkdir -p "$CONF"
		cp "$HERE/klattara.conf.example" "$CONF/klattara.conf"
		echo "Your settings: $CONF/klattara.conf"
	fi
fi

# ---- Speech Dispatcher -------------------------------------------------------
if [ ! -f "$SYS_SD/speechd.conf" ] && [ ! -f "$SD/speechd.conf" ]; then
	echo
	echo "Speech Dispatcher is not installed, so Klattara was not added to it."
	echo "Install it (the package speech-dispatcher), then run this again to use Klattara with Orca."
	exit 0
fi

mkdir -p "$SD/modules"
sed "s|@KLATTARA@|$PREFIX/bin/klattara|g" "$HERE/speech-dispatcher/klattara.conf" > "$SD/modules/klattara.conf"

# A speechd.conf of your own replaces the system's, so start from a copy of it.
if [ ! -f "$SD/speechd.conf" ]; then
	cp "$SYS_SD/speechd.conf" "$SD/speechd.conf"
fi

if ! grep -q '^AddModule "klattara"' "$SD/speechd.conf"; then
	added=""
	# If nothing was named, Speech Dispatcher was finding its modules by itself:
	# name each of them, so they stay once Klattara is named.
	if ! grep -q '^AddModule' "$SD/speechd.conf"; then
		for dir in /usr/lib/*/speech-dispatcher-modules /usr/lib/speech-dispatcher-modules \
			/usr/libexec/speech-dispatcher-modules /usr/lib64/speech-dispatcher-modules \
			/usr/local/lib/speech-dispatcher-modules /usr/local/libexec/speech-dispatcher-modules; do
			[ -d "$dir" ] || continue
			for module in "$dir"/sd_*; do
				[ -x "$module" ] || continue
				binary=$(basename "$module")
				name=${binary#sd_}
				case $name in generic|dummy|cicero|kali|baratinoo|voxin|ibmtts|ivona) continue ;; esac
				if [ -f "$SYS_SD/modules/$name.conf" ] || [ -f "$SD/modules/$name.conf" ]; then
					line="AddModule \"$name\" \"$binary\" \"$name.conf\""
				else
					line="AddModule \"$name\" \"$binary\""
				fi
				printf '%s %s\n' "$line" "$MARK" >> "$SD/speechd.conf"
				added="$added $name"
			done
		done
	fi
	printf 'AddModule "klattara" "sd_generic" "klattara.conf" %s\n' "$MARK" >> "$SD/speechd.conf"
	echo "Added Klattara to $SD/speechd.conf${added:+ (and kept:$added)}"
fi

# Speech Dispatcher reads its settings when it starts; stop it, and the next
# program that speaks starts it again with Klattara. Linux shortens process
# names to 15 characters, so a running one is "speech-dispatch" (FreeBSD keeps
# 19, so there it is "speech-dispatcher"). For you alone,
# only yours; for everyone, every one.
if [ "${1:-}" = "--system" ]; then who=""; else who="-u $(id -u)"; fi
pkill $who -x speech-dispatch 2>/dev/null || true
pkill $who -x speech-dispatcher 2>/dev/null || true
echo
echo "Done. Try:   spd-say -o klattara \"Hello from Klattara\""
echo "In Orca: Preferences, Voice, Speech synthesizer: klattara."
