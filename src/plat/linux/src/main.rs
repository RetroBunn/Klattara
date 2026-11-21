//! klattara: Klattara on Linux.
//!
//! Speaks text from the command line or standard input, through the system's
//! audio (PipeWire, PulseAudio or ALSA), into a WAV file, or as raw PCM on standard
//! output; and is what Speech Dispatcher runs for Orca and every other program that
//! speaks through it (see speech-dispatcher/klattara.conf).
//!
//! Settings come from ~/.config/klattara/klattara.conf, then from the command line.
//! Audio goes to the player as it is made, so speech starts at once.

#[path = "../../shared/dictionary.rs"]
#[allow(dead_code)]
mod dictionary;

use dictionary::Dictionary;
use engine::{FASTEST_WPM, Reading, SLOWEST_WPM, VOICES, Voice, VoiceDef, follow_commands, speak_with};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Every rate the engine is tuned for, in Hz, as on Windows, the Mac and Android.
const SAMPLE_RATES: [u32; 20] = [
    8000, 10000, 11000, 11025, 12000, 16000, 20000, 22000, 22050, 24000, 32000, 40000, 44000, 44100, 48000, 64000,
    80000, 88000, 88200, 96000,
];

/// The [:cv] settings Klattara documents, which --cv and the config's cv lines may set.
const CV_CODES: [&str; 25] = [
    "pi", "in", "hs", "rg", "br", "rc", "sm", "ge", "f4", "f5", "f6", "b4", "b5", "b6", "fl", "vvl", "fvl", "avl",
    "nvl", "fa", "ha", "st", "qk", "lx", "oq",
];

const HELP: &str = "\
klattara: the Klattara speech synthesizer

Usage: klattara [options] [text...]
Speaks the text, or standard input if none is given.

Voice
  -v, --voice NAME         steve, brittany, gary, david or taylor (or a full name, such as \"Giant Gary\")
  -r, --rate WPM           words per minute, 30 to 1920 (190 is normal)
  -p, --pitch HZ           an exact base pitch, 40 to 550 hertz (else the voice's own)
  -i, --inflection PCT     how much the pitch moves, 0 to 250 percent of normal
      --volume PCT         0 to 200 percent (100 is normal)
      --flutter PCT        how much the pitch wavers, 0 to 100 (0, off, unless set)
      --cv SETTINGS        any of the voice's [:cv] settings, such as \"hs 90 br 30\"
      --no-abbreviations   read \"Dr.\" as letters, not \"doctor\"
      --phrase-prediction  break long stretches without punctuation into phrases
      --spell              read the text character by character

Output
  -s, --sample-rate HZ     one of 8000 10000 11000 11025 12000 16000 20000 22000 22050 24000
                           32000 40000 44000 44100 48000 64000 80000 88000 88200 96000 (22050)
  -w, --wav FILE           save a WAV file (- for standard output) instead of playing
      --raw                write 16-bit little-endian mono PCM to standard output
      --player COMMAND     play through this command, which reads raw PCM on its input;
                           {rate} becomes the sample rate (default: paplay, pw-cat or aplay;
                           on FreeBSD, /dev/dsp when none is installed)

Pronunciation
  -d, --dictionary FILE    use a pronunciation dictionary (may be given more than once)
      --commands           obey Klattara's [: commands in the text (off: brackets are read)

Speech Dispatcher (used by speech-dispatcher/klattara.conf)
      --sd-rate N --sd-pitch N --sd-volume N   Speech Dispatcher's -100 to 100 settings

Other
      --config FILE        read settings from FILE (default ~/.config/klattara/klattara.conf)
      --no-config          read no settings file
      --list-voices        list the voices
      --show-settings      show the settings that would be used, and stop
  -h, --help               this help
      --version            the version

Dictionaries are text files, one entry a line: a word and its sounds in ARPAbet
(\"aback AH0 B AE1 K\") or a word, \"=\", and how to say it (\"nvda = en vee dee ay\").
";

#[derive(Clone, Debug)]
struct Settings {
    voice: String,
    rate: f64,
    pitch: Option<f64>,
    inflection: f64,
    volume: f64,
    flutter: f64,
    /// Each voice's own [:cv] settings, by lower-case short name, then "all".
    cv: Vec<(String, String)>,
    abbreviations: bool,
    phrase_prediction: bool,
    sample_rate: u32,
    dictionaries: Vec<PathBuf>,
    player: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            voice: "steve".into(),
            rate: 190.0,
            pitch: None,
            inflection: 100.0,
            volume: 100.0,
            flutter: 0.0,
            cv: Vec::new(),
            abbreviations: true,
            phrase_prediction: false,
            sample_rate: 22050,
            dictionaries: Vec::new(),
            player: None,
        }
    }
}

#[derive(Default)]
struct Request {
    text: Vec<String>,
    spell: bool,
    wav: Option<String>,
    raw: bool,
    commands: bool,
    sd_rate: Option<f64>,
    sd_pitch: Option<f64>,
    sd_volume: Option<f64>,
    list_voices: bool,
    show_settings: bool,
}

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("klattara: {}", message.as_ref());
    std::process::exit(2);
}

fn number(name: &str, value: &str, low: f64, high: f64) -> f64 {
    match value.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => v.clamp(low, high),
        _ => fail(format!("{name} needs a number from {low} to {high}, not \"{value}\"")),
    }
}

fn yes(name: &str, value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "on" | "true" | "1" => true,
        "no" | "off" | "false" | "0" => false,
        _ => fail(format!("{name} is yes or no, not \"{value}\"")),
    }
}

fn find_voice_def(name: &str) -> Option<&'static VoiceDef> {
    let n = name.trim().to_ascii_lowercase();
    VOICES.iter().find(|v| v.name.to_ascii_lowercase() == n || v.full_name.to_ascii_lowercase() == n)
}

/// Checks a list of [:cv] settings, such as "hs 90 br 30", and returns it tidied.
fn checked_cv(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() % 2 != 0 {
        fail(format!("--cv takes pairs of setting and value, such as \"hs 90 br 30\": \"{text}\""));
    }
    let mut out = Vec::new();
    for pair in words.chunks(2) {
        let code = pair[0].to_ascii_lowercase();
        if !CV_CODES.contains(&code.as_str()) {
            fail(format!("no voice setting \"{}\"; the settings are {}", pair[0], CV_CODES.join(" ")));
        }
        let value = pair[1];
        let ok = if code == "ge" {
            matches!(value.to_ascii_lowercase().as_str(), "0" | "1" | "f" | "m")
        } else {
            value.parse::<f64>().map(|v| v.is_finite()).unwrap_or(false)
        };
        if !ok {
            fail(format!("{code} needs a number{}, not \"{value}\"", if code == "ge" { " (or f or m)" } else { "" }));
        }
        out.push(format!("{code} {value}"));
    }
    out.join(" ")
}

impl Settings {
    /// One setting, from the config file or the command line.
    fn set(&mut self, key: &str, value: &str) {
        match key {
            "voice" => {
                if find_voice_def(value).is_none() {
                    fail(format!("no voice \"{value}\"; the voices are {}", voice_names()));
                }
                self.voice = value.trim().to_string();
            }
            "rate" => self.rate = number("rate", value, SLOWEST_WPM, FASTEST_WPM),
            "pitch" => {
                self.pitch = if matches!(value.trim(), "" | "voice" | "own") {
                    None
                } else {
                    Some(number("pitch", value, 40.0, 550.0))
                }
            }
            "inflection" => self.inflection = number("inflection", value, 0.0, 250.0),
            "volume" => self.volume = number("volume", value, 0.0, 200.0),
            "flutter" => self.flutter = number("flutter", value, 0.0, 100.0),
            "abbreviations" => self.abbreviations = yes("abbreviations", value),
            "phrase-prediction" => self.phrase_prediction = yes("phrase-prediction", value),
            "sample-rate" => {
                let rate = number("sample-rate", value, 0.0, 1e6) as u32;
                if !SAMPLE_RATES.contains(&rate) {
                    fail(format!("the sample rate is one of {}", SAMPLE_RATES.map(|r| r.to_string()).join(" ")));
                }
                self.sample_rate = rate;
            }
            "dictionary" => self.dictionaries.push(expand_home(value.trim())),
            "player" => self.player = Some(value.trim().to_string()),
            "cv" => self.cv.push(("all".into(), checked_cv(value))),
            _ if key.starts_with("cv.") => {
                let who = &key[3..];
                let Some(def) = find_voice_def(who) else {
                    fail(format!("no voice \"{who}\" in \"{key}\"; the voices are {}", voice_names()))
                };
                self.cv.push((def.name.to_ascii_lowercase(), checked_cv(value)));
            }
            _ => fail(format!("no setting \"{key}\"")),
        }
    }

    fn load_file(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => fail(format!("could not read {}: {e}", path.display())),
        };
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                fail(format!("{} line {}: expected \"setting = value\"", path.display(), n + 1))
            };
            self.set(key.trim(), value);
        }
    }
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(path),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn default_config() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
        .join("klattara/klattara.conf")
}

fn voice_names() -> String {
    VOICES.iter().map(|v| v.name.to_ascii_lowercase()).collect::<Vec<_>>().join(", ")
}

fn parse_args() -> (Settings, Request) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The config file first, so the command line wins over it.
    let mut config: Option<PathBuf> = Some(default_config());
    let mut explicit_config = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                config = args.get(i + 1).map(|p| expand_home(p));
                explicit_config = true;
                i += 1;
            }
            "--no-config" => config = None,
            "--" => break,
            _ => {}
        }
        i += 1;
    }
    let mut settings = Settings::default();
    if let Some(path) = config {
        if explicit_config || path.exists() {
            settings.load_file(&path);
        }
    }

    let mut request = Request::default();
    let mut i = 0;
    let value = |i: &mut usize, name: &str| -> String {
        *i += 1;
        match args.get(*i) {
            Some(v) => v.clone(),
            None => fail(format!("{name} needs a value")),
        }
    };
    while i < args.len() {
        let arg = args[i].clone();
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--version" => {
                println!("klattara {VERSION}");
                std::process::exit(0);
            }
            "--config" => i += 1,
            "--no-config" => {}
            "-v" | "--voice" => settings.set("voice", &value(&mut i, &arg)),
            "-r" | "--rate" => settings.set("rate", &value(&mut i, &arg)),
            "-p" | "--pitch" => settings.set("pitch", &value(&mut i, &arg)),
            "-i" | "--inflection" => settings.set("inflection", &value(&mut i, &arg)),
            "--volume" => settings.set("volume", &value(&mut i, &arg)),
            "--flutter" => settings.set("flutter", &value(&mut i, &arg)),
            "--cv" => settings.set("cv", &value(&mut i, &arg)),
            "--no-abbreviations" => settings.abbreviations = false,
            "--phrase-prediction" => settings.phrase_prediction = true,
            "--spell" => request.spell = true,
            "-s" | "--sample-rate" => settings.set("sample-rate", &value(&mut i, &arg)),
            "-w" | "--wav" => request.wav = Some(value(&mut i, &arg)),
            "--raw" => request.raw = true,
            "--player" => settings.set("player", &value(&mut i, &arg)),
            "-d" | "--dictionary" => settings.set("dictionary", &value(&mut i, &arg)),
            "--commands" => request.commands = true,
            "--sd-rate" => request.sd_rate = Some(number("--sd-rate", &value(&mut i, &arg), -100.0, 100.0)),
            "--sd-pitch" => request.sd_pitch = Some(number("--sd-pitch", &value(&mut i, &arg), -100.0, 100.0)),
            "--sd-volume" => request.sd_volume = Some(number("--sd-volume", &value(&mut i, &arg), -100.0, 100.0)),
            // Speech Dispatcher passes its voice name: one of ours, or a voice type.
            "--sd-voice" => {
                let v = value(&mut i, &arg);
                if let Some(name) = speechd_voice(&v) {
                    settings.voice = name.to_string();
                }
            }
            "--list-voices" => request.list_voices = true,
            "--show-settings" => request.show_settings = true,
            "--" => {
                request.text.extend(args[i + 1..].iter().cloned());
                break;
            }
            _ if arg.starts_with('-') && arg.len() > 1 => fail(format!("no option \"{arg}\"; see klattara --help")),
            _ => request.text.push(arg),
        }
        i += 1;
    }
    (settings, request)
}

/// Speech Dispatcher's voice: one of Klattara's names, or one of its voice types.
fn speechd_voice(name: &str) -> Option<&'static str> {
    if let Some(def) = find_voice_def(name) {
        return Some(def.name);
    }
    let n = name.trim().to_ascii_lowercase();
    Some(match n.as_str() {
        "male1" => "Steve",
        "male2" => "Gary",
        "male3" => "David",
        "female1" => "Brittany",
        "female2" => "Taylor",
        "female3" => "Brittany",
        "child_male" => "David",
        "child_female" => "Taylor",
        _ => return None,
    })
}

/// Where the audio goes.
enum Output {
    /// PCM written as it is made: to a player (which is then waited for) or to standard output.
    Stream(Option<Child>, Box<dyn Write>),
    Wav { to: String, samples: Vec<i16> },
}

fn find_in_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn start_player(settings: &Settings, rate: u32) -> Output {
    let command = match &settings.player {
        Some(c) => c.replace("{rate}", &rate.to_string()),
        None => {
            if find_in_path("paplay") {
                format!("paplay --raw --format=s16le --rate={rate} --channels=1")
            } else if find_in_path("pw-cat") {
                format!("pw-cat --playback --format=s16 --rate={rate} --channels=1 -")
            } else if find_in_path("aplay") {
                format!("aplay -q -t raw -f S16_LE -r {rate} -c 1 -")
            } else {
                #[cfg(target_os = "freebsd")]
                if let Some(device) = oss::open(rate) {
                    return Output::Stream(None, Box::new(BufWriter::with_capacity(8192, device)));
                }
                fail("found no audio player (paplay, pw-cat or aplay); save a WAV with -w, or name one with --player")
            }
        }
    };
    let mut parts = command.split_whitespace();
    let program = parts.next().unwrap_or_else(|| fail("the player command is empty"));
    match Command::new(program).args(parts).stdin(Stdio::piped()).spawn() {
        Ok(mut child) => {
            let stdin = child.stdin.take().expect("piped");
            Output::Stream(Some(child), Box::new(BufWriter::with_capacity(8192, stdin)))
        }
        Err(e) => fail(format!("could not start the player \"{program}\": {e}")),
    }
}

/// FreeBSD's own sound system, OSS: /dev/dsp, set to 16-bit little-endian mono at
/// the rate, for when none of the Linux players is installed (as on a plain FreeBSD).
#[cfg(target_os = "freebsd")]
mod oss {
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::os::raw::{c_int, c_ulong};

    unsafe extern "C" {
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }

    // _IOWR('P', n, int) on FreeBSD.
    const SNDCTL_DSP_SPEED: c_ulong = 0xC004_5002;
    const SNDCTL_DSP_SETFMT: c_ulong = 0xC004_5005;
    const SNDCTL_DSP_CHANNELS: c_ulong = 0xC004_5006;
    const AFMT_S16_LE: c_int = 0x10;

    pub fn open(rate: u32) -> Option<File> {
        let device = OpenOptions::new().write(true).open("/dev/dsp").ok()?;
        let fd = device.as_raw_fd();
        let set = |request: c_ulong, value: c_int| -> Option<c_int> {
            let mut v = value;
            (unsafe { ioctl(fd, request, &mut v as *mut c_int) } != -1).then_some(v)
        };
        (set(SNDCTL_DSP_SETFMT, AFMT_S16_LE)? == AFMT_S16_LE).then_some(())?;
        (set(SNDCTL_DSP_CHANNELS, 1)? == 1).then_some(())?;
        // The device may round the rate; FreeBSD's sound system converts any rate.
        set(SNDCTL_DSP_SPEED, rate as c_int)?;
        Some(device)
    }
}

fn wav_bytes(samples: &[i16], rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn main() -> ExitCode {
    let (settings, request) = parse_args();

    if request.list_voices {
        for v in VOICES {
            let gender = if matches!(v.gender, engine::Gender::Female) { "female" } else { "male" };
            println!("{:<10} {:<16} {gender}, {} Hz", v.name.to_ascii_lowercase(), v.full_name, v.pitch);
        }
        return ExitCode::SUCCESS;
    }

    let def = find_voice_def(&settings.voice).unwrap_or(&VOICES[0]);
    let key = def.name.to_ascii_lowercase();

    // Speech Dispatcher's rate scales the rate by up to ten times either way;
    // its pitch by up to an octave; its volume runs to the settings' volume.
    let rate = (settings.rate * request.sd_rate.map_or(1.0, |r| 10f64.powf(r / 100.0))).clamp(SLOWEST_WPM, FASTEST_WPM);
    let base_pitch = settings.pitch.unwrap_or(def.pitch);
    let pitch = (base_pitch * request.sd_pitch.map_or(1.0, |p| 2f64.powf(p / 100.0))).clamp(40.0, 550.0);
    let volume = settings.volume * request.sd_volume.map_or(1.0, |v| (v + 100.0) / 200.0);

    let mut voice = Voice::from(def);
    let mut setup = format!("[:cv fl {}]", settings.flutter);
    for (who, cv) in &settings.cv {
        if who == "all" || *who == key {
            setup.push_str(&format!("[:cv {cv}]"));
        }
    }
    setup.push_str(&format!("[:speed {}]", rate.round()));
    follow_commands(&setup, &mut voice);
    voice.pitch = pitch;
    voice.inflection = settings.inflection / 100.0;
    voice.volume = volume / 100.0;
    voice.expand_abbreviations = settings.abbreviations;
    voice.phrase_prediction = settings.phrase_prediction;

    let mut dictionary = Dictionary::new();
    for path in &settings.dictionaries {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let r = dictionary.load(&text);
                if r.rejected > 0 {
                    eprintln!(
                        "klattara: {}: {} lines with mistakes left out, the first at line {}",
                        path.display(),
                        r.rejected,
                        r.first_rejected_line
                    );
                }
            }
            Err(e) => eprintln!("klattara: could not read the dictionary {}: {e}", path.display()),
        }
    }

    if request.show_settings {
        println!("voice {} ({}), rate {} wpm, pitch {} Hz, inflection {}%, volume {}%, flutter {}%",
            key, def.full_name, rate.round(), pitch, settings.inflection, volume.round(), settings.flutter);
        println!("sample rate {} Hz, abbreviations {}, phrase prediction {}, {} dictionary entries",
            settings.sample_rate, settings.abbreviations, settings.phrase_prediction, dictionary.len());
        for (who, cv) in &settings.cv {
            println!("cv for {who}: {cv}");
        }
        return ExitCode::SUCCESS;
    }

    let mut text = request.text.join(" ");
    if text.is_empty() {
        if let Err(e) = io::stdin().read_to_string(&mut text) {
            fail(format!("could not read standard input: {e}"));
        }
    }
    let prepared = if request.commands { text } else { dictionary.apply(&text) };
    let reading = if request.spell { Reading::Spelled } else { Reading::Text };
    let rate_hz = settings.sample_rate;

    let mut output = if let Some(to) = request.wav.clone() {
        Output::Wav { to, samples: Vec::new() }
    } else if request.raw {
        Output::Stream(None, Box::new(BufWriter::with_capacity(8192, io::stdout())))
    } else {
        start_player(&settings, rate_hz)
    };

    let mut pcm: Vec<u8> = Vec::with_capacity(8192);
    let mut broken = false;
    let result = speak_with(&prepared, reading, &mut voice, rate_hz, &mut |chunk: &[f32]| {
        let samples = chunk.iter().map(|&x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16);
        match &mut output {
            Output::Wav { samples: all, .. } => all.extend(samples),
            Output::Stream(_, w) => {
                // A player or reader that has gone (stopped by Speech Dispatcher,
                // say) ends the speaking at once.
                if broken {
                    return false;
                }
                pcm.clear();
                pcm.extend(samples.flat_map(i16::to_le_bytes));
                if w.write_all(&pcm).and_then(|_| w.flush()).is_err() {
                    broken = true;
                    return false;
                }
            }
        }
        true
    });
    if let Err(e) = result {
        eprintln!("klattara: {e:?}");
    }

    match output {
        Output::Wav { to, samples } => {
            let bytes = wav_bytes(&samples, rate_hz);
            let written = if to == "-" { io::stdout().write_all(&bytes) } else { std::fs::write(&to, &bytes) };
            if let Err(e) = written {
                fail(format!("could not write {to}: {e}"));
            }
        }
        Output::Stream(child, mut w) => {
            let _ = w.flush();
            drop(w);
            if let Some(mut child) = child {
                let _ = child.wait();
            }
        }
    }
    ExitCode::SUCCESS
}
