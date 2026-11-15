package com.dgomeze42.klattara;

/**
 * The Klattara engine, in libklattara.so (src/plat/android/jni and rust). Text
 * crosses as UTF-8 bytes. An engine may be used from any thread, but from one at a
 * time.
 */
final class KlattaraNative {
    static {
        System.loadLibrary("klattara");
    }

    private KlattaraNative() {}

    static final int OK = 0;
    static final int STOPPED = 1;
    static final int FEMALE = 1;

    /** Receives 16-bit little-endian mono PCM; returns 0 to go on, anything else to stop. */
    interface AudioSink {
        int onAudio(byte[] pcm, int length);
    }

    static native long create(int sampleRate);
    static native void destroy(long engine);
    static native String version();

    static native int voiceCount();
    static native String voiceName(int index);
    static native String voiceFullName(int index);
    static native double voicePitch(int index);
    static native int voiceGender(int index);
    /** A voice's own value of a [:cv] setting; NaN where the engine works it out. */
    static native double voiceSetting(int index, byte[] code);

    static native int setVoice(long engine, int index);
    static native int setPitch(long engine, double hz);
    static native int setInflection(long engine, double inflection);
    static native int setVolume(long engine, double volume);
    static native int setRate(long engine, double rate);
    static native int setAbbreviations(long engine, boolean expand);
    static native int setPhrasePrediction(long engine, boolean predict);
    /** Commands only, such as "[:cv hs 90]": they make no sound. */
    static native int command(long engine, byte[] utf8);
    static native int speak(long engine, byte[] utf8, boolean spelled, int chunkBytes, AudioSink sink);

    static native long dictionaryNew();
    static native void dictionaryFree(long dictionary);
    /** {taken in, left out, first left-out line}, or null. */
    static native int[] dictionaryLoad(long dictionary, byte[] utf8);
    static native int dictionarySize(long dictionary);
    /** Text with the dictionary applied and brackets made safe; dictionary 0 only makes it safe. */
    static native byte[] dictionaryApply(long dictionary, byte[] utf8);
}
