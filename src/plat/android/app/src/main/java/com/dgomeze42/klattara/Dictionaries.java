package com.dgomeze42.klattara;

import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.List;

/**
 * Pronunciation dictionaries people make themselves: text files, one entry a line,
 * either a word and its sounds in ARPAbet ("aback AH0 B AE1 K", the format of
 * Klattara's extra_words.dict and CMUdict) or a word, "=", and how to say it
 * ("nvda = en vee dee ay"). They are copied into the app when added, so the
 * original can go, and every sound is checked as they load.
 */
final class Dictionaries {
    /** What a file holds, once checked. */
    static final class Info {
        final String file;
        final boolean enabled;
        final int added;
        final int rejected;
        final int firstRejectedLine;

        Info(String file, boolean enabled, int[] counts) {
            this.file = file;
            this.enabled = enabled;
            this.added = counts == null ? 0 : counts[0];
            this.rejected = counts == null ? 0 : counts[1];
            this.firstRejectedLine = counts == null ? 0 : counts[2];
        }
    }

    private Dictionaries() {}

    static File folder(Context context) {
        File f = new File(context.getFilesDir(), "dictionaries");
        if (!f.isDirectory()) f.mkdirs();
        return f;
    }

    /** Where Android 2.2 to 3.2's engine plugin, in another process, can read them. */
    private static final String OLD_ANDROID_PREFIX = "dictionary-";

    private static boolean oldAndroid() {
        return android.os.Build.VERSION.SDK_INT < 14;
    }

    /**
     * Where a dictionary is kept. On Android 2.2 to 3.2 that is the files folder
     * itself, written readable by Android's speech service (which runs the plugin
     * and cannot change a file's permissions after), since a folder of its own
     * would not be readable to it.
     */
    static File file(Context context, String name) {
        return oldAndroid() ? new File(context.getFilesDir(), OLD_ANDROID_PREFIX + name) : new File(folder(context), name);
    }

    @SuppressWarnings("deprecation")
    private static void write(Context context, String name, byte[] text) throws IOException {
        FileOutputStream out = oldAndroid()
                ? context.openFileOutput(OLD_ANDROID_PREFIX + name, Context.MODE_WORLD_READABLE)
                : new FileOutputStream(file(context, name));
        try {
            out.write(text);
        } finally {
            out.close();
        }
    }

    static byte[] read(File file) throws IOException {
        InputStream in = new FileInputStream(file);
        try {
            return readAll(in);
        } finally {
            in.close();
        }
    }

    private static byte[] readAll(InputStream in) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buffer = new byte[16384];
        int n;
        while ((n = in.read(buffer)) > 0) out.write(buffer, 0, n);
        return out.toByteArray();
    }

    /** Counts what a file's text holds, by loading it into a dictionary of its own. */
    static int[] check(byte[] text) {
        long d = KlattaraNative.dictionaryNew();
        try {
            return KlattaraNative.dictionaryLoad(d, text);
        } finally {
            KlattaraNative.dictionaryFree(d);
        }
    }

    /** Every dictionary added, in load order, with what each holds. */
    static List<Info> list(Context context, Settings settings) {
        List<Info> out = new ArrayList<Info>();
        for (Settings.DictionaryEntry e : settings.dictionaries()) {
            int[] counts = null;
            try {
                counts = check(read(file(context, e.file)));
            } catch (IOException ignored) {
            }
            out.add(new Info(e.file, e.enabled, counts));
        }
        return out;
    }

    /** Copies a file the person chose into the app and adds it, last. Returns its counts. */
    static Info add(Context context, Settings settings, Uri uri) throws IOException {
        String name = displayName(context, uri);
        InputStream in = context.getContentResolver().openInputStream(uri);
        if (in == null) throw new IOException("Could not open " + name);
        byte[] text;
        try {
            text = readAll(in);
        } finally {
            in.close();
        }
        int[] counts = check(text);
        if (counts == null || counts[0] == 0) {
            throw new IOException(name + " has no entries Klattara can use"
                    + (counts != null && counts[1] > 0 ? " (" + counts[1] + " lines with mistakes, the first at line " + counts[2] + ")" : "") + ".");
        }
        String file = unique(context, name);
        write(context, file, text);
        List<Settings.DictionaryEntry> list = settings.dictionaries();
        list.add(new Settings.DictionaryEntry(file, true));
        settings.setDictionaries(list);
        return new Info(file, true, counts);
    }

    static void setEnabled(Settings settings, String file, boolean enabled) {
        List<Settings.DictionaryEntry> list = settings.dictionaries();
        for (int i = 0; i < list.size(); i++) {
            if (list.get(i).file.equals(file)) list.set(i, new Settings.DictionaryEntry(file, enabled));
        }
        settings.setDictionaries(list);
    }

    static void remove(Context context, Settings settings, String file) {
        List<Settings.DictionaryEntry> list = settings.dictionaries();
        List<Settings.DictionaryEntry> kept = new ArrayList<Settings.DictionaryEntry>();
        for (Settings.DictionaryEntry e : list) if (!e.file.equals(file)) kept.add(e);
        settings.setDictionaries(kept);
        file(context, file).delete();
    }

    /** A native dictionary holding every enabled file, in order; 0 if there are none. */
    static long build(Context context, Settings settings) {
        long d = 0;
        for (Settings.DictionaryEntry e : settings.dictionaries()) {
            if (!e.enabled) continue;
            try {
                byte[] text = read(file(context, e.file));
                if (d == 0) d = KlattaraNative.dictionaryNew();
                KlattaraNative.dictionaryLoad(d, text);
            } catch (IOException ignored) {
            }
        }
        if (d != 0 && KlattaraNative.dictionarySize(d) == 0) {
            KlattaraNative.dictionaryFree(d);
            d = 0;
        }
        return d;
    }

    private static String displayName(Context context, Uri uri) {
        String name = null;
        Cursor c = null;
        try {
            c = context.getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null);
            if (c != null && c.moveToFirst()) name = c.getString(0);
        } catch (RuntimeException ignored) {
        } finally {
            if (c != null) c.close();
        }
        if (name == null) name = uri.getLastPathSegment();
        if (name == null || name.trim().length() == 0) name = "dictionary.dict";
        return name.replaceAll("[/\\\\:*?\"<>|]", "_");
    }

    private static String unique(Context context, String name) {
        if (!file(context, name).exists()) return name;
        int dot = name.lastIndexOf('.');
        String base = dot > 0 ? name.substring(0, dot) : name, ext = dot > 0 ? name.substring(dot) : "";
        for (int i = 2; ; i++) {
            String candidate = base + " " + i + ext;
            if (!file(context, candidate).exists()) return candidate;
        }
    }
}
