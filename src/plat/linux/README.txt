Klattara for Linux and FreeBSD
==============================

Klattara, the Klatt-style formant synthesizer, for Linux and FreeBSD: the
klattara command,
and a Speech Dispatcher module so Orca and every program that speaks through
Speech Dispatcher can use it. Everything is in this folder; nothing is downloaded.


WHICH PACKAGE

  x86_64    64-bit Intel and AMD PCs (most desktops and laptops)
  i686      32-bit Intel and AMD PCs
  aarch64   64-bit ARM: Raspberry Pi 4 and 5 with a 64-bit system, ARM laptops
  armv7     32-bit ARM: Raspberry Pi 2, 3 and 4 with a 32-bit system
  riscv64   64-bit RISC-V boards
  ppc64le   64-bit PowerPC: IBM POWER8 and newer, Raptor Talos and Blackbird
  s390x     IBM Z and LinuxONE mainframes
  loongarch64  Loongson 3A5000 and newer

  freebsd-x86_64, freebsd-aarch64 and freebsd-i686: FreeBSD 14 or newer, on
  64-bit Intel and AMD, 64-bit ARM, and 32-bit Intel and AMD.

Not sure? Run "uname -m" in a terminal: it prints the name to choose
(arm64 is aarch64, armv7l is armv7, i386 or i686 is i686, loong64 is
loongarch64; on FreeBSD, amd64 is x86_64 and arm64 is aarch64).

The klattara program runs on any distribution (on IBM Z it needs glibc 2.17).
libklattara.so needs glibc 2.17 or newer (2.19 on PowerPC, 2.27 on RISC-V and
2.36 on LoongArch), which every current distribution has.

On FreeBSD, install Speech Dispatcher with "pkg install speech-dispatcher" to
use Klattara with Orca. The klattara command plays through PulseAudio or
PipeWire when they are installed, and otherwise straight through FreeBSD's own
sound (/dev/dsp).


INSTALLING

In a terminal, in this folder:

  ./install.sh                 for you alone (into ~/.local; no password)
  sudo ./install.sh --system   for everyone on this computer

It installs the program, and adds Klattara to Speech Dispatcher, keeping the
voices you already have. Then:

  spd-say -o klattara "Hello from Klattara"

In Orca: Preferences, Voice, Speech synthesizer: klattara, then choose a voice:
steve, brittany, gary, david or taylor.

If ~/.local/bin is not on your PATH, the klattara command is still there for
Speech Dispatcher; run it as ~/.local/bin/klattara.

To take it out again: ./uninstall.sh (or sudo ./uninstall.sh --system).


THE KLATTARA COMMAND

  klattara "Hello, world."                  speaks it
  echo "Some text" | klattara               speaks standard input
  klattara -v gary -r 400 "Faster."         a voice and a rate in words per minute
  klattara -w hello.wav "Saved."            saves a WAV file
  klattara --cv "hs 110 br 20" "Custom."    any [:cv] voice setting
  klattara -d my-words.dict "nvda"          with a pronunciation dictionary
  klattara --list-voices
  klattara --help                           everything else

It plays through PipeWire or PulseAudio (paplay, pw-cat) or ALSA (aplay),
whichever is there.


SETTINGS

~/.config/klattara/klattara.conf holds the settings Klattara uses everywhere,
Orca included: the voice, the rate in words per minute (30 to 1920), an exact
pitch, inflection, volume, flutter (off unless set), the sampling rate (all
twenty, 8000 to 96000 hertz), every [:cv] setting for each voice, and your
dictionaries. install.sh puts a commented example there; remove the # from a
line to use it. Orca's own rate, pitch and volume then scale these.


PRONUNCIATION DICTIONARIES

A dictionary is a text file, one word a line:

  aback AH0 B AE1 K
  nvda = en vee dee ay

A word and its sounds in ARPAbet (as in Klattara's own dictionary and CMUdict),
or a word, "=" and how to say it. Lines starting with # are comments. Add a
"dictionary = path" line to klattara.conf for each; a later one wins. A line
with a mistake is left out, and klattara says which line, never speaking it.


FOR DEVELOPERS

lib/libklattara.so and include/klattara.h are Klattara's C interface, the same
as klattara.dll on Windows.

Klattara is by RetroBunn, under the MIT licence (LICENSE).
