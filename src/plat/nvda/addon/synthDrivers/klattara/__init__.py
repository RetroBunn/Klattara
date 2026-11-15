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
from collections import OrderedDict

import addonHandler
import config
import nvwave
import synthDriverHandler
from autoSettingsUtils.driverSetting import BooleanDriverSetting, DriverSetting, NumericDriverSetting
from autoSettingsUtils.utils import StringParameterInfo
from logHandler import log
from speech.commands import (
	BreakCommand,
	CharacterModeCommand,
	IndexCommand,
	PitchCommand,
	RateCommand,
	VolumeCommand,
)
from synthDriverHandler import VoiceInfo, synthDoneSpeaking, synthIndexReached

addonHandler.initTranslation()

# NVDA is 32-bit before 2026 and 64-bit from then on, so the add-on carries a
# DLL for each and loads the one that matches the running NVDA.
DLL_PATH = os.path.join(
	os.path.dirname(__file__),
	"x64" if ctypes.sizeof(ctypes.c_void_p) == 8 else "x86",
	"klattara.dll",
)
# The engine runs at 8000 to 96000 Hz, 22050 recommended. At 8000 Hz the
# engine widens the highest formants so loud vowels stay off the rail, so the
# add-on offers the whole range.
DEFAULT_SAMPLE_RATE = 22050
# The rates the synthesizer runs at, in Hz: every rate the engine is tuned
# for, from the telephone rate up to the studio rate. The sample rate is a
# choice among these, not a slider: a slider over 8000-96000 cannot land on
# an exact rate, and the engine's behaviour is tuned per rate.
SAMPLE_RATES = (
	8000, 10000, 11000, 11025, 12000, 16000, 20000, 22000, 22050, 24000,
	32000, 40000, 44000, 44100, 48000, 64000, 80000, 88000, 88200, 96000,
)
BYTES_PER_SAMPLE = 2

# Reading modes, as in klattara.h.
KLATTARA_READ_TEXT = 0
KLATTARA_READ_SPELLED = 1

# Which model of the vocal tract the voice uses, as in klattara.h.
KLATTARA_MALE = 0
KLATTARA_FEMALE = 1

KLATTARA_AUDIO_CALLBACK = ctypes.CFUNCTYPE(
	ctypes.c_int,
	ctypes.c_void_p,
	ctypes.POINTER(ctypes.c_int16),
	ctypes.c_int,
)


def _discardAudio(user, samples, count):
	"""An audio callback that throws the audio away, for speaking a command
	on its own, which makes none."""
	return 0


# The callback outlives every call into the DLL, as the speaking one does.
_DISCARD_CALLBACK = KLATTARA_AUDIO_CALLBACK(_discardAudio)

# The voice quality each voice starts with, from src/engine/src/voicedefs.rs,
# in the engine's voice order (Steve, Brittany, Gary, David, Taylor): head
# size, roughness, breathiness, open quotient, smoothness, baseline fall, hat
# rise, stress rise, quickness, lax breathiness, and whether the female
# model of the vocal tract is used. Open quotient is the [:cv] oq value, as
# a percentage, which reproduces each voice's value exactly; the [:cv] rc
# richness setting cannot, as its steps are 0.007 of open quotient.
# Smoothness is the [:cv] sm value that reproduces the voice's spectral tilt.
# Flutter is not here: it is off unless the user turns it on.
VOICE_DEFAULTS = (
	(100, 0, 0, 55, 0, 18, 18, 32, 40, 0, False),
	(100, 0, 36, 65, 12, 12, 16, 36, 58, 65, True),
	(120, 8, 0, 60, 8, 10, 22, 26, 14, 8, False),
	(94, 16, 52, 70, 28, 8, 14, 20, 6, 70, False),
	(89, 0, 24, 58, 4, 5, 20, 42, 72, 30, True),
)

# Each slider spreads evenly over a range of whole-number parameters, as
# DECtalk's do, through NVDA's _percentToParam. The defaults fall wherever
# the normal values land rather than at 50.
# Rate: hundredths of normal speed. 29 is about 50 words per minute, 100
# normal (about 190), 500 about 980.
MIN_RATE, NORMAL_RATE, MAX_RATE = 29, 100, 500
# Precise rate in words per minute, as Reloq-C does it and as the engine's
# own [:speed] command does it. The rate is NOT words per minute divided by
# 190: the engine's speech.rs measures it at eleven points and interpolates
# between them, so 380 is 1.87 and 95 is 0.54, not 2.0 and 0.5. The slowest
# is 38 words per minute and the fastest 1796, the engine's slowest and
# fastest rates (0.25 and 8.0). Below 38 and above 1796 it clamps.
WPM_TABLE = (
	(38.0, 0.25),
	(87.0, 0.5),
	(138.0, 0.75),
	(190.0, 1.0),
	(298.0, 1.5),
	(409.0, 2.0),
	(634.0, 3.0),
	(863.0, 4.0),
	(1094.0, 5.0),
	(1326.0, 6.0),
	(1796.0, 8.0),
)
NORMAL_WPM = 190
MIN_WPM = 38
MAX_WPM = 1796


def _rateForWpm(wpm):
	"""The engine rate multiplier for a words-per-minute rate, as the engine's
	own rate_for_words_per_minute does it: piecewise linear through its
	measured table, clamped to 38-1796 words per minute."""
	wpm = min(MAX_WPM, max(MIN_WPM, wpm))
	for i in range(1, len(WPM_TABLE)):
		if wpm <= WPM_TABLE[i][0]:
			slow_wpm, slow_rate = WPM_TABLE[i - 1]
			fast_wpm, fast_rate = WPM_TABLE[i]
			return slow_rate + (fast_rate - slow_rate) * (wpm - slow_wpm) / (fast_wpm - slow_wpm)
	return WPM_TABLE[-1][1]
# Pitch: the base pitch in Hz; what the engine accepts.
MIN_PITCH, NORMAL_PITCH, MAX_PITCH = 40, 110, 550
# Inflection: percent of normal pitch movement, like DECtalk's pitch range.
MIN_INFLECTION, NORMAL_INFLECTION, MAX_INFLECTION = 0, 100, 250
# Volume: percent of full volume.
MIN_VOLUME, MAX_VOLUME = 0, 100
# What opens a Klattara command in text, such as [:phone klattara enable].
COMMAND_START = "[:"
# Every voice speaks American English.
VOICE_LANGUAGE = "en_US"


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
	lib.klattara_voice_count.argtypes = []
	lib.klattara_voice_count.restype = ctypes.c_int
	lib.klattara_voice_name.argtypes = [ctypes.c_int]
	lib.klattara_voice_name.restype = ctypes.c_char_p
	lib.klattara_voice_full_name.argtypes = [ctypes.c_int]
	lib.klattara_voice_full_name.restype = ctypes.c_char_p
	lib.klattara_voice_pitch.argtypes = [ctypes.c_int]
	lib.klattara_voice_pitch.restype = ctypes.c_double
	lib.klattara_voice_inflection.argtypes = [ctypes.c_int]
	lib.klattara_voice_inflection.restype = ctypes.c_double
	lib.klattara_set_voice.argtypes = [ctypes.c_void_p, ctypes.c_int]
	lib.klattara_set_voice.restype = ctypes.c_int
	lib.klattara_set_gender.argtypes = [ctypes.c_void_p, ctypes.c_int]
	lib.klattara_set_gender.restype = ctypes.c_int
	return lib


class SynthDriver(synthDriverHandler.SynthDriver):
	name = "klattara"
	description = "Klattara"

	_rateSetting = synthDriverHandler.SynthDriver.RateSetting()
	_rateSetting.defaultVal = synthDriverHandler.SynthDriver._paramToPercent(NORMAL_RATE, MIN_RATE, MAX_RATE)
	# The precise rate and pitch are hidden from NVDA's automatic Speech
	# panel: their ids start with an underscore, which the dialog skips, as
	# Reloq-C does for its own. They live in the Klattara settings panel
	# instead, where they get exact numeric inputs rather than 0 to 100
	# sliders. They stay in supportedSettings, so they are still saved and
	# loaded like any other setting.
	_rateWpmSetting = NumericDriverSetting(
		"_rateWpm",
		# Translators: Label for the exact speaking rate in words per minute,
		# set in the Klattara settings panel and used when "Use rate in words
		# per minute" is checked. 190 is the normal rate.
		_("Rate in &words per minute"),
		defaultVal=NORMAL_WPM,
		minVal=MIN_WPM,
		maxVal=MAX_WPM,
	)
	_rateInWpmSetting = BooleanDriverSetting(
		"_rateInWpm",
		# Translators: Label for a check box in the Klattara settings panel.
		# When checked, the rate comes from "Rate in words per minute"
		# instead of the rate slider.
		_("Use rate in words per &minute"),
		defaultVal=False,
	)
	_pitchSetting = synthDriverHandler.SynthDriver.PitchSetting()
	_pitchSetting.defaultVal = synthDriverHandler.SynthDriver._paramToPercent(NORMAL_PITCH, MIN_PITCH, MAX_PITCH)
	_pitchHertzSetting = NumericDriverSetting(
		"_pitchHertz",
		# Translators: Label for the exact base pitch in hertz, set in the
		# Klattara settings panel and used when "Use pitch in hertz" is
		# checked.
		_("Pitch in &hertz"),
		defaultVal=NORMAL_PITCH,
		minVal=MIN_PITCH,
		maxVal=MAX_PITCH,
	)
	_pitchInHertzSetting = BooleanDriverSetting(
		"_pitchInHertz",
		# Translators: Label for a check box in the Klattara settings panel.
		# When checked, the pitch comes from "Pitch in hertz" instead of the
		# pitch slider.
		_("Use pitch in &hertz"),
		defaultVal=False,
	)
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
	# Voice quality, as the engine's [:cv] command sets it. Each slider's
	# default is the first voice's own value; choosing a voice moves them all
	# to that voice's own values, as with pitch and inflection. Flutter is
	# the exception: it stays off unless the user turns it on.
	_flutterSetting = NumericDriverSetting(
		"flutter",
		# Translators: Label for a slider in the voice settings. How much the
		# pitch wavers, slowly and unevenly, as a shaky voice does.
		_("&Flutter"),
		defaultVal=0,
		minVal=0,
		maxVal=100,
	)
	_headSizeSetting = NumericDriverSetting(
		"headSize",
		# Translators: Label for a slider in the voice settings. The size of
		# the head, as a percentage: a larger head lowers the formants, as a
		# longer vocal tract does.
		_("H&ead size"),
		defaultVal=100,
		minVal=64,
		maxVal=135,
	)
	_roughnessSetting = NumericDriverSetting(
		"roughness",
		# Translators: Label for a slider in the voice settings. How uneven
		# the voice is from one pulse to the next.
		_("&Roughness"),
		defaultVal=0,
		minVal=0,
		maxVal=100,
	)
	_breathinessSetting = NumericDriverSetting(
		"breathiness",
		# Translators: Label for a slider in the voice settings. The level of
		# breath noise through the voice, in dB.
		_("&Breathiness"),
		defaultVal=0,
		minVal=0,
		maxVal=72,
	)
	_openQuotientSetting = NumericDriverSetting(
		"openQuotient",
		# Translators: Label for a slider in the voice settings. How long the
		# vocal folds stay open in each cycle, as a percentage: a longer open
		# phase sounds fuller and less buzzy.
		_("&Open quotient"),
		defaultVal=55,
		minVal=20,
		maxVal=90,
	)
	_smoothnessSetting = NumericDriverSetting(
		"smoothness",
		# Translators: Label for a slider in the voice settings. How soft the
		# voice is: more takes the edge off it, but not off consonants.
		_("&Smoothness"),
		defaultVal=0,
		minVal=0,
		maxVal=100,
	)
	_femaleVocalTractSetting = BooleanDriverSetting(
		"femaleVocalTract",
		# Translators: Label for a check box in the voice settings. When
		# checked, the voice uses the female model of the vocal tract, with
		# its higher formants, whatever voice is chosen.
		_("&Female vocal tract"),
		defaultVal=False,
	)
	_baselineFallSetting = NumericDriverSetting(
		"baselineFall",
		# Translators: Label for a slider in the voice settings. How far the
		# pitch's baseline falls through each phrase.
		_("Baseline &fall"),
		defaultVal=18,
		minVal=0,
		maxVal=90,
	)
	_hatRiseSetting = NumericDriverSetting(
		"hatRise",
		# Translators: Label for a slider in the voice settings. How high the
		# slow rise over each phrase goes.
		_("&Hat rise"),
		defaultVal=18,
		minVal=0,
		maxVal=100,
	)
	_stressRiseSetting = NumericDriverSetting(
		"stressRise",
		# Translators: Label for a slider in the voice settings. The size of
		# the accent on each stressed word.
		_("&Stress rise"),
		defaultVal=32,
		minVal=0,
		maxVal=100,
	)
	_quicknessSetting = NumericDriverSetting(
		"quickness",
		# Translators: Label for a slider in the voice settings. How quickly
		# the pitch responds: a larger larynx is slower.
		_("&Quickness"),
		defaultVal=40,
		minVal=0,
		maxVal=100,
	)
	_laxBreathinessSetting = NumericDriverSetting(
		"laxBreathiness",
		# Translators: Label for a slider in the voice settings. Breath that
		# comes up and softens the voice only at the ends of sentences and
		# just before voiceless sounds.
		_("La&x breathiness"),
		defaultVal=0,
		minVal=0,
		maxVal=100,
	)
	_sampleRateSetting = DriverSetting(
		"sampleRate",
		# Translators: Label for a combo box in the voice settings. The rate
		# the synthesizer runs at, in Hz: higher rates carry more of the hiss
		# of S and F, lower ones sound duller.
		_("Sample &rate"),
		defaultVal=str(DEFAULT_SAMPLE_RATE),
	)
	supportedSettings = (
		synthDriverHandler.SynthDriver.VoiceSetting(),
		_rateSetting,
		_rateWpmSetting,
		_rateInWpmSetting,
		_pitchSetting,
		_pitchHertzSetting,
		_pitchInHertzSetting,
		_inflectionSetting,
		_volumeSetting,
		_expandAbbreviationsSetting,
		_phrasePredictionSetting,
		_flutterSetting,
		_headSizeSetting,
		_roughnessSetting,
		_breathinessSetting,
		_openQuotientSetting,
		_smoothnessSetting,
		_femaleVocalTractSetting,
		_baselineFallSetting,
		_hatRiseSetting,
		_stressRiseSetting,
		_quicknessSetting,
		_laxBreathinessSetting,
		_sampleRateSetting,
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
		self._engine = None
		self._engineRate = None
		self._player = None
		self._sampleRate = DEFAULT_SAMPLE_RATE
		# The voices are defined in the engine, the default first. Each is known
		# to NVDA by its short name in lower case, which is what gets saved, and
		# shown by its full name.
		voices = range(self._lib.klattara_voice_count())
		self._voiceIds = [self._lib.klattara_voice_name(index).decode("utf-8").lower() for index in voices]
		self._voiceNames = [self._lib.klattara_voice_full_name(index).decode("utf-8") for index in voices]
		self._voice = self._voiceIds[0]
		self._rate = self._rateSetting.defaultVal
		self._rateWpm = self._rateWpmSetting.defaultVal
		self._rateInWpm = self._rateInWpmSetting.defaultVal
		self._pitch = self._pitchSetting.defaultVal
		self._pitchHertz = self._pitchHertzSetting.defaultVal
		self._pitchInHertz = self._pitchInHertzSetting.defaultVal
		# True while settings load, so a restored slider value does not switch
		# the precise modes off. Set by loadSettings.
		self._loadingSettings = False
		self._inflection = self._inflectionSetting.defaultVal
		self._volume = self._volumeSetting.defaultVal
		self._expandAbbreviations = self._expandAbbreviationsSetting.defaultVal
		self._phrasePrediction = self._phrasePredictionSetting.defaultVal
		self._flutter = self._flutterSetting.defaultVal
		# The first voice's own quality, as _set_voice sets it for any voice.
		(
			self._headSize,
			self._roughness,
			self._breathiness,
			self._openQuotient,
			self._smoothness,
			self._baselineFall,
			self._hatRise,
			self._stressRise,
			self._quickness,
			self._laxBreathiness,
			self._femaleVocalTract,
		) = VOICE_DEFAULTS[0]
		self._ensureAudio()
		if not self._engine:
			raise RuntimeError("Klattara could not create an engine")
		self._generation = 0
		self._speaking = None
		self._queue = queue.Queue()
		# Keep a reference so the callback outlives every call into the DLL.
		self._audioCallback = KLATTARA_AUDIO_CALLBACK(self._onAudio)
		self._thread = threading.Thread(target=self._run, name="Klattara", daemon=True)
		self._thread.start()

	def loadSettings(self, onlyChanged=False):
		# While settings load, a restored slider value must not switch the
		# precise modes off: the saved values are being restored, not the user
		# moving a slider. Reloq-C uses the same guard
		# (_loading_voice_settings). An incremental load with a value that
		# really changed is a user change, so it is not guarded.
		if not onlyChanged:
			self._loadingSettings = True
		try:
			super().loadSettings(onlyChanged)
		finally:
			self._loadingSettings = False

	def _ensureAudio(self):
		"""Makes the engine and the player at the chosen sample rate, or
		remakes them when the rate has changed. Only the speech thread calls
		this while speaking, so nothing is speaking when it runs. The old
		player is closed only once the new one is ready, so a rate the engine
		will not take leaves the old one in place."""
		if self._engine is not None and self._engineRate == self._sampleRate:
			return
		engine = self._lib.klattara_create(self._sampleRate)
		if not engine:
			log.error(f"Klattara could not create an engine at {self._sampleRate} Hz")
			return
		player = nvwave.WavePlayer(
			channels=1,
			samplesPerSec=self._sampleRate,
			bitsPerSample=BYTES_PER_SAMPLE * 8,
			outputDevice=config.conf["audio"]["outputDevice"],
		)
		# NVDA's player trims leading silence off every utterance, and it
		# re-arms after each one, so every short announcement in the settings
		# dialog gets its quiet start eaten: at 16000 Hz the Volume slider
		# loses its first words, and at 24000 Hz and above the same happens
		# to the rate and voice announcements, while 22050 Hz stays clean.
		# Klattara's audio starts within about 2 ms, so there is no silence
		# to trim, and turning it off leaves the beginning of every
		# utterance whole.
		player.enableTrimmingLeadingSilence(False)
		oldEngine, oldPlayer = self._engine, self._player
		self._engine, self._engineRate, self._player = engine, self._sampleRate, player
		if oldPlayer is not None:
			oldPlayer.close()
		if oldEngine is not None:
			self._lib.klattara_destroy(oldEngine)

	def terminate(self):
		self._generation += 1
		self._queue.put(None)
		self._player.stop()
		self._thread.join(timeout=2)
		self._player.close()
		self._lib.klattara_destroy(self._engine)
		self._engine = None
		super().terminate()

	def _getAvailableVoices(self):
		return OrderedDict(
			(voiceId, VoiceInfo(voiceId, name, VOICE_LANGUAGE))
			for voiceId, name in zip(self._voiceIds, self._voiceNames)
		)

	def _getAvailableSamplerates(self):
		# NVDA's dialog shows a plain DriverSetting as a combo box, with its
		# options from the available... property, as it does for voices.
		# The name is availableSamplerates, not availableSampleRates: the
		# dialog builds it as f"available{setting.id.capitalize()}s", and
		# "sampleRate".capitalize() is "Samplerate", so a capital R here
		# would make the lookup miss and the control never appear.
		return OrderedDict(
			(str(rate), StringParameterInfo(str(rate), f"{rate} Hz"))
			for rate in SAMPLE_RATES
		)

	def _get_availableSamplerates(self):
		if not hasattr(self, "_availableSamplerates"):
			self._availableSamplerates = self._getAvailableSamplerates()
		return self._availableSamplerates

	def _get_voice(self):
		return self._voice

	def _set_voice(self, value):
		# NVDA catches this when a saved voice no longer exists, and moves on to
		# the current one.
		if value not in self._voiceIds:
			raise ValueError(f"Unknown voice: {value}")
		self._voice = value
		# Pitch and inflection are stored with the voice, so choosing one sets both
		# sliders, as DECtalk's driver for NVDA does. NVDA updates the sliders after a
		# change of voice, and on loading it sets the voice first, then the saved values.
		voice = self._voiceIds.index(value)
		self._pitch = self._paramToPercent(self._lib.klattara_voice_pitch(voice), MIN_PITCH, MAX_PITCH)
		# When the precise pitch is off, the hertz value follows the voice, so
		# turning it on later starts at the new voice's own pitch. When it is
		# on, the hertz value is absolute and stays, as in Reloq-C.
		if not getattr(self, "_pitchInHertz", False):
			self._pitchHertz = self._lib.klattara_voice_pitch(voice)
		self._inflection = self._paramToPercent(
			self._lib.klattara_voice_inflection(voice) * 100,
			MIN_INFLECTION,
			MAX_INFLECTION,
		)
		# The voice's own quality settings, as with pitch and inflection.
		# Flutter stays where the user put it: off, unless turned on.
		(
			self._headSize,
			self._roughness,
			self._breathiness,
			self._openQuotient,
			self._smoothness,
			self._baselineFall,
			self._hatRise,
			self._stressRise,
			self._quickness,
			self._laxBreathiness,
			self._femaleVocalTract,
		) = VOICE_DEFAULTS[voice]

	# Settings are percentages, as NVDA keeps them. The speech thread reads
	# them before each piece of text, so only that thread touches the engine.

	def _get_rate(self):
		return self._rate

	def _set_rate(self, value):
		self._rate = value
		# Moving the rate slider hands the speed back to the normal scale,
		# as in Reloq-C. Loading a saved rate does not: _loadingSettings is
		# set while settings load.
		if self._rateInWpm and not getattr(self, "_loadingSettings", False):
			self._rateInWpm = False
			try:
				import ui

				ui.message(_("WPM rate off; using the normal rate scale."))
			except Exception:
				pass

	# The precise rate and pitch are plain attributes, not properties: their
	# setting ids start with an underscore, so NVDA's dialog hides them and
	# no _get__/_set__ property is made for them, as in Reloq-C. The Klattara
	# settings panel sets them through configure_rate_wpm and
	# configure_pitch_hertz, and _say reads them before each utterance.

	def configure_rate_wpm(self, enabled, value):
		"""The exact rate, as the Klattara settings panel applies it live.
		Raises ValueError for anything outside 38 to 1796 words per minute."""
		if type(enabled) is not bool or type(value) is not int or not MIN_WPM <= value <= MAX_WPM:
			raise ValueError("invalid WPM rate setting")
		self._rateWpm, self._rateInWpm = value, enabled

	def configure_pitch_hertz(self, enabled, value):
		"""The exact pitch, as the Klattara settings panel applies it live.
		Raises ValueError for anything outside 40 to 550 hertz."""
		if type(enabled) is not bool or type(value) is not int or not MIN_PITCH <= value <= MAX_PITCH:
			raise ValueError("invalid Hertz pitch setting")
		self._pitchHertz, self._pitchInHertz = value, enabled

	def _get_pitch(self):
		return self._pitch

	def _set_pitch(self, value):
		self._pitch = value
		# Moving the pitch slider hands the pitch back to the normal scale,
		# as in Reloq-C. Loading a saved pitch does not.
		if self._pitchInHertz and not getattr(self, "_loadingSettings", False):
			self._pitchInHertz = False
			try:
				import ui

				ui.message(_("Hertz pitch off; using the normal pitch scale."))
			except Exception:
				pass

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

	def _get_flutter(self):
		return self._flutter

	def _set_flutter(self, value):
		self._flutter = value

	def _get_headSize(self):
		return self._headSize

	def _set_headSize(self, value):
		self._headSize = value

	def _get_roughness(self):
		return self._roughness

	def _set_roughness(self, value):
		self._roughness = value

	def _get_breathiness(self):
		return self._breathiness

	def _set_breathiness(self, value):
		self._breathiness = value

	def _get_openQuotient(self):
		return self._openQuotient

	def _set_openQuotient(self, value):
		self._openQuotient = value

	def _get_smoothness(self):
		return self._smoothness

	def _set_smoothness(self, value):
		self._smoothness = value

	def _get_femaleVocalTract(self):
		return self._femaleVocalTract

	def _set_femaleVocalTract(self, value):
		self._femaleVocalTract = value

	def _get_baselineFall(self):
		return self._baselineFall

	def _set_baselineFall(self, value):
		self._baselineFall = value

	def _get_hatRise(self):
		return self._hatRise

	def _set_hatRise(self, value):
		self._hatRise = value

	def _get_stressRise(self):
		return self._stressRise

	def _set_stressRise(self, value):
		self._stressRise = value

	def _get_quickness(self):
		return self._quickness

	def _set_quickness(self, value):
		self._quickness = value

	def _get_laxBreathiness(self):
		return self._laxBreathiness

	def _set_laxBreathiness(self, value):
		self._laxBreathiness = value

	def _get_sampleRate(self):
		return str(self._sampleRate)

	def _set_sampleRate(self, value):
		# The combo box stores the rate as its id string; the slider stored
		# an integer, and a saved value may be any rate at all. Take the
		# nearest supported rate, so an old value still lands on a real one.
		try:
			rate = int(value)
		except (TypeError, ValueError):
			rate = DEFAULT_SAMPLE_RATE
		self._sampleRate = min(SAMPLE_RATES, key=lambda supported: abs(supported - rate))

	def speak(self, speechSequence):
		generation = self._generation
		spelled = False
		# Prosody commands, as percentages by setting name, that replace the
		# configured value until they return to default.
		prosody = {}
		text = []

		def flush():
			joined = "".join(text)
			# A screen reader reads whatever is on the screen, so a command in that text
			# must not reach the engine. Without its opening, the rest is just words,
			# which you still hear. Spelled text is read character by character, and
			# has no commands.
			if not spelled:
				joined = joined.replace(COMMAND_START, " ")
			joined = joined.strip()
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
			self._ensureAudio()
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
					samples = self._sampleRate * payload // 1000
					self._player.feed(bytes(samples * BYTES_PER_SAMPLE))
				elif kind == "done":
					self._player.idle()
					if generation == self._generation:
						synthDoneSpeaking.notify(synth=self)
			except Exception:
				log.error("Klattara speech thread", exc_info=True)

	def _pitchAt(self, voice, percent):
		"""The Hz of a pitch slider position. The slider's steps are 5.1 Hz, so the
		position nearest the voice's own pitch gives exactly that pitch."""
		own = self._lib.klattara_voice_pitch(voice)
		if percent == self._paramToPercent(own, MIN_PITCH, MAX_PITCH):
			return own
		return self._percentToParam(percent, MIN_PITCH, MAX_PITCH)

	def _say(self, generation, text, spelled, prosody):
		lib, engine = self._lib, self._engine
		voice = self._voiceIds.index(self._voice)
		lib.klattara_set_voice(engine, voice)
		if self._rateInWpm:
			# The precise rate owns the speed: the engine's measured table
			# turns words per minute into its rate multiplier. A prosody rate
			# command does not override it, as in Reloq-C; the in-text
			# [:speed] command still works, as the engine reads it.
			rateMult = _rateForWpm(self._rateWpm)
		else:
			rate = self._percentToParam(prosody.get("rate", self._rate), MIN_RATE, MAX_RATE)
			rateMult = min(MAX_RATE, max(MIN_RATE, rate)) / 100
		if self._pitchInHertz:
			# The precise pitch is an exact frequency in Hz.
			pitch = min(MAX_PITCH, max(MIN_PITCH, self._pitchHertz))
		else:
			pitch = self._pitchAt(voice, prosody.get("pitch", self._pitch))
		inflection = self._percentToParam(self._inflection, MIN_INFLECTION, MAX_INFLECTION)
		volume = self._percentToParam(prosody.get("volume", self._volume), MIN_VOLUME, MAX_VOLUME)
		# A prosody command can go past the slider's range; keep within the
		# engine's.
		lib.klattara_set_rate(engine, rateMult)
		lib.klattara_set_pitch(engine, min(MAX_PITCH, max(MIN_PITCH, pitch)))
		lib.klattara_set_inflection(engine, inflection / 100)
		lib.klattara_set_volume(engine, min(MAX_VOLUME, max(MIN_VOLUME, volume)) / 100)
		lib.klattara_set_abbreviations(engine, 1 if self._expandAbbreviations else 0)
		lib.klattara_set_phrase_prediction(engine, 1 if self._phrasePrediction else 0)
		lib.klattara_set_gender(engine, KLATTARA_FEMALE if self._femaleVocalTract else KLATTARA_MALE)
		# The voice-quality settings have no functions of their own, so they
		# reach the engine as one [:cv] command, spoken on its own. A command
		# on its own makes no audio, and a change of voice replaces these
		# settings, so this runs after every set_voice, before the text.
		custom = (
			f"[:cv hs {self._headSize} rg {self._roughness} br {self._breathiness} "
			f"oq {self._openQuotient} sm {self._smoothness} fl {self._flutter} "
			f"fa {self._baselineFall} ha {self._hatRise} st {self._stressRise} "
			f"qk {self._quickness} lx {self._laxBreathiness}]"
		)
		lib.klattara_speak(engine, custom.encode("utf-8"), KLATTARA_READ_TEXT, _DISCARD_CALLBACK, None)
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
