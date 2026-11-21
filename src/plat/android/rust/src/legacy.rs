//! Android 2.2 to 3.2: the settings the engine plugin speaks with.
//!
//! On those versions a third-party voice is a native plugin that Android's own
//! speech service loads (jni/legacy_plugin.c), so it runs in that service's
//! process, not Klattara's, and cannot read the app's settings. The app writes them
//! to a file instead, files/klattara.conf, readable by the service, in the same
//! "key = value" form as Klattara for Linux, and the plugin reads it again whenever
//! it changes. Anything missing, unreadable or wrong is left at its default, never
//! an error: the plugin runs inside Android's speech service and must not stop it.

use crate::dictionary::Dictionary;
use crate::ffi::{
    KlattaraEngine, klattara_set_abbreviations, klattara_set_inflection, klattara_set_phrase_prediction,
    klattara_set_pitch, klattara_set_voice, klattara_set_volume, klattara_speak,
};
use engine::{FASTEST_WPM, SLOWEST_WPM, VOICES};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::SystemTime;

/// Every rate Android 2.2 to 3.2 plays (its audio output stops at 48 kHz).
const SAMPLE_RATES: [u32; 15] =
    [8000, 10000, 11000, 11025, 12000, 16000, 20000, 22000, 22050, 24000, 32000, 40000, 44000, 44100, 48000];

const CV_CODES: [&str; 25] = [
    "pi", "in", "hs", "rg", "br", "rc", "sm", "ge", "f4", "f5", "f6", "b4", "b5", "b6", "fl", "vvl", "fvl", "avl",
    "nvl", "fa", "ha", "st", "qk", "lx", "oq",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub voice: usize,
    pub rate: f64,
    pub follow_rate: bool,
    pub pitch: Option<f64>,
    pub follow_pitch: bool,
    pub inflection: f64,
    pub volume: f64,
    pub flutter: f64,
    pub abbreviations: bool,
    pub phrase_prediction: bool,
    pub sample_rate: u32,
    /// [:cv] settings for one voice (its index) or all (None), checked.
    pub cv: Vec<(Option<usize>, String)>,
    pub dictionaries: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            voice: 0,
            rate: 190.0,
            follow_rate: true,
            pitch: None,
            follow_pitch: true,
            inflection: 100.0,
            volume: 100.0,
            flutter: 0.0,
            abbreviations: true,
            phrase_prediction: false,
            sample_rate: 22050,
            cv: Vec::new(),
            dictionaries: Vec::new(),
        }
    }
}

fn voice_index(name: &str) -> Option<usize> {
    let n = name.trim().to_ascii_lowercase();
    VOICES.iter().position(|v| v.name.to_ascii_lowercase() == n || v.full_name.to_ascii_lowercase() == n)
}

fn number(value: &str, low: f64, high: f64) -> Option<f64> {
    value.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(low, high))
}

fn yes(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "on" | "true" | "1" => Some(true),
        "no" | "off" | "false" | "0" => Some(false),
        _ => None,
    }
}

/// [:cv] settings, kept only if every pair is a known setting and a number.
fn checked_cv(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || words.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::new();
    for pair in words.chunks(2) {
        let code = pair[0].to_ascii_lowercase();
        if !CV_CODES.contains(&code.as_str()) {
            return None;
        }
        let ok = if code == "ge" {
            matches!(pair[1].to_ascii_lowercase().as_str(), "0" | "1" | "f" | "m")
        } else {
            pair[1].parse::<f64>().map(|v| v.is_finite()).unwrap_or(false)
        };
        if !ok {
            return None;
        }
        out.push(format!("{code} {}", pair[1]));
    }
    Some(out.join(" "))
}

impl Config {
    /// Reads the settings file's text; what is missing or wrong stays at its default.
    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "voice" => c.voice = voice_index(value).unwrap_or(c.voice),
                "rate" => c.rate = number(value, SLOWEST_WPM, FASTEST_WPM).unwrap_or(c.rate),
                "follow-rate" => c.follow_rate = yes(value).unwrap_or(c.follow_rate),
                "pitch" => c.pitch = number(value, 40.0, 550.0),
                "follow-pitch" => c.follow_pitch = yes(value).unwrap_or(c.follow_pitch),
                "inflection" => c.inflection = number(value, 0.0, 250.0).unwrap_or(c.inflection),
                "volume" => c.volume = number(value, 0.0, 200.0).unwrap_or(c.volume),
                "flutter" => c.flutter = number(value, 0.0, 100.0).unwrap_or(c.flutter),
                "abbreviations" => c.abbreviations = yes(value).unwrap_or(c.abbreviations),
                "phrase-prediction" => c.phrase_prediction = yes(value).unwrap_or(c.phrase_prediction),
                "sample-rate" => {
                    if let Some(r) = number(value, 0.0, 1e6).map(|r| r as u32).filter(|r| SAMPLE_RATES.contains(r)) {
                        c.sample_rate = r;
                    }
                }
                "dictionary" => {
                    if !value.is_empty() {
                        c.dictionaries.push(value.to_string());
                    }
                }
                "cv" => {
                    if let Some(cv) = checked_cv(value) {
                        c.cv.push((None, cv));
                    }
                }
                _ => {
                    if let Some(who) = key.strip_prefix("cv.") {
                        if let (Some(i), Some(cv)) = (voice_index(who), checked_cv(value)) {
                            c.cv.push((Some(i), cv));
                        }
                    }
                }
            }
        }
        c
    }
}

/// What the plugin keeps: the settings file, what it last said, and the
/// dictionaries it names.
pub struct Legacy {
    path: Option<String>,
    stamp: Option<(SystemTime, u64)>,
    config: Config,
    dictionary: Dictionary,
}

impl Legacy {
    fn refresh(&mut self) {
        let Some(path) = &self.path else { return };
        let stamp = std::fs::metadata(path).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
        if stamp.is_some() && stamp == self.stamp {
            return;
        }
        self.stamp = stamp;
        self.config = std::fs::read_to_string(path).map(|t| Config::parse(&t)).unwrap_or_default();
        let mut dictionary = Dictionary::new();
        for file in &self.config.dictionaries {
            if let Ok(text) = std::fs::read_to_string(file) {
                dictionary.load(&text);
            }
        }
        self.dictionary = dictionary;
    }
}

/// A new plugin state, reading its settings from `path` (null: defaults only).
///
/// # Safety
/// `path` must be null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_legacy_new(path: *const c_char) -> *mut Legacy {
    let path = if path.is_null() { None } else { Some(unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned()) };
    let mut l = Legacy { path, stamp: None, config: Config::default(), dictionary: Dictionary::new() };
    l.refresh();
    Box::into_raw(Box::new(l))
}

/// # Safety
/// `legacy` must be null or from [`klattara_legacy_new`], not used after.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_legacy_free(legacy: *mut Legacy) {
    if !legacy.is_null() {
        drop(unsafe { Box::from_raw(legacy) });
    }
}

/// Reads the settings file again if it has changed, and returns the sample rate
/// to speak at.
///
/// # Safety
/// `legacy` from [`klattara_legacy_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_legacy_refresh(legacy: *mut Legacy) -> c_int {
    let Some(l) = (unsafe { legacy.as_mut() }) else { return 22050 };
    catch_unwind(AssertUnwindSafe(|| {
        l.refresh();
        l.config.sample_rate as c_int
    }))
    .unwrap_or(22050)
}

unsafe extern "C" fn discard(_: *mut c_void, _: *const i16, _: c_int) -> c_int {
    0
}

/// Sets an engine up for the next utterance from the settings, with Android's
/// rate and pitch (100 is normal) applied if the settings follow them.
///
/// # Safety
/// `legacy` from [`klattara_legacy_new`], `engine` a live engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_legacy_prepare(
    legacy: *mut Legacy,
    engine: *mut KlattaraEngine,
    system_rate: c_int,
    system_pitch: c_int,
) -> c_int {
    let Some(l) = (unsafe { legacy.as_mut() }) else { return -1 };
    if engine.is_null() {
        return -1;
    }
    let c = &l.config;
    let voice = c.voice.min(VOICES.len() - 1);
    let rate_scale = if c.follow_rate && system_rate > 0 { f64::from(system_rate) / 100.0 } else { 1.0 };
    let pitch_scale = if c.follow_pitch && system_pitch > 0 { f64::from(system_pitch) / 100.0 } else { 1.0 };
    let wpm = (c.rate * rate_scale).clamp(SLOWEST_WPM, FASTEST_WPM).round();
    let mut commands = format!("[:phone klattara disable][:cv fl {}]", c.flutter);
    for (who, cv) in &c.cv {
        if who.is_none() || *who == Some(voice) {
            commands.push_str(&format!("[:cv {cv}]"));
        }
    }
    commands.push_str(&format!("[:speed {wpm}]"));
    let commands = CString::new(commands).unwrap_or_default();
    let pitch = (c.pitch.unwrap_or(VOICES[voice].pitch) * pitch_scale).clamp(40.0, 550.0);
    // SAFETY: a live engine, and a NUL-terminated string.
    unsafe {
        klattara_set_voice(engine, voice as c_int);
        klattara_speak(engine, commands.as_ptr(), 0, Some(discard), std::ptr::null_mut());
        klattara_set_pitch(engine, pitch);
        klattara_set_inflection(engine, c.inflection / 100.0);
        klattara_set_volume(engine, c.volume / 100.0);
        klattara_set_abbreviations(engine, c_int::from(c.abbreviations));
        klattara_set_phrase_prediction(engine, c_int::from(c.phrase_prediction));
    }
    0
}

/// The text with the dictionaries applied and brackets made safe, as a new
/// NUL-terminated string to free with [`klattara_legacy_free_text`]; null on failure.
///
/// # Safety
/// `legacy` from [`klattara_legacy_new`], `text` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_legacy_text(legacy: *const Legacy, text: *const c_char) -> *mut c_char {
    let Some(l) = (unsafe { legacy.as_ref() }) else { return std::ptr::null_mut() };
    if text.is_null() {
        return std::ptr::null_mut();
    }
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    catch_unwind(AssertUnwindSafe(|| {
        CString::new(l.dictionary.apply(&text).replace('\0', " ")).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// # Safety
/// `text` null or from [`klattara_legacy_text`], not used after.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_legacy_free_text(text: *mut c_char) {
    if !text.is_null() {
        drop(unsafe { CString::from_raw(text) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_read_and_mistakes_are_ignored() {
        let c = Config::parse(
            "# comment\nvoice = gary\nrate = 400\nfollow-rate = no\npitch = 9999\nflutter = 20\n\
             sample-rate = 96000\ncv.taylor = hs 90 br 30\ncv = sm 10 nonsense\nbogus line\nvolume = loud\n",
        );
        assert_eq!(c.voice, 2);
        assert_eq!(c.rate, 400.0);
        assert!(!c.follow_rate);
        assert_eq!(c.pitch, Some(550.0));
        assert_eq!(c.flutter, 20.0);
        assert_eq!(c.sample_rate, 22050, "96 kHz is past what Android 2.x plays");
        assert_eq!(c.cv, vec![(Some(4), "hs 90 br 30".to_string())], "a bad cv line is left out");
        assert_eq!(c.volume, 100.0);
    }

    #[test]
    fn an_empty_or_missing_file_gives_the_defaults() {
        assert_eq!(Config::parse(""), Config::default());
        let l = unsafe { klattara_legacy_new(c"/nonexistent/klattara.conf".as_ptr()) };
        assert_eq!(unsafe { klattara_legacy_refresh(l) }, 22050);
        unsafe { klattara_legacy_free(l) };
    }
}
