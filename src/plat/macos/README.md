# Klattara for macOS

Klattara's five voices as macOS system voices, for VoiceOver (VoiceOver Utility ▸ Speech ▸ Add Voice),
Spoken Content and every app that speaks: Klattara Standard Steve, Breezy Brittany, Giant Gary, Delicate David
and Tiny Taylor.

Runs on macOS 13 Ventura and newer, on Apple silicon and Intel Macs alike: the app and its voice extension
are universal (arm64 and x86_64), so one build serves both.

- `Klattara.app` edits the settings and carries the voice extension. Settings: exact rate in words per minute
  (30–1920), whether VoiceOver's rate scales it, exact base pitch in hertz (40–550), inflection, volume,
  flutter (off by default), abbreviations, phrase prediction, every other documented `[:cv]` voice setting per
  voice (gender, head size, roughness, breathiness, lax breathiness, richness, open quotient, smoothness, the four
  source levels, baseline fall, hat rise, stress rise, quickness, F4–F6 and their bandwidths), and the sample rate (the same twenty the NVDA add-on offers, 22050 Hz by
  default). Its test box speaks directly or through macOS, and **Save as WAV…** (⌘S) writes the text as a
  16-bit mono WAV at the chosen sample rate.
- `KlattaraVoices.appex` is the `AVSpeechSynthesisProviderAudioUnit`. It links the engine statically
  (`rust/`, which compiles `src/plat/windows/src/lib.rs` unchanged, so the C interface is the DLL's).

## Build

```
./build.sh install
```

Needs Rust (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`), Xcode and `xcodegen`. Signing uses
the team in `project.yml`; a free Personal Team works.

macOS lists a third-party voice only when the extension is signed with a real Apple certificate (not ad hoc)
and its audio component is `sandboxSafe`. The system's voice loader, `axassetsd`, backs off after failures;
`build.sh install` restarts it so the new build is read at once.
