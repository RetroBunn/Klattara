"""Tests the NVDA driver outside NVDA, against the real klattara.dll and NVDA's
own Python code.

NVDA is 32-bit before 2026 and 64-bit from then on, so each DLL is tested on the
NVDA that will load it: a 64-bit Python tests the 64-bit DLL on the clone of
https://github.com/nvaccess/nvda in the nvda folder of the project, and a 32-bit
Python tests the 32-bit DLL on NVDA 2025.1, in the nvda-release-2025.1 folder.
If that folder isn't there, the 32-bit DLL is tested on the nvda folder instead.

The driver runs on NVDA's real synthDriverHandler, driverHandler,
autoSettingsUtils, speech.commands, extensionPoints and baseObject. Only what
needs a running NVDA is stood in for: the log, the configuration store,
translations, the settings ring and voice dictionaries, and the audio player,
which needs NVDA's compiled helper library. The stand-in player has the real
one's signature, checked against NVDA's nvwave.py, and records what it is given.

Usage, from the project folder, after building the DLLs:
    py -3-64 src/plat/nvda/test_driver.py
    py -3-32 src/plat/nvda/test_driver.py
"""

import ast
import builtins
import ctypes
import importlib.util
import sys
import threading
import time
import types
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DRIVER = ROOT / "src" / "plat" / "nvda" / "addon" / "synthDrivers" / "klattara" / "__init__.py"
NVDA_SOURCE = ROOT / "nvda" / "source"
if ctypes.sizeof(ctypes.c_void_p) == 8:
    ARCHITECTURE = "x64"
    DLL = ROOT / "target" / "release" / "klattara.dll"
else:
    ARCHITECTURE = "x86"
    DLL = ROOT / "target" / "i686-pc-windows-msvc" / "release" / "klattara.dll"
    if (ROOT / "nvda-release-2025.1" / "source").is_dir():
        NVDA_SOURCE = ROOT / "nvda-release-2025.1" / "source"
    else:
        print("warning: no nvda-release-2025.1 folder, so the 32-bit DLL is tested on current NVDA, not on 32-bit NVDA's code")

events = []
lock = threading.Lock()

# The rates the driver must offer, in Hz: every rate the engine is tuned for.
EXPECTED_SAMPLE_RATES = (
    8000, 10000, 11000, 11025, 12000, 16000, 20000, 22000, 22050, 24000,
    32000, 40000, 44000, 44100, 48000, 64000, 80000, 88000, 88200, 96000,
)


def record(*event):
    with lock:
        events.append(event)


def module(name, **attributes):
    stub = types.ModuleType(name)
    stub.__dict__.update(attributes)
    sys.modules[name] = stub
    return stub


class Log:
    """Stands in for logHandler.log; errors are recorded."""

    def error(self, message, *args, **kwargs):
        record("error", message)

    def exception(self, message="", *args, **kwargs):
        record("error", message)

    def debugWarning(self, message, *args, **kwargs):
        record("debugWarning", message)

    def debug(self, *args, **kwargs):
        pass

    info = warning = io = debug


class SettingsRing:
    """Stands in for synthSettingsRing.SynthSettingsRing."""

    def __init__(self, synth):
        pass

    def updateSupportedSettings(self, synth):
        pass


class WavePlayer:
    """Stands in for nvwave.WavePlayer, with the same signature."""

    def __init__(self, channels, samplesPerSec, bitsPerSample, outputDevice="default", wantDucking=True, purpose=None):
        assert (channels, bitsPerSample) == (1, 16)
        assert samplesPerSec in EXPECTED_SAMPLE_RATES
        assert outputDevice == config.conf["audio"]["outputDevice"]
        record("playerRate", samplesPerSec)

    def feed(self, data, size=None, onDone=None):
        record("audio", size if size is not None else len(data))
        if onDone:
            onDone()

    def idle(self):
        pass

    def enableTrimmingLeadingSilence(self, enable):
        self.trimming = enable
        record("trimming", enable)

    def stop(self):
        record("stop")

    def pause(self, switch):
        record("pause", switch)

    def close(self):
        self.stop()


def validate(spec, value):
    """Checks a value against a setting's config spec as NVDA's configuration
    does when a value is assigned (config/__init__.py, AggregatedSection
    __setitem__: validator.check(spec, val)), with configobj's own validator
    if it is installed, and otherwise by its rules for integer, boolean and
    string: an integer must be an int (a float such as 110.0 is "the wrong
    type") and within min and max."""
    try:
        from configobj.validate import Validator
    except ImportError:
        pass
    else:
        return Validator().check(spec, value)
    kind, _, args = spec.partition("(")
    params = dict(arg.split("=", 1) for arg in args.rstrip(")").split(",") if "=" in arg)
    if kind == "integer":
        if not isinstance(value, (int, str)):
            raise ValueError(f'the value "{value}" is of the wrong type')
        value = int(value)
        if "min" in params and value < int(params["min"]):
            raise ValueError(f'the value "{value}" is too small')
        if "max" in params and value > int(params["max"]):
            raise ValueError(f'the value "{value}" is too big')
    elif kind == "boolean" and not isinstance(value, (bool, int, str)):
        raise ValueError(f'the value "{value}" is of the wrong type')
    elif kind == "string" and not isinstance(value, str) and value is not None:
        raise ValueError(f'the value "{value}" is of the wrong type')
    return value


class ValidatingSection(dict):
    """Stands in for the synth's section of NVDA's configuration, and refuses a
    value its setting's spec does not allow, as NVDA does. A refused value
    raises out of saveSettings, and in NVDA that leaves the settings dialog's
    OK and Apply doing nothing."""

    def __init__(self, settings):
        super().__init__()
        self.specs = {setting.id: setting.configSpec for setting in settings}

    def __setitem__(self, key, value):
        if key in self.specs:
            value = validate(self.specs[key], value)
        super().__setitem__(key, value)


def load_nvda():
    """Imports NVDA's real modules, with stand-ins for its runtime."""
    if not (NVDA_SOURCE / "synthDriverHandler.py").is_file():
        raise SystemExit(f"NVDA's source is needed in {NVDA_SOURCE.parent}: git clone https://github.com/nvaccess/nvda")
    sys.path.insert(0, str(NVDA_SOURCE))
    # NVDA installs these translation functions as builtins.
    builtins._ = lambda text: text
    builtins.pgettext = lambda context, text: text
    module("logHandler", log=Log())
    module("NVDAState", _allowDeprecatedAPI=lambda: True)
    import extensionPoints

    global config
    config = module(
        "config",
        conf={
            "audio": {"outputDevice": "default"},
            "speech": {},
            "debugLog": {"garbageHandler": False, "synthDriver": False},
        },
        pre_configSave=extensionPoints.Action(),
        post_configProfileSwitch=extensionPoints.Action(),
    )
    module("globalVars", settingsRing=None)
    module("synthSettingsRing", SynthSettingsRing=SettingsRing)
    module("languageHandler", getLanguageDescription=lambda language: language)
    module("speechDictHandler", loadVoiceDict=lambda synth: None)
    module("synthDrivers", __path__=[])
    module("nvwave", WavePlayer=WavePlayer)
    # Without an add-on's translations, NVDA's _ stays in use.
    module("addonHandler", initTranslation=lambda: None)
    # Load speech.commands without speech/__init__.py, which starts all of
    # NVDA's speech handling.
    module("speech", __path__=[str(NVDA_SOURCE / "speech")])
    import speech.commands
    import synthDriverHandler

    return synthDriverHandler, speech.commands


def load_driver():
    spec = importlib.util.spec_from_file_location("klattara_driver", DRIVER)
    driver = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(driver)
    check(Path(driver.DLL_PATH).parent.name == ARCHITECTURE, f"this NVDA loads the {ARCHITECTURE} DLL")
    driver.DLL_PATH = str(DLL)
    return driver


def check_stand_ins_match_nvda():
    """The stand-in player proves nothing unless the real one offers the same,
    so compare it with this NVDA's own code, and the output device setting too."""
    tree = ast.parse((NVDA_SOURCE / "nvwave.py").read_text(encoding="utf-8"))
    player = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "WavePlayer")
    methods = {node.name: [arg.arg for arg in node.args.args] for node in player.body if isinstance(node, ast.FunctionDef)}
    expected = ["self", "channels", "samplesPerSec", "bitsPerSample", "outputDevice"]
    check(methods["__init__"][:5] == expected, f"NVDA's player opens as the driver expects: {methods['__init__']}")
    check(methods["feed"][:4] == ["self", "data", "size", "onDone"], f"NVDA's player is fed as the driver expects: {methods['feed']}")
    check({"idle", "stop", "pause", "close", "enableTrimmingLeadingSilence"} <= methods.keys(), "NVDA's player has the other methods the driver calls")
    config_spec = (NVDA_SOURCE / "config" / "configSpec.py").read_text(encoding="utf-8")
    check("[audio]\n\toutputDevice" in config_spec, "NVDA keeps the output device in its audio settings")
    # The pitch slider moves with the voice only if NVDA looks at the settings again
    # after a change of voice, in the dialog and in the settings ring.
    dialogs = (NVDA_SOURCE / "gui" / "settingsDialogs.py").read_text(encoding="utf-8")
    check("self.container.updateDriverSettings(changedSetting=self.setting.id)" in dialogs, "NVDA's dialog updates its sliders after a change of voice")
    ring = (NVDA_SOURCE / "synthSettingsRing.py").read_text(encoding="utf-8")
    check("self.synth.saveSettings()" in ring, "NVDA's settings ring saves the settings after a change of voice")
    # A plain DriverSetting is shown as a combo box, with its options from
    # available{Id}s on the driver: that is how the sample rate is offered.
    check(
        'f"available{setting.id.capitalize()}s"' in dialogs
        and "settingMaker = self._makeStringSettingControl" in dialogs,
        "NVDA's dialog shows a plain setting as a combo box from available{Id}s",
    )


def wait_for(kind, timeout=10.0):
    end = time.time() + timeout
    while time.time() < end:
        with lock:
            if any(e[0] == kind for e in events):
                return True
        time.sleep(0.01)
    return False


def audio_bytes():
    with lock:
        return sum(e[1] for e in events if e[0] == "audio")


def speak_and_measure(synth, sequence):
    events.clear()
    synth.speak(sequence)
    check(wait_for("done"), f"speech finishes: {sequence!r}")
    return audio_bytes()


def on_index_reached(synth, index):
    record("index", index)


def on_done_speaking(synth):
    record("done", None)


def check(condition, message):
    if not condition:
        raise AssertionError(message)
    print(f"ok: {message}")


def main():
    # NVDA 2025.1 runs on Python 3.11, whichever Python runs these tests.
    ast.parse(DRIVER.read_text(encoding="utf-8"), feature_version=(3, 11))
    print("ok: the driver's syntax suits Python 3.11")
    check_stand_ins_match_nvda()
    synthDriverHandler, commands = load_nvda()
    # NVDA's extension points hold handlers weakly; these are module functions.
    synthDriverHandler.synthIndexReached.register(on_index_reached)
    synthDriverHandler.synthDoneSpeaking.register(on_done_speaking)
    driver = load_driver()
    SynthDriver = driver.SynthDriver
    check(SynthDriver.check(), "the driver finds klattara.dll")

    # The sliders spread evenly; NVDA is given the defaults where normal lands.
    specs = {s.id: s.configSpec for s in SynthDriver.supportedSettings}
    check(specs["rate"] == "integer(default=15,min=0,max=100)", f"rate defaults to 15: {specs['rate']}")
    check(specs["pitch"] == "integer(default=14,min=0,max=100)", f"pitch defaults to 14: {specs['pitch']}")
    check(specs["inflection"] == "integer(default=40,min=0,max=100)", f"inflection defaults to 40: {specs['inflection']}")
    to_param = SynthDriver._percentToParam
    check(abs(to_param(15, driver.MIN_RATE, driver.MAX_RATE) - 100) <= 2, "rate 15 is normal speed")
    check((to_param(0, driver.MIN_RATE, driver.MAX_RATE), to_param(100, driver.MIN_RATE, driver.MAX_RATE)) == (29, 500), "rate spans about 50 to 980 words per minute")
    check((to_param(0, driver.MIN_PITCH, driver.MAX_PITCH), to_param(100, driver.MIN_PITCH, driver.MAX_PITCH)) == (40, 550), "pitch spans 40 to 550 Hz")
    check(to_param(40, driver.MIN_INFLECTION, driver.MAX_INFLECTION) == 100, "inflection 40 is normal, 100 is 250%")
    check(
        specs["expandAbbreviations"] == "boolean(default=True)",
        f"abbreviations are expanded by default: {specs['expandAbbreviations']}",
    )
    check(
        specs["phrasePrediction"] == "boolean(default=False)",
        f"phrase prediction is off by default: {specs['phrasePrediction']}",
    )
    # The voice-quality settings: flutter is off, the rest default to the
    # first voice's own values.
    check(specs["flutter"] == "integer(default=0,min=0,max=100)", f"flutter is off by default: {specs['flutter']}")
    check(
        specs["headSize"] == "integer(default=100,min=64,max=135)",
        f"head size defaults to Steve's 100: {specs['headSize']}",
    )
    check(
        specs["roughness"] == "integer(default=0,min=0,max=100)",
        f"roughness defaults to Steve's 0: {specs['roughness']}",
    )
    check(
        specs["breathiness"] == "integer(default=0,min=0,max=72)",
        f"breathiness defaults to Steve's 0: {specs['breathiness']}",
    )
    check(
        specs["openQuotient"] == "integer(default=55,min=20,max=90)",
        f"open quotient defaults to Steve's 55: {specs['openQuotient']}",
    )
    check(
        specs["smoothness"] == "integer(default=0,min=0,max=100)",
        f"smoothness defaults to Steve's 0: {specs['smoothness']}",
    )
    check(
        specs["femaleVocalTract"] == "boolean(default=False)",
        f"the female vocal tract is off by default: {specs['femaleVocalTract']}",
    )
    check(
        specs["baselineFall"] == "integer(default=18,min=0,max=90)",
        f"baseline fall defaults to Steve's 18: {specs['baselineFall']}",
    )
    check(
        specs["hatRise"] == "integer(default=18,min=0,max=100)",
        f"hat rise defaults to Steve's 18: {specs['hatRise']}",
    )
    check(
        specs["stressRise"] == "integer(default=32,min=0,max=100)",
        f"stress rise defaults to Steve's 32: {specs['stressRise']}",
    )
    check(
        specs["quickness"] == "integer(default=40,min=0,max=100)",
        f"quickness defaults to Steve's 40: {specs['quickness']}",
    )
    check(
        specs["laxBreathiness"] == "integer(default=0,min=0,max=100)",
        f"lax breathiness defaults to Steve's 0: {specs['laxBreathiness']}",
    )
    check(
        specs["sampleRate"] == "string(default=22050)",
        f"the sample rate is a choice defaulting to 22050 Hz: {specs['sampleRate']}",
    )
    check(
        specs["_pitchHertz"] == "integer(default=110,min=40,max=550)",
        f"pitch in hertz spans the engine's 40 to 550 Hz: {specs['_pitchHertz']}",
    )
    check(
        specs["_pitchInHertz"] == "boolean(default=False)",
        f"pitch in hertz is off by default: {specs['_pitchInHertz']}",
    )
    check(
        specs["_rateWpm"] == "integer(default=190,min=30,max=1920)",
        f"rate in words per minute spans 30 to 1920: {specs['_rateWpm']}",
    )
    check(
        specs["_rateInWpm"] == "boolean(default=False)",
        f"rate in words per minute is off by default: {specs['_rateInWpm']}",
    )
    check(
        all(s.id.startswith("_") for s in (
            SynthDriver._rateWpmSetting, SynthDriver._rateInWpmSetting,
            SynthDriver._pitchHertzSetting, SynthDriver._pitchInHertzSetting,
        )),
        "the precise pitch and rate are hidden from Speech settings: their ids start with an underscore, so NVDA's dialog skips them and the Klattara panel owns them",
    )
    check(
        isinstance(SynthDriver._sampleRateSetting, driver.DriverSetting)
        and not isinstance(SynthDriver._sampleRateSetting, driver.NumericDriverSetting),
        "the sample rate is a plain setting, so NVDA shows it as a combo box, not a slider",
    )
    check(
        tuple(driver.SAMPLE_RATES) == EXPECTED_SAMPLE_RATES,
        f"the driver offers the twenty supported rates: {driver.SAMPLE_RATES}",
    )

    check(specs["voice"] == "string(default=None)", f"the voice is a setting NVDA keeps: {specs['voice']}")

    synth = SynthDriver()
    # What NVDA does on loading a synth for the first time: save the defaults.
    config.conf["speech"][synth.name] = ValidatingSection(synth.supportedSettings)
    synth.saveSettings()
    synthDriverHandler._curSynth = synth
    check(config.conf["speech"]["klattara"]["rate"] == 15, "the default settings are saved to NVDA's configuration")
    check(config.conf["speech"]["klattara"]["voice"] == "steve", "Steve is the default voice")
    try:
        # Speech with index marks, in order, then done.
        events.clear()
        synth.speak(["Hello, world.  ", commands.IndexCommand(1), "This is Klattara.  ", commands.IndexCommand(2)])
        check(wait_for("done"), "speech finishes and reports done")
        with lock:
            order = [e for e in events if e[0] in ("index", "done")]
        check(order == [("index", 1), ("index", 2), ("done", None)], f"index marks arrive in order: {order}")
        normal = audio_bytes()
        check(normal > 22050, f"audio was played ({normal} bytes)")
        check(
            synth._player.trimming is False,
            "leading-silence trimming is off, so no utterance loses its beginning",
        )

        # A faster rate gives less audio for the same text.
        synth.rate = 50
        fast = speak_and_measure(synth, ["Hello, world.  ", "This is Klattara.  "])
        check(fast < 0.5 * normal, f"rate 50 is much shorter ({fast} against {normal} bytes)")
        synth.rate = 15

        # No utterance is cut off, whatever the voice, the rate, or the
        # sample rate: speaking a text twice gives twice the audio of
        # speaking it once, and a faster rate gives less audio but never
        # less than the rate says it should. A truncated utterance would
        # break the doubling, whatever cut it off.
        cutoff_text = "Hello, world. This is Klattara, a speech synthesizer. "
        for voice in ("steve", "brittany", "gary", "david", "taylor"):
            synth.voice = voice
            base = None
            for rate in (15, 50, 100):
                synth.rate = rate
                once = speak_and_measure(synth, [cutoff_text])
                twice = speak_and_measure(synth, [cutoff_text + cutoff_text])
                check(
                    1.9 < twice / once < 2.1,
                    f"{voice} at rate {rate}: a doubled text is doubled audio ({twice} against {once} bytes)",
                )
                if base is None:
                    base = once
                elif rate == 50:
                    check(
                        once < 0.5 * base,
                        f"{voice} at rate 50 is much shorter than at rate 15 ({once} against {base} bytes)",
                    )
        synth.voice = "steve"
        synth.rate = 50
        for sample_rate in EXPECTED_SAMPLE_RATES:
            synth.sampleRate = str(sample_rate)
            once = speak_and_measure(synth, [cutoff_text])
            twice = speak_and_measure(synth, [cutoff_text + cutoff_text])
            check(
                1.9 < twice / once < 2.1,
                f"Steve at rate 50 and {sample_rate} Hz: a doubled text is doubled audio ({twice} against {once} bytes)",
            )
        synth.sampleRate = "22050"
        synth.rate = 15

        # Cancelling stops at once: no more index marks, no done.
        events.clear()
        long_text = " ".join(["The quick brown fox jumps over the lazy dog."] * 40)
        synth.speak([long_text, commands.IndexCommand(7)])
        synth.cancel()
        time.sleep(0.5)
        with lock:
            late = [e for e in events if e[0] in ("index", "done")]
            stops = [e for e in events if e[0] == "stop"]
        check(not late and stops, "cancel stops speech without reporting index or done")
        check(audio_bytes() < 22050 * 2 * 5, f"cancelled speech played little audio ({audio_bytes()} bytes)")
        speak_and_measure(synth, ["Back again.  "])

        # Character mode spells: "a" is said as the letter name, which is
        # longer than the reduced article.
        letter = speak_and_measure(synth, [commands.CharacterModeCommand(True), "a", commands.CharacterModeCommand(False)])
        article = speak_and_measure(synth, ["a  "])
        check(letter > article, "a spelled letter is longer than the article")

        # A break inserts silence of the given length. NVDA ends each piece of
        # text with its CHUNK_SEPARATOR, two spaces, outside character mode.
        plain = speak_and_measure(synth, ["one  "])
        broken = speak_and_measure(synth, ["one  ", commands.BreakCommand(time=500)])
        check(broken - plain == 22050, f"a 500 ms break adds half a second ({broken - plain} bytes)")

        # Capital pitch change: NVDA's PitchCommand raises the configured
        # pitch by its offset, then returns to it.
        pitches = []
        set_pitch = synth._lib.klattara_set_pitch
        synth._lib.klattara_set_pitch = lambda engine, hz: pitches.append(hz) or set_pitch(engine, hz)
        speak_and_measure(
            synth,
            [commands.CharacterModeCommand(True), commands.PitchCommand(offset=30), "A", commands.PitchCommand(), "b"],
        )
        check(pitches == [264, 110], f"the capital is higher and the pitch returns: {pitches} Hz")
        pitches.clear()
        speak_and_measure(synth, [commands.PitchCommand(offset=200), "A"])
        check(pitches == [550], f"a pitch beyond the slider is limited to the engine's 550 Hz: {pitches}")
        synth._lib.klattara_set_pitch = set_pitch

        # Precise pitch in hertz and precise rate in words per minute, as
        # Reloq-C does them: a value and a check box for each. When the mode
        # is on, the exact value reaches the engine; moving the normal slider
        # switches the mode off again, but loading settings does not.
        old_rate, old_pitch = synth.rate, synth.pitch
        set_rate = synth._lib.klattara_set_rate
        rates = []
        synth._lib.klattara_set_rate = lambda engine, mult: rates.append(mult) or set_rate(engine, mult)
        synth._lib.klattara_set_pitch = lambda engine, hz: pitches.append(hz) or set_pitch(engine, hz)
        pitches.clear()
        # The Klattara settings panel applies these through configure_*; the
        # values are plain attributes, not properties, as in Reloq-C.
        synth.configure_pitch_hertz(True, 150)
        synth.configure_rate_wpm(True, 380)
        speak_and_measure(synth, ["Hello.  "])
        check(pitches == [150], f"hertz pitch sends exactly 150 Hz: {pitches}")
        # The rate is the engine's measured table, not words per minute
        # divided by 190: 380 is 1.87, 95 is 0.54, 190 is 1.0.
        check(abs(rates[0] - 1.869) < 0.01, f"380 words per minute is 1.87 times normal: {rates}")
        pitches.clear()
        rates.clear()
        synth.configure_rate_wpm(True, 95)
        speak_and_measure(synth, ["Hello.  "])
        check(abs(rates[0] - 0.539) < 0.01, f"95 words per minute is 0.54 times normal: {rates}")
        pitches.clear()
        rates.clear()
        synth.configure_rate_wpm(True, 190)
        speak_and_measure(synth, ["Hello.  "])
        check(rates == [1.0], f"190 words per minute is the normal rate: {rates}")
        pitches.clear()
        rates.clear()
        synth.configure_rate_wpm(True, 30)
        speak_and_measure(synth, ["Hello.  "])
        check(rates == [0.2032], f"30 words per minute is the slowest rate: {rates}")
        pitches.clear()
        rates.clear()
        synth.configure_rate_wpm(True, 38)
        speak_and_measure(synth, ["Hello.  "])
        check(rates == [0.25], f"38 words per minute is rate 0.25, as before: {rates}")
        pitches.clear()
        rates.clear()
        synth.configure_rate_wpm(True, 1600)
        speak_and_measure(synth, ["Hello.  "])
        check(abs(rates[0] - 7.166) < 0.01, f"1600 words per minute is 7.17 times normal: {rates}")
        pitches.clear()
        rates.clear()
        synth.configure_rate_wpm(True, 1796)
        speak_and_measure(synth, ["Hello.  "])
        check(rates == [8.0], f"1796 words per minute is rate 8, as before: {rates}")
        pitches.clear()
        rates.clear()
        synth.configure_rate_wpm(True, 1920)
        speak_and_measure(synth, ["Hello.  "])
        check(rates == [8.5276], f"1920 words per minute is the fastest rate: {rates}")
        check(synth._lib.klattara_set_rate(synth._engine, 8.5276) == 0, "the library takes the fastest rate")
        check(synth._lib.klattara_set_rate(synth._engine, 0.2032) == 0, "and the slowest")
        # The driver's table must be the engine's, row for row.
        import re
        speech_rs = (ROOT / "src" / "engine" / "src" / "speech.rs").read_text(encoding="utf-8")
        body = speech_rs.split("const WORDS_PER_MINUTE", 1)[1].split("];", 1)[0]
        engine_table = tuple((float(w), float(r)) for w, r in re.findall(r"\(([\d.]+), ([\d.]+)\)", body))
        check(engine_table == driver.WPM_TABLE, f"the driver's words-per-minute table is the engine's: {engine_table}")
        # Out of range is refused, not clamped silently.
        for bad in (lambda: synth.configure_rate_wpm(True, 29), lambda: synth.configure_rate_wpm(True, 1921),
                    lambda: synth.configure_pitch_hertz(True, 39), lambda: synth.configure_pitch_hertz(True, 551)):
            try:
                bad()
                check(False, "an out-of-range precise value is refused")
            except ValueError:
                pass
        check(True, "out-of-range precise values are refused")
        # The table was verified against the engine's own [:speed] command:
        # the driver and [:speed] give byte-identical audio at 38, 95, 190,
        # 380, 400, 1600 and 1796 words per minute. That check cannot live
        # here, because the driver's speak strips [: commands from screen text.
        synth._rateInWpm = False
        # Moving a slider switches its precise mode off; loading settings does not.
        synth.pitch = 50
        check(not synth._pitchInHertz, "moving the pitch slider switches hertz pitch off")
        synth.rate = 50
        check(not synth._rateInWpm, "moving the rate slider switches words-per-minute rate off")
        synth._pitchInHertz = True
        synth._rateInWpm = True
        synth._loadingSettings = True
        synth.pitch = 50
        synth.rate = 50
        check(synth._pitchInHertz and synth._rateInWpm, "loading settings leaves the precise modes on")
        synth._loadingSettings = False
        # The hertz value follows the voice while the mode is off.
        synth._pitchInHertz = False
        synthDriverHandler.changeVoice(synth, "gary")
        check(synth._pitchHertz == 78, f"the hertz value follows Gary's 78 Hz: {synth._pitchHertz}")
        # Regression (1.9.3): the hertz value taken from a voice was the
        # library's float, 78.0. NVDA's configuration refuses a float for an
        # integer setting, so saving after any change of voice raised, and
        # the settings dialog's OK and Apply did nothing.
        check(type(synth._pitchHertz) is int, f"the hertz value from a voice is a whole number: {synth._pitchHertz!r}")
        synth.saveSettings()
        check(
            config.conf["speech"]["klattara"]["_pitchHertz"] == 78,
            "the settings save after a change of voice, as OK and Apply do",
        )
        for name in synth._voiceIds:
            synthDriverHandler.changeVoice(synth, name)
            synth.saveSettings()
        check(True, "every voice's settings save, with hertz pitch off")
        synthDriverHandler.changeVoice(synth, "steve")
        synth.rate, synth.pitch = old_rate, old_pitch
        synth._lib.klattara_set_pitch = set_pitch
        synth._lib.klattara_set_rate = set_rate

        # The voices, in the engine's order, with their names and language.
        voices = synth.availableVoices
        check(list(voices) == ["steve", "brittany", "gary", "david", "taylor"], f"the voices are listed: {list(voices)}")
        check(
            [v.displayName for v in voices.values()] == ["Standard Steve", "Breezy Brittany", "Giant Gary", "Delicate David", "Tiny Taylor"],
            "the voices show their full names",
        )
        check(synth.language == "en_US", f"the voices speak American English: {synth.language}")

        # Each voice's pitch and inflection are stored with it, and choosing a voice
        # sets both sliders.
        synth._lib.klattara_set_pitch = lambda engine, hz: pitches.append(hz) or set_pitch(engine, hz)
        chosen = []
        set_voice = synth._lib.klattara_set_voice
        synth._lib.klattara_set_voice = lambda engine, voice: chosen.append(voice) or set_voice(engine, voice)
        pitches.clear()
        steve = speak_and_measure(synth, ["Hello, world.  "])
        check(pitches == [110] and chosen == [0], f"Steve speaks at his own 110 Hz: {pitches}")
        pitches.clear()
        chosen.clear()
        # As NVDA's settings ring does. Capital pitch change counts from the saved pitch.
        synthDriverHandler.changeVoice(synth, "brittany")
        synth.saveSettings()
        check(synth.pitch == 33, f"changing to Brittany moves the slider to her pitch: {synth.pitch}")
        brittany = speak_and_measure(synth, ["Hello, world.  "])
        check(pitches == [210] and chosen == [1], f"Brittany speaks at her own 210 Hz: {pitches}")
        check(steve == brittany, f"both voices speak at the same rate ({steve} and {brittany} bytes)")
        pitches.clear()
        speak_and_measure(
            synth,
            [commands.CharacterModeCommand(True), commands.PitchCommand(offset=30), "A", commands.PitchCommand(), "b"],
        )
        check(pitches == [361, 210], f"Brittany's capital is higher, and the pitch returns: {pitches} Hz")
        pitches.clear()
        synth.pitch = 100
        speak_and_measure(synth, ["Hello.  "])
        check(pitches == [550], f"the slider's highest gives Brittany the engine's 550 Hz: {pitches}")
        synth.pitch = 0
        pitches.clear()
        speak_and_measure(synth, ["Hello.  "])
        check(pitches == [40], f"the slider's lowest gives Brittany 40 Hz: {pitches}")
        synth.pitch = 34
        pitches.clear()
        speak_and_measure(synth, ["Hello.  "])
        check(pitches == [213], f"a step up from her own pitch is 213 Hz: {pitches}")
        synth.pitch = 33
        pitches.clear()
        speak_and_measure(synth, ["Hello.  "])
        check(pitches == [210], f"and back on her own position it is 210 Hz again: {pitches}")

        # A change the user makes lasts until another voice is chosen. That voice's
        # own pitch and inflection then take over, wherever the sliders were.
        synth.pitch = 60
        synth.inflection = 80
        synth.voice = "steve"
        check((synth.pitch, synth.inflection) == (14, 40), f"Steve's own values come back: {synth.pitch}, {synth.inflection}")
        synth.pitch = 90
        synth.inflection = 10
        synth.voice = "brittany"
        check((synth.pitch, synth.inflection) == (33, 40), f"so do Brittany's: {synth.pitch}, {synth.inflection}")
        synth.voice = "gary"
        check((synth.pitch, synth.inflection) == (7, 34), f"and Gary's: {synth.pitch}, {synth.inflection}")
        pitches.clear()
        chosen.clear()
        speak_and_measure(synth, ["Hello, world.  "])
        check(pitches == [78] and chosen == [2], f"Gary speaks at his own 78 Hz: {pitches}")
        synth.voice = "david"
        check((synth.pitch, synth.inflection) == (19, 32), f"and David's: {synth.pitch}, {synth.inflection}")
        pitches.clear()
        chosen.clear()
        speak_and_measure(synth, ["Hello, world.  "])
        check(pitches == [138] and chosen == [3], f"David speaks at his own 138 Hz: {pitches}")
        synth.voice = "taylor"
        check((synth.pitch, synth.inflection) == (45, 46), f"and Taylor's: {synth.pitch}, {synth.inflection}")
        pitches.clear()
        chosen.clear()
        speak_and_measure(synth, ["Hello, world.  "])
        check(pitches == [270] and chosen == [4], f"Taylor speaks at their own 270 Hz: {pitches}")
        synth.voice = "brittany"
        synth.pitch = 50
        synth.voice = "brittany"
        check(synth.pitch == 33, f"also on choosing the voice already in use: {synth.pitch}")

        # A voice brings its own quality settings, except flutter, which
        # stays where the user put it.
        synth.voice = "david"
        check(
            (
                synth.headSize,
                synth.roughness,
                synth.breathiness,
                synth.openQuotient,
                synth.smoothness,
                synth.baselineFall,
                synth.hatRise,
                synth.stressRise,
                synth.quickness,
                synth.laxBreathiness,
                synth.femaleVocalTract,
            )
            == (94, 16, 52, 70, 28, 8, 14, 20, 6, 70, False),
            f"David's own quality: head {synth.headSize}, roughness {synth.roughness}, breath {synth.breathiness}, "
            f"open quotient {synth.openQuotient}, smooth {synth.smoothness}, fall {synth.baselineFall}, "
            f"hat {synth.hatRise}, stress {synth.stressRise}, quick {synth.quickness}, lax {synth.laxBreathiness}",
        )
        check(synth.flutter == 0, f"flutter stays off on a change of voice: {synth.flutter}")
        synth.flutter = 60
        synth.voice = "brittany"
        check(synth.flutter == 60, f"flutter stays where the user put it: {synth.flutter}")
        check(
            (synth.headSize, synth.breathiness, synth.openQuotient, synth.laxBreathiness, synth.femaleVocalTract)
            == (100, 36, 65, 65, True),
            f"so do Brittany's, with her female vocal tract: {synth.headSize}, {synth.breathiness}, "
            f"{synth.openQuotient}, {synth.laxBreathiness}, {synth.femaleVocalTract}",
        )
        synth.flutter = 0
        synth.voice = "steve"

        # NVDA saves the voice and the sliders, and takes them back on loading: the
        # voice first, then the saved values, which have the last word. It gets over a
        # saved voice that no longer exists, and settings saved before there were voices.
        synth.voice = "brittany"
        synth.pitch = 35
        synth.inflection = 60
        synth.saveSettings()
        check(config.conf["speech"]["klattara"]["voice"] == "brittany", "the voice is saved to NVDA's configuration")
        check(config.conf["speech"]["klattara"]["pitch"] == 35, "the pitch is saved with it")
        synth.voice = "steve"
        synth.loadSettings()
        restored = (synth.voice, synth.pitch, synth.inflection)
        check(restored == ("brittany", 35, 60), f"they come back on loading: {restored}")
        synth.voice = "steve"
        synth.loadSettings(onlyChanged=True)
        restored = (synth.voice, synth.pitch, synth.inflection)
        check(restored == ("brittany", 35, 60), f"also on a change of profile: {restored}")
        synth.pitch = 33
        synth.saveSettings()
        synth.voice = "steve"
        synth.loadSettings()
        pitches.clear()
        speak_and_measure(synth, ["Hello.  "])
        check(pitches == [210], f"her own pitch, saved, is exact again on loading: {pitches}")
        config.conf["speech"]["klattara"]["voice"] = "nobody"
        synth.loadSettings()
        check(synth.voice == "brittany", "a saved voice that no longer exists leaves the voice as it is")
        check(config.conf["speech"]["klattara"]["voice"] == "brittany", "and the configuration is corrected")
        synth.voice = "steve"
        del config.conf["speech"]["klattara"]["voice"]
        config.conf["speech"]["klattara"]["pitch"] = 14
        config.conf["speech"]["klattara"]["inflection"] = 40
        synth.loadSettings()
        check((synth.voice, synth.pitch) == ("steve", 14), "settings saved before there were voices load as Steve")
        synth._lib.klattara_set_pitch = set_pitch
        synth._lib.klattara_set_voice = set_voice

        # With the check box off, abbreviations are read as written: "Inc."
        # as "ink" is shorter than "incorporated".
        expanded = speak_and_measure(synth, ["Apple Inc. is here.  "])
        synth.expandAbbreviations = False
        written = speak_and_measure(synth, ["Apple Inc. is here.  "])
        synth.expandAbbreviations = True
        check(written < expanded - 10000, f"unchecked, abbreviations are read as written ({written} < {expanded} bytes)")
        check(config.conf["speech"]["klattara"]["expandAbbreviations"] is True, "the setting is saved to NVDA's configuration")

        # Checked, phrase prediction breaks a long sentence into phrases. Each
        # break lengthens the syllable before it a little.
        long = "The old man walked slowly down the long dusty road toward the small town where he lived.  "
        whole = speak_and_measure(synth, [long])
        synth.phrasePrediction = True
        phrased = speak_and_measure(synth, [long])
        synth.phrasePrediction = False
        check(phrased > whole + 1000, f"checked, a long sentence is broken into phrases ({phrased} > {whole} bytes)")
        check(config.conf["speech"]["klattara"]["phrasePrediction"] is False, "phrase prediction is saved to NVDA's configuration")

        # The sample rate can change: the engine and the player are remade at
        # it, speech still flows, and a break stays the same length in time.
        hello = speak_and_measure(synth, ["Hello, world.  "])
        synth.sampleRate = 44100
        hello44100 = speak_and_measure(synth, ["Hello, world.  "])
        check(
            hello44100 > hello * 1.8,
            f"speech still flows at 44100 Hz, with twice the samples ({hello44100} against {hello} bytes)",
        )
        with lock:
            rates = [e[1] for e in events if e[0] == "playerRate"]
        check(rates[-1] == 44100, f"the player was remade at 44100 Hz: {rates}")
        plain = speak_and_measure(synth, ["one  "])
        broken = speak_and_measure(synth, ["one  ", commands.BreakCommand(time=500)])
        check(broken - plain == 44100, f"a 500 ms break adds half a second at 44100 Hz ({broken - plain} bytes)")
        synth.sampleRate = 22050
        check(
            speak_and_measure(synth, ["Hello, world.  "]) == hello,
            "and the same text at 22050 Hz is itself again",
        )
        with lock:
            rates = [e[1] for e in events if e[0] == "playerRate"]
        check(rates[-1] == 22050, f"and back at 22050 Hz: {rates}")

        # The sample rate is a combo box, not a slider: the dialog stores the
        # rate as its id string, and the driver offers the twenty rates.
        # The dialog looks the rates up as f"available{id.capitalize()}s",
        # and "sampleRate".capitalize() is "Samplerate", so the property
        # must be availableSamplerates or the control never appears.
        check(
            hasattr(synth, f"available{SynthDriver._sampleRateSetting.id.capitalize()}s"),
            "the rates are found under the name NVDA's dialog builds from the setting id",
        )
        check(
            list(synth.availableSamplerates) == [str(rate) for rate in EXPECTED_SAMPLE_RATES],
            f"the combo box offers the twenty rates: {list(synth.availableSamplerates)}",
        )
        check(
            [info.displayName for info in synth.availableSamplerates.values()][:3] == ["8000 Hz", "10000 Hz", "11000 Hz"],
            "each rate is shown with its unit",
        )
        synth.sampleRate = "96000"
        check(synth.sampleRate == "96000", "the rate is kept as its id string")
        check(
            speak_and_measure(synth, ["Hello, world.  "]) > hello * 4,
            "speech still flows at 96000 Hz, with four times the samples",
        )
        with lock:
            rates = [e[1] for e in events if e[0] == "playerRate"]
        check(rates[-1] == 96000, f"the player was remade at 96000 Hz: {rates}")
        # A saved value from the old slider may be any rate at all: it lands
        # on the nearest supported one.
        synth.sampleRate = "15000"
        check(synth.sampleRate == "16000", "an old value of 15000 Hz lands on 16000 Hz")
        speak_and_measure(synth, ["Hello, world.  "])
        with lock:
            rates = [e[1] for e in events if e[0] == "playerRate"]
        check(rates[-1] == 16000, f"and the player follows: {rates}")
        synth.sampleRate = 22050
        check(synth.sampleRate == "22050", "an integer still works, and reads back as a string")

        # A Klattara command in the text never reaches the engine, wherever it falls,
        # so phoneme input stays off and a page can't change how speech is read.
        # The driver's own voice-quality command is the exception: it sends one
        # [:cv] command itself, before each piece of text.
        sent = []
        real_speak = synth._lib.klattara_speak
        def recording_speak(engine, text, reading, callback, user):
            sent.append((text, reading))
            return real_speak(engine, text, reading, callback, user)
        synth._lib.klattara_speak = recording_speak

        def own_command():
            # The command the driver sends for the sliders as they are now.
            # Saved values have the last word on loading, as with pitch, so
            # these need not be the current voice's own values.
            return (
                f"[:cv hs {synth.headSize} rg {synth.roughness} br {synth.breathiness} "
                f"oq {synth.openQuotient} sm {synth.smoothness} fl {synth.flutter} "
                f"fa {synth.baselineFall} ha {synth.hatRise} st {synth.stressRise} "
                f"qk {synth.quickness} lx {synth.laxBreathiness}]"
            ).encode("utf-8")

        def spoken():
            """What was sent for the text itself, without the driver's own command."""
            return [(text, reading) for text, reading in sent if not text.startswith(b"[:cv ")]

        word = "[HH AH0 L OW1].  "
        as_text = speak_and_measure(synth, [word])
        for sequence in (
            ["[:phone klattara enable]  "],
            ["[:PHONE KLATTARA ENABLE]  "],
            ["[:vb]  "],
            ["[:voice Brittany]  "],
            ["[:cv pi 300 hs 70]  "],
            ["Before [:phone klattara enable] after.  "],
            ["[:phone klattara ", "enable]  "],
            ["[", ":phone klattara enable]  "],
            ["[:phone klattara ", commands.IndexCommand(9), "enable]  "],
            ["[", commands.IndexCommand(9), ":phone klattara enable]  "],
        ):
            speak_and_measure(synth, sequence)
            check(speak_and_measure(synth, [word]) == as_text, f"phoneme input stays off after {sequence!r}")
        check(
            all(text == own_command() for text, _ in sent if text.startswith(b"[:cv ")),
            f"the driver's own command is the voice's own quality: {sent}",
        )
        check(
            not any(b"[:" in text for text, _ in spoken()),
            f"no command from the text was sent to the engine: {sent}",
        )
        sent.clear()
        speak_and_measure(synth, ["Before [:phone klattara enable] after.  "])
        check(
            spoken() == [(b"Before  phone klattara enable] after.", driver.KLATTARA_READ_TEXT)],
            f"the words of a command are still read: {spoken()!r}",
        )
        sent.clear()
        speak_and_measure(synth, ["[:  "])
        check(not sent, "nothing is sent when only a command's opening was there")
        # Spelled text keeps its characters: the engine doesn't take commands there.
        speak_and_measure(synth, [commands.CharacterModeCommand(True), "[:", commands.CharacterModeCommand(False)])
        check(spoken() == [(b"[:", driver.KLATTARA_READ_SPELLED)], f"spelled text keeps its characters: {spoken()}")
        check(speak_and_measure(synth, [word]) == as_text, "and phoneme input stays off after them")
        synth._lib.klattara_speak = real_speak

        # Commands the driver does not support are skipped quietly.
        speak_and_measure(synth, [commands.LangChangeCommand("en_US"), "Hello.  "])
        with lock:
            errors = [e for e in events if e[0] == "error"]
        check(not errors, f"no errors were logged: {errors}")
    finally:
        synth.terminate()
    check(not synth._thread.is_alive(), "the speech thread shuts down")
    print("All driver tests passed.")


if __name__ == "__main__":
    main()
