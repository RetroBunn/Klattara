package com.dgomeze42.klattara;

import android.annotation.TargetApi;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.DialogInterface;
import android.content.Intent;
import android.media.AudioFormat;
import android.media.AudioManager;
import android.media.AudioTrack;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.speech.tts.TextToSpeech;
import android.text.InputType;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.CompoundButton;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.TextView;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Klattara's settings, and a place to try them. Built from Android's standard
 * controls in code, so it works the same with TalkBack, a braille notetaker's keys
 * or a touch screen, from Android 4.0 on.
 */
public class SettingsActivity extends Activity {
    private static final int REQUEST_DICTIONARY = 1;
    private static final int REQUEST_WAV = 2;
    static final String ACTION_SELF_TEST = "com.dgomeze42.klattara.SELF_TEST";

    private Settings settings;
    private Synthesizer synthesizer;
    private final Handler main = new Handler(Looper.getMainLooper());
    private LinearLayout root;
    private LinearLayout voiceSettings;
    private LinearLayout dictionaryList;
    private EditText testText;
    private TextView status;
    private TextToSpeech systemTts;
    private volatile boolean stopDirect;
    private Thread directThread;
    private AudioTrack track;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        settings = new Settings(this);
        synthesizer = new Synthesizer(this);

        ScrollView scroll = new ScrollView(this);
        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        int pad = dp(16);
        root.setPadding(pad, pad, pad, pad);
        scroll.addView(root);
        setContentView(scroll);
        build();

        if (ACTION_SELF_TEST.equals(getIntent().getAction())) SelfTest.run(this);
    }

    @Override
    protected void onDestroy() {
        stopAll();
        if (systemTts != null) systemTts.shutdown();
        synthesizer.release();
        super.onDestroy();
    }

    // ---- The screen -------------------------------------------------------------------

    private void build() {
        heading("Klattara " + versionName());
        note("Choose Klattara in Android's text-to-speech settings (Settings, Accessibility, Text-to-speech output, Preferred engine), "
                + "then choose its voice below, under Voice profile. Everything here applies as soon as you change it.");

        heading("Voice profile");
        final List<Voices.Voice> voices = Voices.all();
        String[] voiceNames = new String[voices.size()];
        for (int i = 0; i < voiceNames.length; i++) voiceNames[i] = voices.get(i).fullName;
        choiceButton(root, "Voice profile", voiceNames, new Choice() {
            public int selected() { return Math.min(settings.voice(), voices.size() - 1); }
            public void picked(int index) {
                settings.setVoice(index);
                rebuildVoiceSettings();
            }
        });
        note("The voice Klattara speaks with everywhere: your screen reader, Android's speech settings and every app, "
                + "on every Android version. The voice settings further down are this voice's own.");

        heading("Rate");
        numberRow("Rate in words per minute", Settings.MIN_WPM, Settings.MAX_WPM, 10, "words per minute", new IntValue() {
            public int get() { return settings.rateWpm(); }
            public void set(int v) { settings.setRateWpm(v); }
        });
        checkBox("Follow the screen reader's rate", settings.followSystemRate(), new Toggled() {
            public void toggled(boolean on) { settings.setFollowSystemRate(on); }
        });
        note("On, the screen reader's rate scales this one. Off, always exactly this rate.");

        heading("Pitch");
        checkBox("Use base pitch in hertz", settings.exactPitch(), new Toggled() {
            public void toggled(boolean on) { settings.setExactPitch(on); }
        });
        numberRow("Base pitch in hertz", Settings.MIN_PITCH, Settings.MAX_PITCH, 5, "hertz", new IntValue() {
            public int get() { return settings.pitchHz(); }
            public void set(int v) { settings.setPitchHz(v); }
        });
        checkBox("Follow the screen reader's pitch", settings.followSystemPitch(), new Toggled() {
            public void toggled(boolean on) { settings.setFollowSystemPitch(on); }
        });
        numberRow("Inflection, percent of normal", 0, Settings.MAX_INFLECTION, 10, "percent", new IntValue() {
            public int get() { return settings.inflection(); }
            public void set(int v) { settings.setInflection(v); }
        });
        flutterRow();

        heading("Reading");
        numberRow("Volume, percent", 0, Settings.MAX_VOLUME, 10, "percent", new IntValue() {
            public int get() { return settings.volume(); }
            public void set(int v) { settings.setVolume(v); }
        });
        checkBox("Read abbreviations in full", settings.abbreviations(), new Toggled() {
            public void toggled(boolean on) { settings.setAbbreviations(on); }
        });
        checkBox("Phrase prediction", settings.phrasePrediction(), new Toggled() {
            public void toggled(boolean on) { settings.setPhrasePrediction(on); }
        });

        heading("Sampling rate");
        final int[] rates = Settings.sampleRates();
        String[] rateNames = new String[rates.length];
        for (int i = 0; i < rates.length; i++) {
            rateNames[i] = rates[i] + " hertz" + (rates[i] == Settings.DEFAULT_SAMPLE_RATE ? " (recommended)" : "");
        }
        choiceButton(root, "Sampling rate", rateNames, new Choice() {
            public int selected() {
                for (int i = 0; i < rates.length; i++) if (rates[i] == settings.sampleRate()) return i;
                return 0;
            }
            public void picked(int index) { settings.setSampleRate(rates[index]); }
        });
        note("The rate Klattara synthesizes at, from 8000 to 48000 hertz. Lower rates sound duller and older, higher ones brighter.");

        voiceSettings = new LinearLayout(this);
        voiceSettings.setOrientation(LinearLayout.VERTICAL);
        root.addView(voiceSettings);
        rebuildVoiceSettings();

        heading("Pronunciation dictionaries");
        note("Your own pronunciations, one a line: a word and its sounds in ARPAbet, as in \"aback AH0 B AE1 K\" "
                + "(the format of Klattara's dictionary and CMUdict), or a word, an equals sign and how to say it, "
                + "as in \"nvda = en vee dee ay\". Lines starting with # are comments. A later dictionary wins over an earlier one.");
        dictionaryList = new LinearLayout(this);
        dictionaryList.setOrientation(LinearLayout.VERTICAL);
        root.addView(dictionaryList);
        button("Add Dictionary File", new Runnable() {
            public void run() { chooseDictionary(); }
        });
        rebuildDictionaries();

        heading("Try it");
        testText = new EditText(this);
        testText.setText("Hello. This is Klattara, a Klatt-style formant synthesizer, speaking on Android.");
        testText.setHint("Text to speak");
        testText.setMinLines(2);
        root.addView(testText, matchWidth());
        LinearLayout buttons = row();
        smallButton(buttons, "Speak", new Runnable() { public void run() { speakDirectly(); } });
        smallButton(buttons, "Speak Through Android", new Runnable() { public void run() { speakThroughSystem(); } });
        root.addView(buttons, matchWidth());
        LinearLayout buttons2 = row();
        smallButton(buttons2, "Stop", new Runnable() { public void run() { stopAll(); } });
        smallButton(buttons2, "Save as WAV", new Runnable() { public void run() { chooseWavFile(); } });
        root.addView(buttons2, matchWidth());
        status = new TextView(this);
        if (Build.VERSION.SDK_INT >= 19) status.setAccessibilityLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);
        root.addView(status, matchWidth());
    }

    /** The current voice's own [:cv] settings, in their groups. */
    private void rebuildVoiceSettings() {
        voiceSettings.removeAllViews();
        final Voices.Voice voice = Voices.get(settings.voice());
        for (String group : Parameters.GROUPS) {
            heading(voiceSettings, group + ": " + voice.fullName);
            for (Parameters.Parameter p : Parameters.ALL) {
                if (p.group.equals(group)) parameterRow(voiceSettings, voice, p);
            }
        }
        Button reset = new Button(this);
        reset.setText("Reset " + voice.fullName + " to Its Own Settings");
        reset.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                settings.resetParameters(voice);
                rebuildVoiceSettings();
                announce(voice.fullName + " is back to its own settings");
            }
        });
        voiceSettings.addView(reset, matchWidth());
    }

    private void parameterRow(LinearLayout parent, final Voices.Voice voice, final Parameters.Parameter p) {
        final double own = p.ownValue(voice);
        Map<String, Double> custom = settings.parameters(voice);
        final Double changed = custom.get(p.code);
        String ownText = Double.isNaN(own) ? "automatic" : describe(p, own);

        TextView label = new TextView(this);
        label.setText(p.name + " (" + p.code + "), " + Parameters.shown(p.min) + " to " + Parameters.shown(p.max));
        parent.addView(label, matchWidth());
        LinearLayout r = row();
        parent.addView(r, matchWidth());

        if (p.code.equals("ge")) {
            choiceButton(r, p.name, new String[]{"Female", "Male"}, new Choice() {
                public int selected() {
                    Double now = settings.parameters(voice).get(p.code);
                    return (now != null ? now : own) >= 0.5 ? 1 : 0;
                }
                public void picked(int index) {
                    if (changed == null && index == own) return;
                    settings.setParameter(voice, p.code, (double) index);
                    rebuildVoiceSettingsLater();
                }
            });
        } else {
            final EditText field = new EditText(this);
            field.setInputType(InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_FLAG_DECIMAL);
            field.setImeOptions(EditorInfo.IME_ACTION_DONE);
            field.setSingleLine(true);
            double current = changed != null ? changed : own;
            if (!Double.isNaN(current)) field.setText(Parameters.shown(current));
            labelField(label, field, Double.isNaN(own) ? p.name + ", automatic" : p.name);
            Runnable apply = new Runnable() {
                public void run() {
                    String typed = field.getText().toString().trim();
                    if (typed.isEmpty()) {
                        if (changed != null) { settings.setParameter(voice, p.code, null); rebuildVoiceSettingsLater(); }
                        return;
                    }
                    double value;
                    try { value = Double.parseDouble(typed); } catch (NumberFormatException e) { return; }
                    double clamped = p.clamp(value);
                    if (clamped != value) announce(p.name + ", " + describe(p, clamped) + ", the " + (value < clamped ? "lowest" : "highest") + " allowed");
                    double current = changed != null ? changed : own;
                    if (clamped == current) return;
                    settings.setParameter(voice, p.code, clamped);
                    field.setText(Parameters.shown(clamped));
                    if (Parameters.partner(p.code) != null) rebuildVoiceSettingsLater();
                }
            };
            onCommit(field, apply);
            r.addView(field, weight(1));
            stepButton(r, "−", "Lower " + p.name, field, -p.step, p.min, p.max, apply, own);
            stepButton(r, "+", "Raise " + p.name, field, p.step, p.min, p.max, apply, own);
        }
        Button reset = new Button(this);
        reset.setText("Reset");
        reset.setContentDescription("Reset " + p.name);
        reset.setEnabled(changed != null);
        reset.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                settings.setParameter(voice, p.code, null);
                announce(p.name + ", " + voice.fullName + "'s own, " + (Double.isNaN(own) ? "automatic" : describe(p, own)));
                rebuildVoiceSettingsLater();
            }
        });
        r.addView(reset);
        note(parent, p.help + " " + voice.fullName + "'s own: " + ownText + "." + (changed != null ? " Changed." : ""));
    }

    private void rebuildVoiceSettingsLater() {
        main.post(new Runnable() { public void run() { rebuildVoiceSettings(); } });
    }

    private static String describe(Parameters.Parameter p, double value) {
        if (p.code.equals("ge")) return value >= 0.5 ? "male" : "female";
        return Parameters.shown(value) + (p.unit.isEmpty() ? "" : " " + p.unit);
    }

    private void flutterRow() {
        final TextView label = new TextView(this);
        root.addView(label, matchWidth());
        final SeekBar bar = new SeekBar(this);
        bar.setMax(Settings.MAX_FLUTTER / 5);
        bar.setProgress(settings.flutter() / 5);
        Runnable describe = new Runnable() {
            public void run() {
                int v = settings.flutter();
                String text = "Flutter: " + (v == 0 ? "off" : v + " percent");
                label.setText(text);
                bar.setContentDescription(text);
            }
        };
        describe.run();
        final Runnable describeFinal = describe;
        bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            public void onProgressChanged(SeekBar s, int progress, boolean fromUser) {
                settings.setFlutter(progress * 5);
                describeFinal.run();
            }
            public void onStartTrackingTouch(SeekBar s) {}
            public void onStopTrackingTouch(SeekBar s) {}
        });
        root.addView(bar, matchWidth());
        note("How much the pitch wavers, slowly and unevenly, as a shaky voice does. Off unless you turn it up.");
    }

    // ---- Dictionaries ----------------------------------------------------------------

    private void rebuildDictionaries() {
        dictionaryList.removeAllViews();
        List<Dictionaries.Info> list = Dictionaries.list(this, settings);
        if (list.isEmpty()) {
            note(dictionaryList, "No dictionaries yet.");
            return;
        }
        for (final Dictionaries.Info d : list) {
            LinearLayout r = row();
            CheckBox box = new CheckBox(this);
            box.setText(d.file + ": " + d.added + " entries" + (d.rejected > 0 ? ", " + d.rejected + " lines with mistakes left out (the first at line " + d.firstRejectedLine + ")" : ""));
            box.setChecked(d.enabled);
            box.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
                public void onCheckedChanged(CompoundButton b, boolean on) {
                    Dictionaries.setEnabled(settings, d.file, on);
                }
            });
            r.addView(box, weight(1));
            Button remove = new Button(this);
            remove.setText("Remove");
            remove.setContentDescription("Remove " + d.file);
            remove.setOnClickListener(new View.OnClickListener() {
                public void onClick(View v) {
                    Dictionaries.remove(SettingsActivity.this, settings, d.file);
                    announce("Removed " + d.file);
                    rebuildDictionaries();
                }
            });
            r.addView(remove);
            dictionaryList.addView(r, matchWidth());
        }
    }

    private void chooseDictionary() {
        Intent i = new Intent(Build.VERSION.SDK_INT >= 19 ? Intent.ACTION_OPEN_DOCUMENT : Intent.ACTION_GET_CONTENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        i.setType("*/*");
        try {
            startActivityForResult(Intent.createChooser(i, "Choose a dictionary file"), REQUEST_DICTIONARY);
        } catch (RuntimeException e) {
            setStatus("This device has no way to choose a file.");
        }
    }

    // ---- Trying it ---------------------------------------------------------------------

    private void speakDirectly() {
        stopAll();
        final String text = testText.getText().toString();
        final Voices.Voice voice = Voices.get(settings.voice());
        final int rate = synthesizer.sampleRate();
        stopDirect = false;
        directThread = new Thread(new Runnable() {
            public void run() {
                int min = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT);
                AudioTrack t = new AudioTrack(AudioManager.STREAM_MUSIC, rate, AudioFormat.CHANNEL_OUT_MONO,
                        AudioFormat.ENCODING_PCM_16BIT, Math.max(min, 8192), AudioTrack.MODE_STREAM);
                synchronized (SettingsActivity.this) { track = t; }
                t.play();
                final AudioTrack out = t;
                synthesizer.speak(text, voice, 1.0, 1.0, 4096, new KlattaraNative.AudioSink() {
                    public int onAudio(byte[] pcm, int length) {
                        if (stopDirect) return 1;
                        out.write(pcm, 0, length);
                        return stopDirect ? 1 : 0;
                    }
                });
                if (!stopDirect) t.stop();
            }
        }, "Klattara test");
        directThread.start();
        setStatus("Speaking with " + voice.fullName + " at " + rate + " hertz.");
    }

    private void speakThroughSystem() {
        stopAll();
        final String text = testText.getText().toString();
        if (systemTts == null) {
            systemTts = new TextToSpeech(this, new TextToSpeech.OnInitListener() {
                public void onInit(int s) {
                    if (s == TextToSpeech.SUCCESS) speakSystem(text);
                    else setStatus("Android could not start Klattara's speech service.");
                }
            }, getPackageName());
        } else {
            speakSystem(text);
        }
    }

    @SuppressWarnings("deprecation")
    private void speakSystem(String text) {
        systemTts.speak(text, TextToSpeech.QUEUE_FLUSH, null);
        setStatus("Speaking through Android's speech service.");
    }

    private void stopAll() {
        stopDirect = true;
        synchronized (this) {
            if (track != null) {
                try { track.pause(); track.flush(); track.release(); } catch (RuntimeException ignored) {}
                track = null;
            }
        }
        if (systemTts != null) systemTts.stop();
    }

    private void chooseWavFile() {
        Voices.Voice voice = Voices.get(settings.voice());
        String name = "Klattara " + voice.fullName + " " + synthesizer.sampleRate() + " Hz.wav";
        if (Build.VERSION.SDK_INT >= 19) {
            Intent i = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType("audio/x-wav");
            i.putExtra(Intent.EXTRA_TITLE, name);
            try {
                startActivityForResult(i, REQUEST_WAV);
                return;
            } catch (RuntimeException ignored) {
            }
        }
        // Android 4.0 to 4.3: into the Music folder.
        File folder = new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MUSIC), "Klattara");
        folder.mkdirs();
        File file = new File(folder, name);
        try {
            saveWav(new FileOutputStream(file), file.getPath());
        } catch (IOException e) {
            setStatus("Could not save: " + e.getMessage());
        }
    }

    private void saveWav(final OutputStream out, final String where) {
        final String text = testText.getText().toString();
        final Voices.Voice voice = Voices.get(settings.voice());
        setStatus("Saving…");
        new Thread(new Runnable() {
            public void run() {
                String message;
                try {
                    double seconds = Wav.write(synthesizer, text, voice, out);
                    message = String.format(java.util.Locale.US, "Saved %s: %.1f seconds, %d hertz, 16-bit mono.",
                            where, seconds, synthesizer.sampleRate());
                } catch (IOException e) {
                    message = "Could not save: " + e.getMessage();
                }
                final String m = message;
                main.post(new Runnable() { public void run() { setStatus(m); } });
            }
        }, "Klattara WAV").start();
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (result != RESULT_OK || data == null || data.getData() == null) return;
        Uri uri = data.getData();
        if (request == REQUEST_DICTIONARY) {
            try {
                Dictionaries.Info d = Dictionaries.add(this, settings, uri);
                rebuildDictionaries();
                setStatus("Added " + d.file + ": " + d.added + " entries"
                        + (d.rejected > 0 ? ", " + d.rejected + " lines with mistakes left out, the first at line " + d.firstRejectedLine : "") + ".");
            } catch (IOException e) {
                setStatus("Could not add the dictionary: " + e.getMessage());
            }
        } else if (request == REQUEST_WAV) {
            try {
                OutputStream out = getContentResolver().openOutputStream(uri);
                if (out == null) throw new IOException("could not open the file");
                saveWav(out, "the file");
            } catch (IOException e) {
                setStatus("Could not save: " + e.getMessage());
            }
        }
    }

    // ---- Building blocks ---------------------------------------------------------------

    interface IntValue {
        int get();
        void set(int v);
    }

    interface Toggled {
        void toggled(boolean on);
    }

    private void heading(String text) {
        heading(root, text);
    }

    @TargetApi(28)
    private void heading(LinearLayout parent, String text) {
        TextView t = new TextView(this);
        t.setText(text);
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 20);
        t.setPadding(0, dp(18), 0, dp(6));
        if (Build.VERSION.SDK_INT >= 28) t.setAccessibilityHeading(true);
        parent.addView(t, matchWidth());
    }

    private void note(String text) {
        note(root, text);
    }

    private void note(LinearLayout parent, String text) {
        TextView t = new TextView(this);
        t.setText(text);
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        t.setPadding(0, 0, 0, dp(8));
        parent.addView(t, matchWidth());
    }

    private void checkBox(String text, boolean on, final Toggled toggled) {
        CheckBox box = new CheckBox(this);
        box.setText(text);
        box.setChecked(on);
        box.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
            public void onCheckedChanged(CompoundButton b, boolean isOn) { toggled.toggled(isOn); }
        });
        root.addView(box, matchWidth());
    }

    interface Choice {
        int selected();
        void picked(int index);
    }

    /**
     * A choice as a button saying what is chosen ("Voice profile: Standard Steve"),
     * which opens a dialog listing every option. Drop-down lists are awkward or
     * unreachable with some notetakers' screen readers and on older Android; a
     * button and a plain list work with all of them.
     */
    private void choiceButton(LinearLayout parent, final String title, final String[] options, final Choice choice) {
        final Button b = new Button(this);
        final Runnable describe = new Runnable() {
            public void run() { b.setText(title + ": " + options[Math.max(0, Math.min(choice.selected(), options.length - 1))]); }
        };
        describe.run();
        b.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                new AlertDialog.Builder(SettingsActivity.this)
                        .setTitle(title)
                        .setSingleChoiceItems(options, choice.selected(), new DialogInterface.OnClickListener() {
                            public void onClick(DialogInterface dialog, int which) {
                                choice.picked(which);
                                describe.run();
                                dialog.dismiss();
                                announce(title + ": " + options[which]);
                            }
                        })
                        .setNegativeButton(android.R.string.cancel, null)
                        .show();
            }
        });
        parent.addView(b, parent == root ? matchWidth() : weight(1));
    }

    private void numberRow(final String label, final int min, final int max, int step, final String unit, final IntValue value) {
        TextView l = new TextView(this);
        l.setText(label + " (" + min + " to " + max + ")");
        root.addView(l, matchWidth());
        LinearLayout r = row();
        final EditText field = new EditText(this);
        field.setInputType(InputType.TYPE_CLASS_NUMBER);
        field.setImeOptions(EditorInfo.IME_ACTION_DONE);
        field.setSingleLine(true);
        field.setText(String.valueOf(value.get()));
        labelField(l, field, label);
        final Runnable apply = new Runnable() {
            public void run() {
                String typed = field.getText().toString().trim();
                int v;
                try { v = Integer.parseInt(typed); } catch (NumberFormatException e) { field.setText(String.valueOf(value.get())); return; }
                int clamped = Math.max(min, Math.min(max, v));
                value.set(clamped);
                field.setText(String.valueOf(clamped));
                if (clamped != v) announce(clamped + " " + unit + ", the " + (v < clamped ? "lowest" : "highest") + " allowed");
            }
        };
        onCommit(field, apply);
        r.addView(field, weight(1));
        stepButton(r, "−", "Lower " + label, field, -step, min, max, apply, Double.NaN);
        stepButton(r, "+", "Raise " + label, field, step, min, max, apply, Double.NaN);
        root.addView(r, matchWidth());
    }

    private void stepButton(LinearLayout r, String text, String description, final EditText field, final double step,
                            final double min, final double max, final Runnable apply, final double fallback) {
        Button b = new Button(this);
        b.setText(text);
        b.setContentDescription(description);
        b.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                double current;
                try { current = Double.parseDouble(field.getText().toString().trim()); }
                catch (NumberFormatException e) { current = Double.isNaN(fallback) ? (min + max) / 2 : fallback; }
                double next = Math.max(min, Math.min(max, Math.rint(current + step)));
                field.setText(Parameters.shown(next));
                apply.run();
                announce(Parameters.shown(next));
            }
        });
        r.addView(b);
    }

    private void onCommit(final EditText field, final Runnable apply) {
        field.setOnEditorActionListener(new TextView.OnEditorActionListener() {
            public boolean onEditorAction(TextView v, int action, android.view.KeyEvent e) {
                apply.run();
                return false;
            }
        });
        field.setOnFocusChangeListener(new View.OnFocusChangeListener() {
            public void onFocusChange(View v, boolean has) { if (!has) apply.run(); }
        });
    }

    private void button(String text, final Runnable action) {
        Button b = new Button(this);
        b.setText(text);
        b.setOnClickListener(new View.OnClickListener() { public void onClick(View v) { action.run(); } });
        root.addView(b, matchWidth());
    }

    private void smallButton(LinearLayout r, String text, final Runnable action) {
        Button b = new Button(this);
        b.setText(text);
        b.setOnClickListener(new View.OnClickListener() { public void onClick(View v) { action.run(); } });
        r.addView(b, weight(1));
    }

    private LinearLayout row() {
        LinearLayout r = new LinearLayout(this);
        r.setOrientation(LinearLayout.HORIZONTAL);
        r.setGravity(Gravity.CENTER_VERTICAL);
        return r;
    }

    /**
     * Ties a label to an edit field, so a screen reader says the field's value and
     * then what it is, rather than a description in place of the value. Android 4.2
     * and newer link the label itself; before that, the hint carries it.
     */
    private void labelField(TextView label, EditText field, String hint) {
        if (Build.VERSION.SDK_INT >= 17) {
            label.setLabelFor(assignId(field));
            if (hint.endsWith("automatic")) field.setHint("Automatic");
        } else {
            field.setHint(hint);
        }
    }

    private static int nextId = 0x4b000000;

    private static int assignId(View v) {
        v.setId(++nextId);
        return nextId;
    }

    private LinearLayout.LayoutParams matchWidth() {
        return new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
    }

    private LinearLayout.LayoutParams weight(float w) {
        return new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, w);
    }

    private int dp(int v) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, getResources().getDisplayMetrics());
    }

    private void announce(String text) {
        if (Build.VERSION.SDK_INT >= 16) root.announceForAccessibility(text);
    }

    private void setStatus(String text) {
        status.setText(text);
        announce(text);
    }

    private String versionName() {
        try {
            return getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
        } catch (Exception e) {
            return "";
        }
    }
}
