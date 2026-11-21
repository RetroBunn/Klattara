package com.dgomeze42.klattara;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.OutputStream;

/** Speech saved as a 16-bit mono WAV file at the chosen sample rate. */
final class Wav {
    private Wav() {}

    /** Writes the text spoken with the voice and settings, and closes out. Returns its length in seconds. */
    static double write(Synthesizer synthesizer, String text, Voices.Voice voice, OutputStream out) throws IOException {
        final ByteArrayOutputStream pcm = new ByteArrayOutputStream();
        int rate = synthesizer.sampleRate();
        int result = synthesizer.speak(text, voice, 1.0, 1.0, 8192, new KlattaraNative.AudioSink() {
            public int onAudio(byte[] data, int length) {
                pcm.write(data, 0, length);
                return 0;
            }
        });
        if (result < 0) throw new IOException("Klattara could not speak the text");
        byte[] data = pcm.toByteArray();
        try {
            out.write(header(rate, data.length));
            out.write(data);
        } finally {
            out.close();
        }
        return data.length / 2.0 / rate;
    }

    static byte[] header(int rate, int dataBytes) {
        byte[] h = new byte[44];
        put(h, 0, "RIFF");
        int32(h, 4, 36 + dataBytes);
        put(h, 8, "WAVE");
        put(h, 12, "fmt ");
        int32(h, 16, 16);
        int16(h, 20, 1);          // PCM
        int16(h, 22, 1);          // mono
        int32(h, 24, rate);
        int32(h, 28, rate * 2);   // bytes per second
        int16(h, 32, 2);          // bytes per frame
        int16(h, 34, 16);         // bits per sample
        put(h, 36, "data");
        int32(h, 40, dataBytes);
        return h;
    }

    private static void put(byte[] b, int at, String s) {
        for (int i = 0; i < 4; i++) b[at + i] = (byte) s.charAt(i);
    }

    private static void int16(byte[] b, int at, int v) {
        b[at] = (byte) v;
        b[at + 1] = (byte) (v >> 8);
    }

    private static void int32(byte[] b, int at, int v) {
        int16(b, at, v);
        int16(b, at + 2, v >> 16);
    }
}
