package com.dgomeze42.klattara;

import android.annotation.TargetApi;
import android.content.Context;
import android.os.Build;
import android.os.Bundle;
import android.os.SystemClock;
import android.speech.tts.TextToSpeech;
import android.speech.tts.UtteranceProgressListener;
import android.util.Log;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * A check of Klattara through Android's own text-to-speech service, for testing
 * without a person listening: each voice profile is saved to a file through the
 * service, at normal speed and at double, the files are compared, the sampling
 * rate is checked, and how soon the first audio arrives is timed. The settings are
 * put back afterwards. Results go to the log under "KlattaraSelfTest", the files to
 * the app's external files folder. Started with
 *   adb shell am start -a com.dgomeze42.klattara.SELF_TEST -n com.dgomeze42.klattara/.SettingsActivity
 */
@TargetApi(21)
final class SelfTest {
    static final String TAG = "KlattaraSelfTest";
    private static final String TEXT = "Hello from Klattara on Android. The quick brown fox jumps over the lazy dog.";

    private SelfTest() {}

    static void run(final Context context) {
        if (Build.VERSION.SDK_INT < 21) {
            Log.i(TAG, "needs Android 5.0 or newer");
            return;
        }
        Log.i(TAG, "engine " + KlattaraNative.version() + ", " + Voices.all().size() + " voices");
        long d = KlattaraNative.dictionaryNew();
        int[] counts = KlattaraNative.dictionaryLoad(d, "# test\nhello HH AH0 L OW1\nnvda = en vee dee ay\nbad B QQ1 D\n".getBytes(Parameters.UTF8));
        byte[] applied = KlattaraNative.dictionaryApply(d, "Hello NVDA [:volume 0] bad".getBytes(Parameters.UTF8));
        KlattaraNative.dictionaryFree(d);
        String out = applied == null ? "null" : new String(applied, Parameters.UTF8);
        boolean ok = counts != null && counts[0] == 2 && counts[1] == 1 && counts[2] == 4
                && out.equals("[:phone klattara enable][HH AH0 L OW1] en vee dee ay (:volume 0) bad[:phone klattara disable]");
        Log.i(TAG, (ok ? "OK" : "FAIL") + " dictionary: " + out);

        final TextToSpeech[] holder = new TextToSpeech[1];
        holder[0] = new TextToSpeech(context, new TextToSpeech.OnInitListener() {
            public void onInit(int status) {
                if (status != TextToSpeech.SUCCESS) {
                    Log.e(TAG, "FAIL: the speech service did not start (" + status + ")");
                    return;
                }
                new Run(context, holder[0]).next();
            }
        }, context.getPackageName());
    }

    /** One recording: a voice profile, a speed and a sampling rate. */
    private static final class Job {
        final int voice;
        final float speed;
        final int sampleRate;

        Job(int voice, float speed, int sampleRate) {
            this.voice = voice;
            this.speed = speed;
            this.sampleRate = sampleRate;
        }

        String name() {
            return Voices.get(voice).key() + "-x" + (int) speed + "-" + sampleRate;
        }
    }

    private static final class Run extends UtteranceProgressListener {
        private final Context context;
        private final TextToSpeech tts;
        private final Settings settings;
        private final int savedVoice, savedRate;
        private final List<Job> jobs = new ArrayList<Job>();
        private final Set<String> normalSpeedDigests = new HashSet<String>();
        private int normalSpeedFiles;
        private long started, firstAudio;
        private int index = -1;

        Run(Context context, TextToSpeech tts) {
            this.context = context;
            this.tts = tts;
            this.settings = new Settings(context);
            savedVoice = settings.voice();
            savedRate = settings.sampleRate();
            tts.setOnUtteranceProgressListener(this);
            for (int v = 0; v < Voices.all().size(); v++) {
                jobs.add(new Job(v, 1.0f, Settings.DEFAULT_SAMPLE_RATE));
                jobs.add(new Job(v, 2.0f, Settings.DEFAULT_SAMPLE_RATE));
            }
            // The lowest rate, 48 kHz, and the rates above it, through the service.
            jobs.add(new Job(0, 1.0f, 8000));
            for (int rate : Settings.sampleRates()) if (rate >= 48000) jobs.add(new Job(0, 1.0f, rate));
            Log.i(TAG, "sampling rates offered: " + Settings.sampleRates().length + ", highest " + Settings.highestSampleRate());
        }

        private File file(Job j) {
            return new File(context.getExternalFilesDir(null), j.name() + ".wav");
        }

        void next() {
            index++;
            if (index >= jobs.size()) {
                settings.setVoice(savedVoice);
                settings.setSampleRate(savedRate);
                Log.i(TAG, (normalSpeedDigests.size() == normalSpeedFiles ? "OK" : "FAIL") + " voice profiles: "
                        + normalSpeedDigests.size() + " different recordings from " + normalSpeedFiles + " profiles");
                Log.i(TAG, "DONE");
                tts.shutdown();
                return;
            }
            Job j = jobs.get(index);
            settings.setVoice(j.voice);
            settings.setSampleRate(j.sampleRate);
            tts.setSpeechRate(j.speed);
            started = SystemClock.elapsedRealtime();
            firstAudio = 0;
            tts.synthesizeToFile(TEXT, new Bundle(), file(j), String.valueOf(index));
        }

        @Override
        public void onStart(String id) {}

        @Override
        public void onAudioAvailable(String id, byte[] audio) {
            if (firstAudio == 0) firstAudio = SystemClock.elapsedRealtime();
        }

        @Override
        public void onDone(String id) {
            Job j = jobs.get(index);
            File f = file(j);
            long total = SystemClock.elapsedRealtime() - started;
            int headerRate = wavRate(f);
            if (j.speed == 1.0f && j.sampleRate == Settings.DEFAULT_SAMPLE_RATE) {
                normalSpeedFiles++;
                normalSpeedDigests.add(digest(f));
            }
            boolean rateOk = headerRate == j.sampleRate;
            Log.i(TAG, (rateOk ? "OK " : "FAIL ") + Voices.get(j.voice).fullName + " x" + (int) j.speed + " at " + j.sampleRate
                    + " Hz: " + f.length() + " bytes, file says " + headerRate + " Hz, first audio after "
                    + (firstAudio == 0 ? -1 : firstAudio - started) + " ms, all in " + total + " ms");
            next();
        }

        @Override
        public void onError(String id) {
            Log.e(TAG, "FAIL utterance " + id);
            next();
        }

        private static int wavRate(File f) {
            byte[] h = new byte[28];
            try {
                FileInputStream in = new FileInputStream(f);
                try {
                    if (in.read(h) < 28) return -1;
                } finally {
                    in.close();
                }
            } catch (IOException e) {
                return -1;
            }
            return (h[24] & 0xff) | (h[25] & 0xff) << 8 | (h[26] & 0xff) << 16 | (h[27] & 0xff) << 24;
        }

        private static String digest(File f) {
            try {
                MessageDigest md = MessageDigest.getInstance("SHA-1");
                return android.util.Base64.encodeToString(md.digest(Dictionaries.read(f)), android.util.Base64.NO_WRAP);
            } catch (Exception e) {
                return f.getName();
            }
        }
    }
}
