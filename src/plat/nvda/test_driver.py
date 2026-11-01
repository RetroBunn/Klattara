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
    check({"idle", "stop", "pause", "close"} <= methods.keys(), "NVDA's player has the other methods the driver calls")
    config_spec = (NVDA_SOURCE / "config" / "configSpec.py").read_text(encoding="utf-8")
    check("[audio]\n\toutputDevice" in config_spec, "NVDA keeps the output device in its audio settings")
    # The pitch slider moves with the voice only if NVDA looks at the settings again
    # after a change of voice, in the dialog and in the settings ring.
    dialogs = (NVDA_SOURCE / "gui" / "settingsDialogs.py").read_text(encoding="utf-8")
    check("self.container.updateDriverSettings(changedSetting=self.setting.id)" in dialogs, "NVDA's dialog updates its sliders after a change of voice")
    ring = (NVDA_SOURCE / "synthSettingsRing.py").read_text(encoding="utf-8")
    check("self.synth.saveSettings()" in ring, "NVDA's settings ring saves the settings after a change of voice")


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

    check(specs["voice"] == "string(default=None)", f"the voice is a setting NVDA keeps: {specs['voice']}")

    synth = SynthDriver()
    # What NVDA does on loading a synth for the first time: save the defaults.
    config.conf["speech"][synth.name] = {}
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
        check(pitches == [264, 110], f"the capital is higher and the pitch returns: {pitches} Hz")
        pitches.clear()
        speak_and_measure(synth, [commands.PitchCommand(offset=200), "A"])
        check(pitches == [550], f"a pitch beyond the slider is limited to the engine's 550 Hz: {pitches}")
        synth._lib.klattara_set_pitch = set_pitch

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

        # A Klattara command in the text never reaches the engine, wherever it falls,
        # so phoneme input stays off and a page can't change how speech is read.
        sent = []
        real_speak = synth._lib.klattara_speak
        def recording_speak(engine, text, reading, callback, user):
            sent.append((text, reading))
            return real_speak(engine, text, reading, callback, user)
        synth._lib.klattara_speak = recording_speak
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
        check(not any(b"[:" in text for text, _ in sent), f"no command was sent to the engine: {sent}")
        sent.clear()
        speak_and_measure(synth, ["Before [:phone klattara enable] after.  "])
        check(sent[0][0] == b"Before  phone klattara enable] after.", f"the words of a command are still read: {sent[0][0]!r}")
        sent.clear()
        speak_and_measure(synth, ["[:  "])
        check(not sent, "nothing is sent when only a command's opening was there")
        # Spelled text keeps its characters: the engine doesn't take commands there.
        speak_and_measure(synth, [commands.CharacterModeCommand(True), "[:", commands.CharacterModeCommand(False)])
        check(sent == [(b"[:", driver.KLATTARA_READ_SPELLED)], f"spelled text keeps its characters: {sent}")
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
