/* Klattara: a Klatt-style formant speech synthesizer for American English.
 *
 * C interface to klattara.dll. Every function catches internal errors and
 * returns an error code rather than crashing the host program. An engine may
 * be used from any thread, but from one thread at a time.
 *
 * Typical use:
 *
 *     KlattaraEngine *engine = klattara_create(22050);
 *     klattara_set_rate(engine, 2.0);
 *     klattara_speak(engine, "Hello, world.", KLATTARA_READ_TEXT, on_audio, my_data);
 *     klattara_destroy(engine);
 *
 * where on_audio receives signed 16-bit mono samples at the engine's rate,
 * and returns 0 to go on or nonzero to stop speaking at once.
 */

#ifndef KLATTARA_H
#define KLATTARA_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define KLATTARA_OK 0             /* Success. */
#define KLATTARA_STOPPED 1        /* The audio callback stopped speech early. */
#define KLATTARA_INVALID (-1)     /* Null pointer, value out of range, or bad phonetic input. */
#define KLATTARA_INTERNAL_ERROR (-2)

#define KLATTARA_READ_TEXT 0      /* Ordinary text. */
#define KLATTARA_READ_SPELLED 1   /* Character by character. */
#define KLATTARA_READ_PHONEMES 2  /* Bracketed ARPAbet, such as "[HH AH0 L OW1]". */

typedef struct KlattaraEngine KlattaraEngine;

typedef int (*KlattaraAudioCallback)(void *user, const int16_t *samples, int count);

/* The library version, such as "1.0.0". */
const char *klattara_version(void);

/* Creates an engine at sample_rate Hz (8000 to 96000; 22050 recommended).
 * Returns NULL if the rate is out of range. */
KlattaraEngine *klattara_create(int sample_rate);

/* Frees an engine. NULL is ignored. */
void klattara_destroy(KlattaraEngine *engine);

/* Speaking rate: 1.0 is about 200 words per minute, 2.0 twice as fast (0.25 to 8). */
int klattara_set_rate(KlattaraEngine *engine, double rate);

/* Base pitch in Hz (40 to 600; default 110). */
int klattara_set_pitch(KlattaraEngine *engine, double hz);

/* Pitch movement: 0 is monotone, 1 normal, up to 2.5. */
int klattara_set_inflection(KlattaraEngine *engine, double inflection);

/* Volume: 0 is silent, 1 normal, up to 2. */
int klattara_set_volume(KlattaraEngine *engine, double volume);

/* Abbreviations in text: nonzero (the default) reads them in full, "Dr." as
 * "doctor" and "Calif." as "California"; zero reads them as written, "D R". */
int klattara_set_abbreviations(KlattaraEngine *engine, int expand);

/* Phrase prediction: nonzero breaks long stretches of text without
 * punctuation into phrases, each with its own pitch contour and no pause;
 * zero (the default) reads each stretch as one phrase. */
int klattara_set_phrase_prediction(KlattaraEngine *engine, int predict);

/* Speaks UTF-8 input, passing audio to callback in chunks as it is made.
 * reading is one of the KLATTARA_READ_ constants. Returns KLATTARA_OK,
 * KLATTARA_STOPPED, or an error. */
int klattara_speak(KlattaraEngine *engine, const char *input, int reading,
                   KlattaraAudioCallback callback, void *user);

/* Writes how UTF-8 text will be pronounced, in bracketed ARPAbet, into
 * buffer if it fits in size bytes. Returns the size needed including the
 * terminating NUL, or an error. */
int klattara_transcribe(const char *text, char *buffer, int size);

#ifdef __cplusplus
}
#endif

#endif
