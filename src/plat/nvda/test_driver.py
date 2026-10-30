"""Tests the NVDA driver outside NVDA, against the real klattara.dll and NVDA's
own Python code.

NVDA is 32-bit before 2026 and 64-bit from then on, so each DLL is tested on the
NVDA that will load it: a 64-bit Python tests the 64-bit DLL on the clone of
https://github.com/nvaccess/nvda in the nvda folder of the project, and a 32-bit
Python tests the 32-bit DLL on NVDA 2025.1, in the nvda-release-2025.1 folder.

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
if ctypes.sizeof(ctypes.c_void_p) == 8:
    ARCHITECTURE = "x64"
    NVDA_SOURCE = ROOT / "nvda" / "source"
    NVDA_CLONE = "git clone --depth 1 https://github.com/nvaccess/nvda nvda"
    DLL = ROOT / "target" / "release" / "klattara.dll"
else:
    ARCHITECTURE = "x86"
    NVDA_SOURCE = ROOT / "nvda-release-2025.1" / "source"
    NVDA_CLONE = "git clone --depth 1 --branch release-2025.1 https://github.com/nvaccess/nvda nvda-release-2025.1"
    DLL = ROOT / "target" / "i686-pc-windows-msvc" / "release" / "klattara.dll"

events = []
lock = threading.Lock()


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


class WavePlayer:
    """Stands in for nvwave.WavePlayer, with the same signature."""

    def __init__(self, channels, samplesPerSec, bitsPerSample, outputDevice="default", wantDucking=True, purpose=None):
        assert (channels, samplesPerSec, bitsPerSample) == (1, 22050, 16)
        assert outputDevice == config.conf["audio"]["outputDevice"]

    def feed(self, data, size=None, onDone=None):
        record("audio", size if size is not None else len(data))
        if onDone:
            onDone()

    def idle(self):
        pass

    def stop(self):
        record("stop")

    def pause(self, switch):
        record("pause", switch)

    def close(self):
        self.stop()


def load_nvda():
    """Imports NVDA's real modules, with stand-ins for its runtime."""
    if not (NVDA_SOURCE / "synthDriverHandler.py").is_file():
        raise SystemExit(f"NVDA's source is needed in {NVDA_SOURCE.parent}: {NVDA_CLONE}")
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
    module("synthSettingsRing", SynthSettingsRing=object)
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
    check({"idle", "stop", "pause", "close"} <= methods.keys(), "NVDA's player has the other methods the driver calls")
    config_spec = (NVDA_SOURCE / "config" / "configSpec.py").read_text(encoding="utf-8")
    check("[audio]\n\toutputDevice" in config_spec, "NVDA keeps the output device in its audio settings")


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
    check(specs["pitch"] == "integer(default=12,min=0,max=100)", f"pitch defaults to 12: {specs['pitch']}")
    check(specs["inflection"] == "integer(default=40,min=0,max=100)", f"inflection defaults to 40: {specs['inflection']}")
    to_param = SynthDriver._percentToParam
    check(abs(to_param(15, driver.MIN_RATE, driver.MAX_RATE) - 100) <= 2, "rate 15 is normal speed")
    check((to_param(0, driver.MIN_RATE, driver.MAX_RATE), to_param(100, driver.MIN_RATE, driver.MAX_RATE)) == (29, 500), "rate spans about 50 to 980 words per minute")
    check((to_param(0, driver.MIN_PITCH, driver.MAX_PITCH), to_param(100, driver.MIN_PITCH, driver.MAX_PITCH)) == (40, 600), "pitch spans 40 to 600 Hz")
    check(to_param(40, driver.MIN_INFLECTION, driver.MAX_INFLECTION) == 100, "inflection 40 is normal, 100 is 250%")
    check(
        specs["expandAbbreviations"] == "boolean(default=True)",
        f"abbreviations are expanded by default: {specs['expandAbbreviations']}",
    )
    check(
        specs["phrasePrediction"] == "boolean(default=False)",
        f"phrase prediction is off by default: {specs['phrasePrediction']}",
    )

    synth = SynthDriver()
    # What NVDA does on loading a synth for the first time: save the defaults.
    config.conf["speech"][synth.name] = {}
    synth.saveSettings()
    synthDriverHandler._curSynth = synth
    check(config.conf["speech"]["klattara"]["rate"] == 15, "the default settings are saved to NVDA's configuration")
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

        # A faster rate gives less audio for the same text.
        synth.rate = 50
        fast = speak_and_measure(synth, ["Hello, world.  ", "This is Klattara.  "])
        check(fast < 0.5 * normal, f"rate 50 is much shorter ({fast} against {normal} bytes)")
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
        check(pitches == [275, 107], f"the capital is higher and the pitch returns: {pitches} Hz")
        pitches.clear()
        speak_and_measure(synth, [commands.PitchCommand(offset=200), "A"])
        check(pitches == [600], f"a pitch beyond the slider is limited to the engine's 600 Hz: {pitches}")
        synth._lib.klattara_set_pitch = set_pitch

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
