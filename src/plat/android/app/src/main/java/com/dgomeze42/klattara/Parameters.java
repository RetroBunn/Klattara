package com.dgomeze42.klattara;

import java.nio.charset.Charset;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * The voice-quality settings Klattara documents for its [:cv] command (klattara.h),
 * as the app offers them per voice. Pitch (pi), inflection (in) and flutter (fl)
 * have controls of their own and are not repeated here.
 */
final class Parameters {
    static final class Parameter {
        final String code;
        final String name;
        final String help;
        final double min;
        final double max;
        final double step;
        final String unit;
        final String group;

        Parameter(String code, String name, String help, double min, double max, double step, String unit, String group) {
            this.code = code;
            this.name = name;
            this.help = help;
            this.min = min;
            this.max = max;
            this.step = step;
            this.unit = unit;
            this.group = group;
        }

        /** The voice's own value, or NaN where the engine works it out. */
        double ownValue(Voices.Voice voice) {
            return KlattaraNative.voiceSetting(voice.index, utf8(code));
        }

        double clamp(double value) {
            return Math.max(min, Math.min(max, value));
        }
    }

    static final Charset UTF8 = Charset.forName("UTF-8");

    /** UTF-8 bytes. String.getBytes(Charset) is Android 2.3; this is in every version. */
    static byte[] utf8(String text) {
        try {
            return text.getBytes("UTF-8");
        } catch (java.io.UnsupportedEncodingException e) {
            throw new AssertionError(e);
        }
    }

    static final String QUALITY = "Voice quality";
    static final String SOURCES = "Source levels";
    static final String MELODY = "Melody";
    static final String FORMANTS = "Upper formants";

    static final List<Parameter> ALL = Arrays.asList(
            new Parameter("ge", "Gender", "The model of the vocal tract, female or male. It changes nothing else, so set the pitch too.", 0, 1, 1, "", QUALITY),
            new Parameter("hs", "Head size", "Percent of normal. Smaller sounds younger and higher, larger deeper.", 64, 135, 1, "percent", QUALITY),
            new Parameter("rg", "Roughness", "From none to the most: unevenness from one pulse of the voice to the next.", 0, 100, 1, "", QUALITY),
            new Parameter("br", "Breathiness", "Breath running through the voice, in decibels.", 0, 72, 1, "decibels", QUALITY),
            new Parameter("lx", "Lax breathiness", "Breath at the ends of sentences, percent.", 0, 100, 1, "percent", QUALITY),
            new Parameter("rc", "Richness", "Percent. More is thinner and buzzier, less fuller. Sets the open quotient, from the other end.", 0, 100, 1, "percent", QUALITY),
            new Parameter("oq", "Open quotient", "Percent of each period the vocal folds are open. Sets the same thing as richness.", 20, 90, 1, "percent", QUALITY),
            new Parameter("sm", "Smoothness", "Percent. More softens the voice, but not the consonants.", 0, 100, 1, "percent", QUALITY),
            new Parameter("vvl", "Voicing level", "Percent of normal: vowels and everything the vocal folds sound. 0 is off.", 0, 200, 5, "percent", SOURCES),
            new Parameter("fvl", "Frication level", "Percent of normal: the hiss of S and F, and the bursts of stops.", 0, 200, 5, "percent", SOURCES),
            new Parameter("avl", "Aspiration level", "Percent of normal: H, and the puff after P, T and K.", 0, 200, 5, "percent", SOURCES),
            new Parameter("nvl", "Nasal level", "Percent of normal: M, N and NG.", 0, 200, 5, "percent", SOURCES),
            new Parameter("fa", "Baseline fall", "How far the pitch falls through a phrase, 0 to 90. 18 is normal.", 0, 90, 1, "", MELODY),
            new Parameter("ha", "Hat rise", "The slow rise over a phrase, 0 to 100. 18 is normal.", 0, 100, 1, "", MELODY),
            new Parameter("st", "Stress rise", "The size of each accent, 0 to 100. 32 is normal.", 0, 100, 1, "", MELODY),
            new Parameter("qk", "Quickness", "How quickly the pitch moves, percent. 40 is normal.", 0, 100, 1, "percent", MELODY),
            new Parameter("f4", "F4", "The fourth formant, in hertz. Automatic follows head size and gender.", 2000, 6000, 50, "hertz", FORMANTS),
            new Parameter("b4", "F4 bandwidth", "In hertz.", 100, 4000, 10, "hertz", FORMANTS),
            new Parameter("f5", "F5", "The fifth formant, in hertz.", 2500, 6000, 50, "hertz", FORMANTS),
            new Parameter("b5", "F5 bandwidth", "In hertz.", 100, 4000, 10, "hertz", FORMANTS),
            new Parameter("f6", "F6", "The sixth formant, in hertz.", 3000, 9000, 50, "hertz", FORMANTS),
            new Parameter("b6", "F6 bandwidth", "In hertz.", 100, 4000, 10, "hertz", FORMANTS));

    static final String[] GROUPS = {QUALITY, SOURCES, MELODY, FORMANTS};

    private Parameters() {}

    static Parameter named(String code) {
        for (Parameter p : ALL) if (p.code.equals(code)) return p;
        return null;
    }

    /** Richness and open quotient set the same thing: setting one clears the other. */
    static String partner(String code) {
        if ("rc".equals(code)) return "oq";
        if ("oq".equals(code)) return "rc";
        return null;
    }

    /** A whole number as such; anything else to full precision. */
    static String number(double value) {
        if (value == Math.rint(value) && Math.abs(value) < 1e9) return Long.toString((long) value);
        return String.format(Locale.US, "%.10g", value).replaceAll("0+$", "").replaceAll("\\.$", "");
    }

    /** One decimal at most, and none when it would be ".0". */
    static String shown(double value) {
        double tenths = Math.rint(value * 10) / 10;
        if (tenths == Math.rint(tenths)) return Long.toString((long) tenths);
        return String.format(Locale.US, "%.1f", tenths);
    }

    /** The [:cv] command for a voice's customizations, or "" if there are none. */
    static String command(Map<String, Double> custom) {
        StringBuilder b = new StringBuilder();
        for (Parameter p : ALL) {
            Double v = custom.get(p.code);
            if (v == null || v.isNaN()) continue;
            b.append(' ').append(p.code).append(' ').append(number(p.clamp(v)));
        }
        return b.length() == 0 ? "" : "[:cv" + b + "]";
    }
}
