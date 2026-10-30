# Klattara speech synthesizer driver for NVDA.
#
# Speech is synthesized on a background thread by klattara.dll, in a folder
# next to this file, and played through NVDA's own audio output. Synthesis
# runs at hundreds of times real time, and audio is streamed in small
# chunks, so speech starts at once.
#
# Cancelling bumps a generation counter. Work queued before the cancel is
# skipped, and synthesis in progress stops at its next audio chunk.
#
# Written against NVDA's own source: synthDriverHandler, nvwave,
# speech.commands and autoSettingsUtils.

import ctypes
import os
import queue
import threading

import addonHandler
import config
import nvwave
import synthDriverHandler
from autoSettingsUtils.driverSetting import BooleanDriverSetting
from logHandler import log
from speech.commands import (
	BreakCommand,
	CharacterModeCommand,
	IndexCommand,
	PitchCommand,
	RateCommand,
	VolumeCommand,
)
from synthDriverHandler import synthDoneSpeaking, synthIndexReached

addonHandler.initTranslation()

# NVDA is 32-bit before 2026 and 64-bit from then on, so the add-on carries a
# DLL for each and loads the one that matches the running NVDA.
DLL_PATH = os.path.join(
	os.path.dirname(__file__),
	"x64" if ctypes.sizeof(ctypes.c_void_p) == 8 else "x86",
	"klattara.dll",
)
SAMPLE_RATE = 22050
BYTES_PER_SAMPLE = 2

# Reading modes, as in klattara.h.
KLATTARA_READ_TEXT = 0
KLATTARA_READ_SPELLED = 1

KLATTARA_AUDIO_CALLBACK = ctypes.CFUNCTYPE(
	ctypes.c_int,
	ctypes.c_void_p,
	ctypes.POINTER(ctypes.c_int16),
	ctypes.c_int,
)

# Each slider spreads evenly over a range of whole-number parameters, as
# DECtalk's do, through NVDA's _percentToParam. The defaults fall wherever
# the normal values land rather than at 50.
# Rate: hundredths of normal speed. 29 is about 50 words per minute, 100
# normal (about 190), 500 about 980.
MIN_RATE, NORMAL_RATE, MAX_RATE = 29, 100, 500
# Pitch: the base pitch in Hz; what the engine accepts.
MIN_PITCH, NORMAL_PITCH, MAX_PITCH = 40, 110, 600
# Inflection: percent of normal pitch movement, like DECtalk's pitch range.
MIN_INFLECTION, NORMAL_INFLECTION, MAX_INFLECTION = 0, 100, 250
# Volume: percent of full volume.
MIN_VOLUME, MAX_VOLUME = 0, 100


def load_library(path):
	lib = ctypes.CDLL(path)
	lib.klattara_create.argtypes = [ctypes.c_int]
	lib.klattara_create.restype = ctypes.c_void_p
	lib.klattara_destroy.argtypes = [ctypes.c_void_p]
	lib.klattara_destroy.restype = None
	for setter in ("rate", "pitch", "inflection", "volume"):
		function = getattr(lib, f"klattara_set_{setter}")
		function.argtypes = [ctypes.c_void_p, ctypes.c_double]
		function.restype = ctypes.c_int
	for setter in ("abbreviations", "phrase_prediction"):
		function = getattr(lib, f"klattara_set_{setter}")
		function.argtypes = [ctypes.c_void_p, ctypes.c_int]
		function.restype = ctypes.c_int
	lib.klattara_speak.argtypes = [
		ctypes.c_void_p,
		ctypes.c_char_p,
		ctypes.c_int,
		KLATTARA_AUDIO_CALLBACK,
		ctypes.c_void_p,
	]
	lib.klattara_speak.restype = ctypes.c_int
	return lib


class SynthDriver(synthDriverHandler.SynthDriver):
	name = "klattara"
	description = "Klattara"

	_rateSetting = synthDriverHandler.SynthDriver.RateSetting()
	_rateSetting.defaultVal = synthDriverHandler.SynthDriver._paramToPercent(NORMAL_RATE, MIN_RATE, MAX_RATE)
	_pitchSetting = synthDriverHandler.SynthDriver.PitchSetting()
	_pitchSetting.defaultVal = synthDriverHandler.SynthDriver._paramToPercent(NORMAL_PITCH, MIN_PITCH, MAX_PITCH)
	_inflectionSetting = synthDriverHandler.SynthDriver.InflectionSetting()
	_inflectionSetting.defaultVal = synthDriverHandler.SynthDriver._paramToPercent(
		NORMAL_INFLECTION,
		MIN_INFLECTION,
		MAX_INFLECTION,
	)
	_volumeSetting = synthDriverHandler.SynthDriver.VolumeSetting()
	_volumeSetting.defaultVal = MAX_VOLUME
	_expandAbbreviationsSetting = BooleanDriverSetting(
		"expandAbbreviations",
		# Translators: Label for a check box in the voice settings. When checked,
		# abbreviations are read in full: "Dr." as "doctor", "Calif." as "California".
		_("Expand &abbreviations"),
		defaultVal=True,
	)
	_phrasePredictionSetting = BooleanDriverSetting(
		"phrasePrediction",
		# Translators: Label for a check box in the voice settings. When checked,
		# long sentences without commas are broken into phrases, each with its own
		# rise and fall in pitch, as Eloquence's phrase prediction does.
		_("P&hrase prediction"),
		defaultVal=False,
	)
	supportedSettings = (
		_rateSetting,
		_pitchSetting,
		_inflectionSetting,
		_volumeSetting,
		_expandAbbreviationsSetting,
		_phrasePredictionSetting,
	)
	supportedCommands = {
		IndexCommand,
		CharacterModeCommand,
		BreakCommand,
		PitchCommand,
		RateCommand,
		VolumeCommand,
	}
	supportedNotifications = {synthIndexReached, synthDoneSpeaking}

	@classmethod
	def check(cls):
		return os.path.isfile(DLL_PATH)

	def __init__(self):
		super().__init__()
		self._lib = load_library(DLL_PATH)
		self._engine = self._lib.klattara_create(SAMPLE_RATE)
		if not self._engine:
			raise RuntimeError("Klattara could not create an engine")
		self._rate = self._rateSetting.defaultVal
		self._pitch = self._pitchSetting.defaultVal
		self._inflection = self._inflectionSetting.defaultVal
		self._volume = self._volumeSetting.defaultVal
		self._expandAbbreviations = self._expandAbbreviationsSetting.defaultVal
		self._phrasePrediction = self._phrasePredictionSetting.defaultVal
		self._player = nvwave.WavePlayer(
			channels=1,
			samplesPerSec=SAMPLE_RATE,
			bitsPerSample=BYTES_PER_SAMPLE * 8,
			outputDevice=config.conf["audio"]["outputDevice"],
		)
		self._generation = 0
		self._speaking = None
		self._queue = queue.Queue()
		# Keep a reference so the callback outlives every call into the DLL.
		self._audioCallback = KLATTARA_AUDIO_CALLBACK(self._onAudio)
		self._thread = threading.Thread(target=self._run, name="Klattara", daemon=True)
		self._thread.start()

	def terminate(self):
		self._generation += 1
		self._queue.put(None)
		self._player.stop()
		self._thread.join(timeout=2)
		self._player.close()
		self._lib.klattara_destroy(self._engine)
		self._engine = None
		super().terminate()

	# Settings are percentages, as NVDA keeps them. The speech thread reads
	# them before each piece of text, so only that thread touches the engine.

	def _get_rate(self):
		return self._rate

	def _set_rate(self, value):
		self._rate = value

	def _get_pitch(self):
		return self._pitch

	def _set_pitch(self, value):
		self._pitch = value

	def _get_inflection(self):
		return self._inflection

	def _set_inflection(self, value):
		self._inflection = value

	def _get_volume(self):
		return self._volume

	def _set_volume(self, value):
		self._volume = value

	def _get_expandAbbreviations(self):
		return self._expandAbbreviations

	def _set_expandAbbreviations(self, value):
		self._expandAbbreviations = value

	def _get_phrasePrediction(self):
		return self._phrasePrediction

	def _set_phrasePrediction(self, value):
		self._phrasePrediction = value

	def speak(self, speechSequence):
		generation = self._generation
		spelled = False
		# Prosody commands, as percentages by setting name, that replace the
		# configured value until they return to default.
		prosody = {}
		text = []

		def flush():
			joined = "".join(text).strip()
			if joined:
				self._queue.put((generation, "text", (joined, spelled, dict(prosody))))
			text.clear()

		for item in speechSequence:
			if isinstance(item, str):
				text.append(item)
			elif isinstance(item, IndexCommand):
				flush()
				self._queue.put((generation, "index", item.index))
			elif isinstance(item, CharacterModeCommand):
				flush()
				spelled = item.state
			elif isinstance(item, BreakCommand):
				flush()
				self._queue.put((generation, "break", item.time))
			elif isinstance(item, (PitchCommand, RateCommand, VolumeCommand)):
				# Capital pitch change comes as a PitchCommand. newValue is the
				# configured percentage with the change applied.
				flush()
				if item.isDefault:
					prosody.pop(item.settingName, None)
				else:
					prosody[item.settingName] = item.newValue
			else:
				log.debugWarning(f"Unsupported speech command: {item!r}")
		flush()
		self._queue.put((generation, "done", None))

	def cancel(self):
		self._generation += 1
		# Drop queued speech, but keep the shutdown signal if it is there.
		stopping = False
		try:
			while True:
				if self._queue.get_nowait() is None:
					stopping = True
		except queue.Empty:
			pass
		if stopping:
			self._queue.put(None)
		self._player.stop()

	def pause(self, switch):
		self._player.pause(switch)

	def _run(self):
		while True:
			item = self._queue.get()
			if item is None:
				return
			generation, kind, payload = item
			if generation != self._generation:
				continue
			try:
				if kind == "text":
					self._say(generation, *payload)
				elif kind == "index":
					# Reported when the audio before it has played: an empty
					# chunk carries the callback, as in NVDA's SAPI 5 driver.
					self._player.feed(
						None,
						size=0,
						onDone=lambda index=payload: synthIndexReached.notify(synth=self, index=index),
					)
				elif kind == "break":
					samples = SAMPLE_RATE * payload // 1000
					self._player.feed(bytes(samples * BYTES_PER_SAMPLE))
				elif kind == "done":
					self._player.idle()
					if generation == self._generation:
						synthDoneSpeaking.notify(synth=self)
			except Exception:
				log.error("Klattara speech thread", exc_info=True)

	def _say(self, generation, text, spelled, prosody):
		lib, engine = self._lib, self._engine
		rate = self._percentToParam(prosody.get("rate", self._rate), MIN_RATE, MAX_RATE)
		pitch = self._percentToParam(prosody.get("pitch", self._pitch), MIN_PITCH, MAX_PITCH)
		inflection = self._percentToParam(self._inflection, MIN_INFLECTION, MAX_INFLECTION)
		volume = self._percentToParam(prosody.get("volume", self._volume), MIN_VOLUME, MAX_VOLUME)
		# A prosody command can go past the slider's range; keep within the
		# engine's.
		lib.klattara_set_rate(engine, min(MAX_RATE, max(MIN_RATE, rate)) / 100)
		lib.klattara_set_pitch(engine, min(MAX_PITCH, max(MIN_PITCH, pitch)))
		lib.klattara_set_inflection(engine, inflection / 100)
		lib.klattara_set_volume(engine, min(MAX_VOLUME, max(MIN_VOLUME, volume)) / 100)
		lib.klattara_set_abbreviations(engine, 1 if self._expandAbbreviations else 0)
		lib.klattara_set_phrase_prediction(engine, 1 if self._phrasePrediction else 0)
		self._speaking = generation
		reading = KLATTARA_READ_SPELLED if spelled else KLATTARA_READ_TEXT
		result = lib.klattara_speak(engine, text.encode("utf-8"), reading, self._audioCallback, None)
		if result < 0:
			log.error(f"Klattara could not speak {text!r}: error {result}")

	def _onAudio(self, user, samples, count):
		"""Called by the DLL with each chunk of audio. Returns nonzero to stop."""
		if self._speaking != self._generation:
			return 1
		try:
			# feed copies the audio before returning, so the DLL's buffer can
			# be passed directly, as NVDA's OneCore driver does.
			self._player.feed(ctypes.cast(samples, ctypes.c_void_p), size=count * BYTES_PER_SAMPLE)
		except Exception:
			log.error("Klattara audio output", exc_info=True)
			return 1
		return 0 if self._speaking == self._generation else 1
