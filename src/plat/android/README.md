# Klattara for Android

Klattara as an Android text-to-speech engine, for TalkBack, other screen readers, braille notetakers and
every app that speaks, from Android 4.0 (API 14) on phones, tablets, notetakers and Wear OS watches. Install
the APK, choose Klattara as the preferred engine (Settings, Accessibility, Text-to-speech output), and it
speaks: everything is in the app, nothing is downloaded.

Features, as on the Mac: the five voices, chosen as the voice profile, which Klattara speaks with everywhere
on every Android version; rate from 30 to 1920 words per minute, following the screen reader's rate or
exact; exact base pitch in hertz; inflection; flutter (off by default); volume; abbreviations; phrase
prediction; the sampling rates from 8000 to 48000 hertz, the highest every Android's audio takes; every
documented `[:cv]` voice setting, kept per voice; your own pronunciation dictionaries; and a test box that
speaks directly or through Android, and saves WAV files.

## The builds

| Flavor | Android | Processors | For |
|---|---|---|---|
| `anyDevice` | 4.0+ | all four | any device: install this if unsure |
| `legacy` | 4.0+ | 32-bit ARM, x86 | Android 4.x phones and notetakers (smaller) |
| `modernArm64` | 5.0+ | 64-bit ARM | most phones and notetakers today |
| `modernArm32` | 5.0+ | 32-bit ARM | older or lower-cost devices |
| `modernX86_64` | 5.0+ | 64-bit Intel | emulators, Chromebooks |
| `modernX86` | 5.0+ | 32-bit Intel | older Intel tablets, emulators |
| `watch` | 7.1+ (Wear OS) | 64-bit and 32-bit ARM | Pixel Watch, Galaxy Watch and others |

All share one application ID and version code, so any one replaces another.

## Building

Needs the Android SDK (with platform 35) and NDK (27 or newer), Java 17, Python 3 and Rust with the Android
targets:

```
rustup target add armv7-linux-androideabi aarch64-linux-android i686-linux-android x86_64-linux-android
./build-native.sh          # the engine, into app/src/main/jniLibs/<abi>/libklattara.so
./gradlew assembleRelease  # every flavor, into app/build/outputs/apk/<flavor>/release/
```

Set `ANDROID_HOME` (or write `sdk.dir=...` in `local.properties`). The source zip already holds built engine
libraries, so changes to the Java side alone only need the Gradle step. The release builds are signed with
the debug key so they install straight away; use your own key to publish.

## How it fits together

- `rust/` is a static library: the engine's C interface (`../windows/src/lib.rs`, unchanged, so it is the
  same as `klattara.dll` and the Mac's), each voice's own `[:cv]` values (`../shared/voice_settings.rs`) and
  the dictionaries (`../shared/dictionary.rs`).
- `jni/klattara_jni.c` is the bridge to Java. Text crosses as UTF-8 bytes; audio is handed over in chunks of
  the size Android's synthesis callback takes, and each chunk can stop speech at once.
- `app/` is plain Java on Android's own APIs (no support libraries, which no longer reach Android 4.0):
  `KlattaraTtsService` is the engine, `Synthesizer` speaks for it, the settings screen and WAV export alike,
  and `SettingsActivity` is the settings screen, built from standard controls for TalkBack and notetakers.
  Choices such as the voice profile and the sampling rate are buttons that open a dialog listing every
  option, rather than drop-down lists, which some notetakers' screen readers cannot reach.

### Running on Android 4.0

Two things in Rust's standard library are newer than Android 4.0 and would stop the library loading there:
its allocator calls `posix_memalign` (Android 4.2), and its unwinder `dl_iterate_phdr` (Android 5.0 on
32-bit ARM). The library uses its own allocator on `malloc` and `memalign`, and the bridge supplies a
`dl_iterate_phdr` that hands over to the system's where there is one and otherwise describes this library
alone, which is all the unwinder needs. `build-native.sh` then checks every system function each library
imports against Android's own records (`tools/bionic-android11/`, from bionic) and fails the build if any is
newer than the oldest Android that library is for.

## Testing

`adb shell am start -a com.dgomeze42.klattara.SELF_TEST -n com.dgomeze42.klattara/.SettingsActivity` runs a
check through Android's speech service (Android 5.0 and newer): every voice at normal and double speed, the
time to the first audio, and the dictionary through the native bridge. The results go to the log under
`KlattaraSelfTest`.

## Dictionaries

Text files, one entry a line: a word and its sounds in ARPAbet (`aback AH0 B AE1 K`, the format of
Klattara's `extra_words.dict` and CMUdict, so those files load as they are), or a word, `=`, and how to say
it (`nvda = en vee dee ay`). `#` starts a comment. Every sound is checked when a file is added; lines with
mistakes are left out and counted, never spoken. A later dictionary wins over an earlier one.
