/* Klattara for Android: functions the Android library adds to klattara.h.
 * Implemented in src/plat/android/rust. */
#ifndef KLATTARA_ANDROID_H
#define KLATTARA_ANDROID_H

typedef struct KlattaraDictionary KlattaraDictionary;

/* A voice's own value of a [:cv] setting such as "hs", in the command's units;
 * NaN when the voice leaves it to the engine, or for an unknown voice or code. */
double klattara_android_voice_setting(int index, const char *code);

/* Pronunciation dictionaries: see src/plat/shared/dictionary.rs for the format. */
KlattaraDictionary *klattara_dictionary_new(void);
void klattara_dictionary_free(KlattaraDictionary *dictionary);
/* results[0] entries taken in, [1] lines left out, [2] first such line (0 if none). */
int klattara_dictionary_load(KlattaraDictionary *dictionary, const char *text, int *results);
int klattara_dictionary_size(const KlattaraDictionary *dictionary);
/* Writes text with the dictionary applied (brackets always made safe); returns
 * the size needed including the NUL. A NULL dictionary only makes it safe. */
int klattara_dictionary_apply(const KlattaraDictionary *dictionary, const char *text, char *buffer, int size);

#endif
