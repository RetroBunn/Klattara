package com.dgomeze42.klattara;

import android.content.Context;
import android.content.SharedPreferences;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * Klattara's settings, shared by the settings screen and the speech service (one
 * process, so both see every change at once). Reading is cheap enough for every
 * utterance: SharedPreferences keeps them in memory.
 */
final class Settings {
    static final int MIN_WPM = 30, MAX_WPM = 1920, NORMAL_WPM = 190;
    static final int MIN_PITCH = 40, MAX_PITCH = 550;
    static final int MAX_INFLECTION = 250, MAX_VOLUME = 200, MAX_FLUTTER = 100;
    static final int DEFAULT_SAMPLE_RATE = 22050;
    /** Every rate the engine is tuned for, the same twenty as on Windows and the Mac. */
    static final int[] SAMPLE_RATES = {
            8000, 10000, 11000, 11025, 12000, 16000, 20000, 22000, 22050, 24000,
            32000, 40000, 44000, 44100, 48000, 64000, 80000, 88000, 88200, 96000,
    };

    private final SharedPreferences prefs;

    Settings(Context context) {
        prefs = context.getApplicationContext().getSharedPreferences("klattara", Context.MODE_PRIVATE);
    }

    private static int clamp(int v, int lo, int hi) {
        return Math.max(lo, Math.min(hi, v));
    }

    private int getInt(String key, int def, int lo, int hi) {
        return clamp(prefs.getInt(key, def), lo, hi);
    }

    private void put(String key, int value) {
        prefs.edit().putInt(key, value).apply();
    }

    private void put(String key, boolean value) {
        prefs.edit().putBoolean(key, value).apply();
    }

    /** The voice profile: the voice Klattara speaks with everywhere, on every Android version. */
    int voice() { return getInt("voice", 0, 0, 1000); }
    void setVoice(int v) { put("voice", v); }

    int rateWpm() { return getInt("rateWpm", NORMAL_WPM, MIN_WPM, MAX_WPM); }
    void setRateWpm(int v) { put("rateWpm", clamp(v, MIN_WPM, MAX_WPM)); }
    /** Let the screen reader's rate scale the rate. */
    boolean followSystemRate() { return prefs.getBoolean("followRate", true); }
    void setFollowSystemRate(boolean v) { put("followRate", v); }

    boolean exactPitch() { return prefs.getBoolean("exactPitch", false); }
    void setExactPitch(boolean v) { put("exactPitch", v); }
    int pitchHz() { return getInt("pitchHz", 110, MIN_PITCH, MAX_PITCH); }
    void setPitchHz(int v) { put("pitchHz", clamp(v, MIN_PITCH, MAX_PITCH)); }
    boolean followSystemPitch() { return prefs.getBoolean("followPitch", true); }
    void setFollowSystemPitch(boolean v) { put("followPitch", v); }

    int inflection() { return getInt("inflection", 100, 0, MAX_INFLECTION); }
    void setInflection(int v) { put("inflection", clamp(v, 0, MAX_INFLECTION)); }
    int volume() { return getInt("volume", 100, 0, MAX_VOLUME); }
    void setVolume(int v) { put("volume", clamp(v, 0, MAX_VOLUME)); }
    /** Off unless turned on. */
    int flutter() { return getInt("flutter", 0, 0, MAX_FLUTTER); }
    void setFlutter(int v) { put("flutter", clamp(v, 0, MAX_FLUTTER)); }

    boolean abbreviations() { return prefs.getBoolean("abbreviations", true); }
    void setAbbreviations(boolean v) { put("abbreviations", v); }
    boolean phrasePrediction() { return prefs.getBoolean("phrasePrediction", false); }
    void setPhrasePrediction(boolean v) { put("phrasePrediction", v); }

    /**
     * The highest rate offered: 48 kHz, which every Android's audio output takes.
     * Some newer devices take more, but not all, so the engine's higher rates are
     * left out here.
     */
    static final int HIGHEST_SAMPLE_RATE = 48000;

    /** The rates offered, 8000 to 48000 hertz. */
    static int[] sampleRates() {
        int n = 0;
        for (int r : SAMPLE_RATES) if (r <= HIGHEST_SAMPLE_RATE) n++;
        int[] out = new int[n];
        int i = 0;
        for (int r : SAMPLE_RATES) if (r <= HIGHEST_SAMPLE_RATE) out[i++] = r;
        return out;
    }

    int sampleRate() {
        int r = prefs.getInt("sampleRate", DEFAULT_SAMPLE_RATE);
        if (r > HIGHEST_SAMPLE_RATE) return HIGHEST_SAMPLE_RATE;
        for (int s : SAMPLE_RATES) if (s == r) return r;
        return DEFAULT_SAMPLE_RATE;
    }
    void setSampleRate(int v) { put("sampleRate", v); }

    // ---- Each voice's customizations -----------------------------------------

    Map<String, Double> parameters(Voices.Voice voice) {
        Map<String, Double> out = new HashMap<String, Double>();
        String json = prefs.getString("params." + voice.key(), null);
        if (json == null) return out;
        try {
            JSONObject o = new JSONObject(json);
            Iterator<String> keys = o.keys();
            while (keys.hasNext()) {
                String code = keys.next();
                Parameters.Parameter p = Parameters.named(code);
                double v = o.optDouble(code, Double.NaN);
                if (p != null && !Double.isNaN(v) && !Double.isInfinite(v)) out.put(code, p.clamp(v));
            }
        } catch (JSONException ignored) {
        }
        return out;
    }

    /** Sets one, or with null puts it back to the voice's own. */
    void setParameter(Voices.Voice voice, String code, Double value) {
        Map<String, Double> own = parameters(voice);
        if (value == null) {
            own.remove(code);
        } else {
            own.put(code, value);
            String partner = Parameters.partner(code);
            if (partner != null) own.remove(partner);
        }
        saveParameters(voice, own);
    }

    void resetParameters(Voices.Voice voice) {
        prefs.edit().remove("params." + voice.key()).apply();
    }

    private void saveParameters(Voices.Voice voice, Map<String, Double> own) {
        if (own.isEmpty()) {
            resetParameters(voice);
            return;
        }
        JSONObject o = new JSONObject();
        try {
            for (Map.Entry<String, Double> e : own.entrySet()) o.put(e.getKey(), e.getValue().doubleValue());
        } catch (JSONException ignored) {
        }
        prefs.edit().putString("params." + voice.key(), o.toString()).apply();
    }

    // ---- Pronunciation dictionaries ----------------------------------------------

    static final class DictionaryEntry {
        final String file;
        final boolean enabled;

        DictionaryEntry(String file, boolean enabled) {
            this.file = file;
            this.enabled = enabled;
        }
    }

    /** The dictionaries in load order: a later one wins for the same word. */
    List<DictionaryEntry> dictionaries() {
        List<DictionaryEntry> out = new ArrayList<DictionaryEntry>();
        try {
            JSONArray a = new JSONArray(prefs.getString("dictionaries", "[]"));
            for (int i = 0; i < a.length(); i++) {
                JSONObject o = a.getJSONObject(i);
                out.add(new DictionaryEntry(o.getString("file"), o.optBoolean("enabled", true)));
            }
        } catch (JSONException ignored) {
        }
        return out;
    }

    void setDictionaries(List<DictionaryEntry> list) {
        JSONArray a = new JSONArray();
        try {
            for (DictionaryEntry d : list) a.put(new JSONObject().put("file", d.file).put("enabled", d.enabled));
        } catch (JSONException ignored) {
        }
        prefs.edit().putString("dictionaries", a.toString())
                .putInt("dictionaryVersion", dictionaryVersion() + 1).apply();
    }

    /** Moves on whenever the dictionaries change, so the engine reloads them. */
    int dictionaryVersion() { return prefs.getInt("dictionaryVersion", 0); }
}
