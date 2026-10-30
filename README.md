# Klattara

Klattara is a text-to-speech synthesizer for American English. It is built in the tradition of Dennis Klatt's formant synthesizers, the research behind DECtalk. It does not play back recordings of a human voice. Instead it computes every sound from a model of the vocal tract, so the voice stays clear and steady at very high speaking rates. That makes it a good fit for screen reader users who listen fast.

It ships as an add-on for the NVDA screen reader, and as a Windows library (klattara.dll) with a plain C interface that other programs can use.

## Features

- A cascade and parallel formant synthesizer after Klatt (1980), with the voicing source, spectral tilt and pitch flutter of Klatt and Klatt (1990), and creaky voice.
- Intelligibility at fast rates comes first. Speech is compressed evenly, so every timing relation within and between sounds is kept, and pauses shrink faster than speech does.
- Pronunciation from the CMU Pronouncing Dictionary, about 135,000 words, with a hand-written list of common modern words and names that it lacks or gets wrong ("YouTube", "email", "Nietzsche"). Words in neither go to a letter-to-sound model trained on the same dictionary, helped by common endings and by compounds of known words.
- Klatt's (1979) duration rules, a pitch contour for each phrase, and smooth formant movement between sounds. That includes flapped T and D ("butter"), syllabic L and N ("bottle", "button"), a glottal catch on T at the end of a syllable ("bat now", "button"), a soft R after vowels, vowels pulled back before a dark L ("fill", "bell", "fool"), and front K and G before front vowels ("key", "geese").
- Text normalization for everyday reading:
  - Numbers, including ordinals, decimals, fractions and phone numbers.
  - Money in dollars, pounds, euros and yen, with scales ("$5M", "$10bn") and other countries' dollars ("US$", "A$").
  - Times, dates and years.
  - Units of measure, including squares, cubes and rates ("km²", "m³/s", "km/h").
  - Roman numerals: a king's or pope's number after the name ("Henry VIII" is "Henry the eighth"), and a plain number elsewhere ("World War II", "Final Fantasy VII").
  - Abbreviations, including US states in addresses, and two capital letters spelled in ordinary text ("the US", "photo ID").
  - Greek letters, web and email addresses ("Games.com", "name@example.com"), and British spellings.
- Homographs are chosen by context: "lead" the metal or the verb, "read" in the present or the past, "Polish" or "polish".
- Optional phrase prediction, as in Eloquence: long sentences without commas are broken into phrases, each with its own pitch contour.
- Live control of rate, pitch, inflection (how much the pitch moves) and volume.
- No GPL code, no SAPI and no leaked source. Everything is written from published papers and openly licensed data.

## Installing the NVDA add-on

The add-on needs NVDA 2025.1 or later, on Windows 10 or later. It works in both 32-bit NVDA, as NVDA was before 2026, and 64-bit NVDA.

1. Download klattara-1.0.nvda-addon.
2. Open the file while NVDA is running, and confirm that you want to install it.
3. Restart NVDA when it asks.
4. Press NVDA+Control+S to open the synthesizer list, choose Klattara, and press Enter.

### Recommended settings

You find these in NVDA's Voice settings, opened with NVDA+Control+V.

1. Rate: 15 is the normal rate, about 190 words per minute. 41 or 42 is about 430 words per minute, the fastest rate where Klattara has tested clearest so far. The slider goes up to about 950 words per minute, so there is no separate rate boost.
2. Pitch: 12, which is the default, a base pitch of 107 Hz, close to the engine's normal 110.
3. Inflection: 40, which is the default, normal pitch movement. Set it to 0 for a monotone.
4. Expand abbreviations: checked by default. When checked, "Dr." is read "doctor" and "Calif." is read "California". When unchecked, abbreviations are read as written: spelled out when they have no vowel, and sounded out when they do.
5. Phrase prediction: unchecked by default, as most screen reader users prefer. When checked, a long sentence without commas is broken into phrases where a reader would break it, such as before "which" or "toward": the pitch falls a little and starts afresh, with no pause. It can make long sentences in documents easier to follow.

The sliders map evenly onto the engine's values, as DECtalk's do. Rate goes from 0.29 to 5 times normal, pitch from 40 to 600 Hz, and inflection from 0 to 250 percent of normal.

## Using the library from other programs

klattara.dll is a Windows library, in 64-bit and 32-bit versions, for Windows 10 or later. It needs nothing else installed: Microsoft's C runtime is built in. Its interface is declared in src/plat/windows/klattara.h. Every function catches internal errors and returns an error code instead of crashing the program that calls it. An engine may be used from any thread, but from only one thread at a time. Text can be any length: very long text is read in pieces, so memory stays small and speech starts at once.

On 32-bit Windows, the functions and the audio callback use the C calling convention (cdecl). The header says so for C and C++ programs; programs in other languages need to declare it themselves.

### The C interface

- `klattara_version()` returns the library version, such as "1.0.0".
- `klattara_create(sample_rate)` makes an engine at 8000 to 96000 Hz; 22050 is recommended. It returns NULL if the rate is out of range.
- `klattara_destroy(engine)` frees an engine.
- `klattara_set_rate(engine, rate)` sets the speaking rate. 1.0 is normal and 2.0 twice as fast. The range is 0.25 to 8.
- `klattara_set_pitch(engine, hz)` sets the base pitch, 40 to 600 Hz. The default is 110.
- `klattara_set_inflection(engine, inflection)` sets the pitch movement, from 0 for a monotone through 1 for normal, up to 2.5.
- `klattara_set_volume(engine, volume)` sets the volume, from 0 for silence through 1 for normal, up to 2.
- `klattara_set_abbreviations(engine, expand)` reads abbreviations in full when expand is nonzero, which is the default, or as written when it is zero.
- `klattara_set_phrase_prediction(engine, predict)` breaks long stretches of text without punctuation into phrases when predict is nonzero. It is off by default.
- `klattara_speak(engine, input, reading, callback, user)` speaks UTF-8 input. It hands signed 16-bit mono samples to the callback in chunks as they are made. The callback returns 0 to go on, or nonzero to stop at once. The reading is one of three constants:
  - `KLATTARA_READ_TEXT` for ordinary text.
  - `KLATTARA_READ_SPELLED` to read character by character.
  - `KLATTARA_READ_PHONEMES` for bracketed ARPAbet, such as "[HH AH0 L OW1]".
- `klattara_transcribe(text, buffer, size)` writes how text will be pronounced, in bracketed ARPAbet. It returns the buffer size needed, including the terminating NUL.

The functions return `KLATTARA_OK` (0) on success, and `KLATTARA_STOPPED` (1) when the callback stopped speech early. They return `KLATTARA_INVALID` (-1) for a null pointer, a value out of range or bad phonetic input, and `KLATTARA_INTERNAL_ERROR` (-2) if something went wrong inside.

A short example:

```c
static int on_audio(void *user, const int16_t *samples, int count) {
    /* Send count samples to the sound card or a file. */
    return 0;
}

KlattaraEngine *engine = klattara_create(22050);
klattara_set_rate(engine, 2.0);
klattara_speak(engine, "Hello, world.", KLATTARA_READ_TEXT, on_audio, NULL);
klattara_destroy(engine);
```

### The Rust interface

The engine is the Rust crate in src/engine. A `Voice` holds the settings: pitch, inflection, rate, volume, whether to expand abbreviations, whether to use phrase prediction, and the voice quality. `speak_text` returns a whole utterance as samples between -1 and 1. `speak_with` streams audio to a closure as it is made. `transcribe_with` gives the pronunciation in bracketed ARPAbet. `wav::write_wav` saves samples to a WAV file.

```rust
use klattara::{Voice, speak_text, wav};

let voice = Voice { rate: 2.0, ..Voice::default() };
let audio = speak_text("Hello, world.", &voice, 22050);
wav::write_wav("hello.wav", &audio, 22050)?;
```

## Building from source

You need Rust 1.99 or later; the code uses the 2024 edition. To build the engine and klattara.dll, run this from the project folder:

    cargo build --release

The DLL is written to target/release/klattara.dll. For the 32-bit version, add Rust's 32-bit target once, and build for it:

    rustup target add i686-pc-windows-msvc
    cargo build --release -p klattara-windows --target i686-pc-windows-msvc

That DLL is written to target/i686-pc-windows-msvc/release/klattara.dll.

To build the NVDA add-on, you also need the 32-bit target above, and both 64-bit and 32-bit Python with the py launcher. The build runs the driver's tests against NVDA's own source code, so clone NVDA into the project folder first:

    git clone --depth 1 https://github.com/nvaccess/nvda nvda
    python src/plat/nvda/build.py

That builds both DLLs, tests the driver on each, and writes dist/klattara-1.0.nvda-addon, which carries both. To test the 32-bit DLL on the code of 32-bit NVDA rather than current NVDA, also clone its last release; the tests use it if it's there:

    git clone --depth 1 --branch release-2025.1 https://github.com/nvaccess/nvda nvda-release-2025.1

To correct a word's pronunciation, or add a word, edit src/data/extra_words.dict and rebuild. The notes at the top of that file explain the format.

To try the voice without NVDA, the say example writes a WAV file and prints the pronunciation it used:

    cargo run --release --example say -- "Hello, world." samples/hello.wav 2.0

## Testing

Run the unit tests with `cargo test --release`. They check the signal processing, durations, text reading and the C interface. Add `--target i686-pc-windows-msvc` to run them in 32-bit.

The tools in src/tools measure how clear the voice is, using a speech recognizer as a stand-in for a listener. They need a Python environment in .venv with faster-whisper and numpy installed.

- intelligibility.py reads the Harvard sentences in sentences.tsv at chosen rates, and reports the share of words the recognizer got right.
- word_pairs.py checks whether pairs of words that differ in one sound, such as "bat" and "pat", stay distinct at a given rate. It uses three lists: word_pairs.tsv, vowel_pairs.tsv and final_pairs.tsv.
- homograph_eval.py scores the homograph choices against the Wikipedia Homograph Data. The data is not included; the tool's own description says how to get it.
- normalization_eval.py compares how numbers, dates and abbreviations are read against the Sproat and Jaitly (2016) text normalization data. That data is large and is not included. Look for errors in its training files, and keep the standard test set, from the last file, for the final score.
- train_lts.py retrains the letter-to-sound model from CMUdict, and build_homographs.py rebuilds the homograph table.

## How it works

1. Text is split into sentences and phrases. Numbers, dates, money, abbreviations and symbols are turned into words. With phrase prediction on, a long stretch without punctuation is also split, before words that start a new group, such as "and it", "which" or a preposition, into parts of similar length (Bachenko and Fitzpatrick 1990).
2. Each word's pronunciation comes from the dictionary or a homograph rule. A word the dictionary lacks is built from its stem if it has a common ending ("rebooted"), or else goes to the letter-to-sound model, whose main stress is checked against a table of word endings ("relegation"). If the model is unsure of a word, and it is made of two known words ("webcam"), it is said as those two words. Endings like -s and -ed are marked, since they change how the word is timed. Each word keeps a single main stress. A compound stresses its first part ("MIcrosoft"), and words like "nineteen" and "outside" follow the rhythm of the sentence ("she's nineTEEN", "NINEteen EIGHty").
3. The sounds are grouped into syllables, and each takes the right variant for its position, such as an aspirated, a flapped or a glottalized T. Before a glottalized T, the voice turns creaky, lower and uneven, as most American speakers' voices do (Garellek and Seyfarth 2016).
4. Klatt's duration rules set each sound's length from its stress, its place in the word and phrase, and its neighbors. The whole timeline is then compressed to the speaking rate.
5. A pitch contour is laid over each phrase, with accents on stressed words and a rise or fall at the end that depends on the punctuation. Each accent is a little lower than the one before, but never less than half height, so long sentences don't flatten out. Following Bell Labs' accent rules (Hirschberg 1993), words that only link clauses, such as "which" in mid-sentence or "because", get no accent, and neither does a word mentioned before ("buy a DOG, but I'm allergic to dogs"), unless it is a name or a number. Questions that start with a question word, such as "What time is it?", fall like statements, as most do in American speech; other questions rise.
6. Each sound has targets for the formants, the resonances of the vocal tract. Smooth transitions join the targets, so the resonances glide from one sound to the next.
7. The formant synthesizer turns these tracks into audio: a glottal pulse and noise sources shaped by a chain of resonators.

## Project layout

- src/engine: the speech engine, as a Rust library, with examples.
- src/plat/windows: klattara.dll and its C header.
- src/plat/nvda: the NVDA add-on, its build script and its tests.
- src/data: the pronouncing dictionary, the hand-written additions to it, the letter-to-sound model and the homograph table.
- src/tools: Python tools for measuring clarity and for rebuilding the data.
- src/research: a list of the papers the engine is based on, with where to get each one. The papers themselves are not included.

## Credits

- Dennis Klatt's papers on formant synthesis, segment durations and synthesis by rule are the foundation of the engine. src/research/README.md lists every paper used.
- The CMU Pronouncing Dictionary, by Carnegie Mellon University, under a BSD license.
- The Wikipedia Homograph Data, by Kyle Gorman, Gleb Mazovetskiy and Vitaly Nikolaev, under the Apache 2.0 license. The homograph table is built from it.
- Wikidata, dedicated to the public domain under CC0. The names of monarchs and popes that take a numbered title ("Henry VIII") come from it.
- NV Access, for NVDA and its source code, which the add-on is written against.
- TGSpeechBox, by Tamas Geczy, for the idea of weaker coarticulation in fast speech, which makes consonants clearer.

The full license texts for the included data are in THIRD-PARTY-NOTICES.txt.

## License

Klattara is released under the MIT License. See the LICENSE file.
