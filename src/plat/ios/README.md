# Klattara for iPhone and iPad

Klattara's five voices as iOS and iPadOS system voices, for VoiceOver (Settings, Accessibility, VoiceOver,
Speech, Voice), Spoken Content and every app that speaks. This is the Mac version's app and voice extension
(`../macos/Sources`), built for iPhone and iPad: the same settings (rate 30–1920 words per minute, base pitch,
inflection, flutter, volume, abbreviations, phrase prediction, every `[:cv]` voice setting per voice, the
twenty sample rates), the same test box, and Save as WAV, which saves into Klattara's folder in the Files app.
Needs iOS or iPadOS 16 or newer.

## Build and install

```
./build.sh device       # for a real iPhone or iPad
./build.sh simulator    # for the iOS Simulator
```

Needs Rust (`rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios`), Xcode and
`xcodegen`. Signing uses the team in `project.yml`; to build for yourself, put your own team there and change
the bundle identifier and App Group (`group.<your bundle identifier>`) to your own.

Install with Xcode (Window, Devices and Simulators, then the + under Installed Apps), or with
`xcrun devicectl device install app --device <device> build/dd/Build/Products/Release-iphoneos/Klattara.app`.
Open Klattara once, then choose a Klattara voice in VoiceOver's Speech settings. With a free Apple account the
app works for 7 days, then needs installing again; a paid developer account makes it a year.

The app and the voice extension share their settings through the App Group, so a change in the app reaches
VoiceOver from its next sentence.
