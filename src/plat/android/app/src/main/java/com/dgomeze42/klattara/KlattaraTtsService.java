package com.dgomeze42.klattara;

import android.annotation.TargetApi;
import android.media.AudioFormat;
import android.os.Build;
import android.speech.tts.SynthesisCallback;
import android.speech.tts.SynthesisRequest;
import android.speech.tts.TextToSpeech;
import android.speech.tts.TextToSpeechService;
import android.speech.tts.Voice;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;

/**
 * Klattara as an Android text-to-speech engine, for TalkBack, other screen readers
 * and every app that speaks, on Android 4.0 and newer.
 *
 * Android calls onSynthesizeText on one thread, one request at a time; the engine
 * is kept between requests, and audio is handed over as it is made, so speech
 * starts at once. onStop comes on another thread and takes effect at the next
 * chunk. Nothing runs between requests.
 */
public class KlattaraTtsService extends TextToSpeechService {
    private static final String LANGUAGE = "eng";
    private static final String COUNTRY = "USA";
    /** What Android before 4.3 takes at once, which has no way to ask. */
    private static final int OLD_CHUNK_BYTES = 4096;

    private Synthesizer synthesizer;
    private volatile boolean stopRequested;

    @Override
    public void onCreate() {
        super.onCreate();
        synthesizer = new Synthesizer(this);
    }

    @Override
    public void onDestroy() {
        if (synthesizer != null) synthesizer.release();
        super.onDestroy();
    }

    // ---- Languages: American English, and English generally --------------------

    private static boolean isEnglish(String lang) {
        return lang != null && (lang.equalsIgnoreCase("eng") || lang.equalsIgnoreCase("en"));
    }

    private static boolean isUs(String country) {
        return country != null && (country.equalsIgnoreCase("USA") || country.equalsIgnoreCase("US"));
    }

    @Override
    protected int onIsLanguageAvailable(String lang, String country, String variant) {
        if (!isEnglish(lang)) return TextToSpeech.LANG_NOT_SUPPORTED;
        return isUs(country) ? TextToSpeech.LANG_COUNTRY_AVAILABLE : TextToSpeech.LANG_AVAILABLE;
    }

    @Override
    protected String[] onGetLanguage() {
        return new String[]{LANGUAGE, COUNTRY, ""};
    }

    @Override
    protected int onLoadLanguage(String lang, String country, String variant) {
        return onIsLanguageAvailable(lang, country, variant);
    }

    // ---- Voices (Android 5.0 and newer) -------------------------------------------

    @TargetApi(21)
    @Override
    public List<Voice> onGetVoices() {
        List<Voice> voices = new ArrayList<Voice>();
        for (Voices.Voice v : Voices.all()) {
            voices.add(new Voice(v.id(), Locale.US, Voice.QUALITY_HIGH, Voice.LATENCY_VERY_LOW, false,
                    new HashSet<String>()));
        }
        return voices;
    }

    @TargetApi(21)
    @Override
    public int onIsValidVoiceName(String voiceName) {
        return Voices.byId(voiceName) != null ? TextToSpeech.SUCCESS : TextToSpeech.ERROR;
    }

    @TargetApi(21)
    @Override
    public int onLoadVoice(String voiceName) {
        return onIsValidVoiceName(voiceName);
    }

    @TargetApi(21)
    @Override
    public String onGetDefaultVoiceNameFor(String lang, String country, String variant) {
        if (!isEnglish(lang)) return null;
        return Voices.get(new Settings(this).voice()).id();
    }

    // ---- Speaking ----------------------------------------------------------------------

    @Override
    protected void onStop() {
        stopRequested = true;
    }

    @Override
    protected void onSynthesizeText(SynthesisRequest request, final SynthesisCallback callback) {
        stopRequested = false;
        CharSequence text = Build.VERSION.SDK_INT >= 18 ? request.getCharSequenceText() : request.getText();
        if (text == null) text = "";
        if (!isEnglish(request.getLanguage()) && request.getLanguage() != null && !request.getLanguage().isEmpty()) {
            callback.error();
            return;
        }
        // The voice profile chosen in Klattara's settings, always. A screen reader
        // keeps the voice name it was given when it connected, so following the
        // request's voice would leave a change of profile unheard until it
        // reconnects; and some screen readers, on notetakers especially, never
        // choose a voice at all.
        Voices.Voice voice = Voices.get(synthesizer.settings().voice());

        int chunk = Build.VERSION.SDK_INT >= 18 ? callback.getMaxBufferSize() : OLD_CHUNK_BYTES;
        chunk = Math.max(256, Math.min(chunk, 8192));
        if (callback.start(synthesizer.sampleRate(), AudioFormat.ENCODING_PCM_16BIT, 1) != TextToSpeech.SUCCESS) return;

        int result = synthesizer.speak(text, voice, request.getSpeechRate() / 100.0, request.getPitch() / 100.0,
                chunk, new KlattaraNative.AudioSink() {
                    @Override
                    public int onAudio(byte[] pcm, int length) {
                        if (stopRequested) return 1;
                        return callback.audioAvailable(pcm, 0, length) == TextToSpeech.SUCCESS ? 0 : 1;
                    }
                });
        if (result < 0 && !stopRequested) {
            callback.error();
        } else {
            callback.done();
        }
    }
}
