package com.dgomeze42.klattara;

import android.content.Context;

import java.util.Map;

/**
 * Speaks with Klattara as the settings say: the speech service, the settings
 * screen's Speak button and its WAV export all use this, so all three sound alike.
 *
 * The engine is made once and kept (remade only when the sample rate changes), and
 * the dictionaries are loaded once and kept until they change, so an utterance
 * starts with nothing to set up but a few settings. Use one Synthesizer from one
 * thread at a time.
 */
final class Synthesizer {
    private final Context context;
    private final Settings settings;
    private long engine;
    private int engineRate;
    private long dictionary;
    private int dictionaryVersion = -1;

    Synthesizer(Context context) {
        this.context = context.getApplicationContext();
        this.settings = new Settings(this.context);
    }

    Settings settings() {
        return settings;
    }

    /** The rate the next utterance will come out at. */
    int sampleRate() {
        return settings.sampleRate();
    }

    private void prepare() {
        int rate = settings.sampleRate();
        if (engine == 0 || engineRate != rate) {
            if (engine != 0) KlattaraNative.destroy(engine);
            engine = KlattaraNative.create(rate);
            engineRate = rate;
        }
        int version = settings.dictionaryVersion();
        if (version != dictionaryVersion) {
            if (dictionary != 0) KlattaraNative.dictionaryFree(dictionary);
            dictionary = Dictionaries.build(context, settings);
            dictionaryVersion = version;
        }
    }

    /**
     * Speaks text, handing PCM to the sink in chunks of at most chunkBytes.
     *
     * @param rateScale  the screen reader's rate, 1 being normal (Android's 100)
     * @param pitchScale the screen reader's pitch, 1 being normal
     * @return KlattaraNative.OK, STOPPED, or a negative error
     */
    int speak(CharSequence text, Voices.Voice voice, double rateScale, double pitchScale,
              int chunkBytes, KlattaraNative.AudioSink sink) {
        prepare();
        if (engine == 0) return -2;
        KlattaraNative.setVoice(engine, voice.index);

        // Commands, after the voice (choosing one brings back its own values):
        // phoneme input off (a dictionary turns it on only around its own words),
        // flutter (off unless turned up), this voice's customizations, the rate.
        double scale = settings.followSystemRate() ? rateScale : 1.0;
        long wpm = Math.round(settings.rateWpm() * scale);
        wpm = Math.max(Settings.MIN_WPM, Math.min(Settings.MAX_WPM, wpm));
        Map<String, Double> custom = settings.parameters(voice);
        String commands = "[:phone klattara disable][:cv fl " + settings.flutter() + "]"
                + Parameters.command(custom) + "[:speed " + wpm + "]";
        KlattaraNative.command(engine, commands.getBytes(Parameters.UTF8));

        double pitch = settings.exactPitch() ? settings.pitchHz() : voice.pitch;
        if (settings.followSystemPitch()) pitch *= pitchScale;
        KlattaraNative.setPitch(engine, Math.max(Settings.MIN_PITCH, Math.min(Settings.MAX_PITCH, pitch)));
        KlattaraNative.setInflection(engine, settings.inflection() / 100.0);
        KlattaraNative.setVolume(engine, settings.volume() / 100.0);
        KlattaraNative.setAbbreviations(engine, settings.abbreviations());
        KlattaraNative.setPhrasePrediction(engine, settings.phrasePrediction());

        // Brackets in the text become parentheses, so nothing on screen can work
        // as a command, and dictionary words become their sounds.
        byte[] prepared = KlattaraNative.dictionaryApply(dictionary, text.toString().getBytes(Parameters.UTF8));
        if (prepared == null) return -1;
        return KlattaraNative.speak(engine, prepared, false, chunkBytes, sink);
    }

    void release() {
        if (engine != 0) KlattaraNative.destroy(engine);
        if (dictionary != 0) KlattaraNative.dictionaryFree(dictionary);
        engine = 0;
        dictionary = 0;
        dictionaryVersion = -1;
    }
}
