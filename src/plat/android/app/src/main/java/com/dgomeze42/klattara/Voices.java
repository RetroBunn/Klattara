package com.dgomeze42.klattara;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

/** Klattara's voices, as the engine defines them, the default first. */
final class Voices {
    static final class Voice {
        final int index;
        /** The engine's short name, such as "Steve". */
        final String name;
        /** Such as "Standard Steve". */
        final String fullName;
        final double pitch;
        final boolean female;

        Voice(int index, String name, String fullName, double pitch, boolean female) {
            this.index = index;
            this.name = name;
            this.fullName = fullName;
            this.pitch = pitch;
            this.female = female;
        }

        /** The name Android's voice lists use; it never changes, so a choice stays made. */
        String id() {
            return "klattara-" + name.toLowerCase(Locale.US);
        }

        /** The key its customizations are kept under. */
        String key() {
            return name.toLowerCase(Locale.US);
        }

        @Override
        public String toString() {
            return fullName;
        }
    }

    private static List<Voice> all;

    private Voices() {}

    static synchronized List<Voice> all() {
        if (all == null) {
            List<Voice> list = new ArrayList<Voice>();
            int n = KlattaraNative.voiceCount();
            for (int i = 0; i < n; i++) {
                list.add(new Voice(i, KlattaraNative.voiceName(i), KlattaraNative.voiceFullName(i),
                        KlattaraNative.voicePitch(i), KlattaraNative.voiceGender(i) == KlattaraNative.FEMALE));
            }
            all = Collections.unmodifiableList(list);
        }
        return all;
    }

    static Voice get(int index) {
        List<Voice> list = all();
        return index >= 0 && index < list.size() ? list.get(index) : list.get(0);
    }

    /** The voice with this id, or null. */
    static Voice byId(String id) {
        if (id == null) return null;
        for (Voice v : all()) if (v.id().equalsIgnoreCase(id)) return v;
        return null;
    }
}
