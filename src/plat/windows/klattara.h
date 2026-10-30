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

/* 32-bit Windows has several calling conventions. The DLL's functions and the
 * audio callback all use cdecl, whatever the program's default. */
#ifdef _WIN32
#define KLATTARA_CALL __cdecl
#else
#define KLATTARA_CALL
#endif

#define KLATTARA_OK 0             /* Success. */
#define KLATTARA_STOPPED 1        /* The audio callback stopped speech early. */
#define KLATTARA_INVALID (-1)     /* Null pointer, value out of range, or bad phonetic input. */
#define KLATTARA_INTERNAL_ERROR (-2)

#define KLATTARA_READ_TEXT 0      /* Ordinary text. */
#define KLATTARA_READ_SPELLED 1   /* Character by character. */
#define KLATTARA_READ_PHONEMES 2  /* Bracketed ARPAbet, such as "[HH AH0 L OW1]". */

#define KLATTARA_MALE 0           /* The male model of the vocal tract. */
#define KLATTARA_FEMALE 1         /* The female model of the vocal tract. */

typedef struct KlattaraEngine KlattaraEngine;

typedef int (KLATTARA_CALL *KlattaraAudioCallback)(void *user, const int16_t *samples, int count);

/* The library version, such as "1.0.0". */
const char *KLATTARA_CALL klattara_version(void);

/* Creates an engine at sample_rate Hz (8000 to 96000; 22050 recommended).
 * Returns NULL if the rate is out of range. */
KlattaraEngine *KLATTARA_CALL klattara_create(int sample_rate);

/* Frees an engine. NULL is ignored. */
void KLATTARA_CALL klattara_destroy(KlattaraEngine *engine);

/* The voices are numbered from 0; the first is the default. These give how
 * many there are, and each one's name as a constant string (NULL if there is no
 * such voice), base pitch in Hz (0 if none), inflection (-1 if none) and gender
 * (KLATTARA_INVALID if none). */
int KLATTARA_CALL klattara_voice_count(void);
const char *KLATTARA_CALL klattara_voice_name(int index);
double KLATTARA_CALL klattara_voice_pitch(int index);
double KLATTARA_CALL klattara_voice_inflection(int index);
int KLATTARA_CALL klattara_voice_gender(int index);

/* Changes to a voice: its gender, pitch, inflection and voice quality. Rate,
 * volume and how text is read stay as they are. */
int KLATTARA_CALL klattara_set_voice(KlattaraEngine *engine, int index);

/* Which model of the vocal tract the current voice uses, KLATTARA_MALE or
 * KLATTARA_FEMALE, for making a voice of your own. Nothing else changes, so set
 * the pitch as well. */
int KLATTARA_CALL klattara_set_gender(KlattaraEngine *engine, int gender);

/* Speaking rate: 1.0 is about 200 words per minute, 2.0 twice as fast (0.25 to 8). */
int KLATTARA_CALL klattara_set_rate(KlattaraEngine *engine, double rate);

/* Base pitch in Hz (40 to 550; each voice has its own default, 110 for the first). */
int KLATTARA_CALL klattara_set_pitch(KlattaraEngine *engine, double hz);

/* Pitch movement: 0 is monotone, 1 normal, up to 2.5. */
int KLATTARA_CALL klattara_set_inflection(KlattaraEngine *engine, double inflection);

/* Volume: 0 is silent, 1 normal, up to 2. */
int KLATTARA_CALL klattara_set_volume(KlattaraEngine *engine, double volume);

/* Abbreviations in text: nonzero (the default) reads them in full, "Dr." as
 * "doctor" and "Calif." as "California"; zero reads them as written, "D R". */
int KLATTARA_CALL klattara_set_abbreviations(KlattaraEngine *engine, int expand);

/* Phrase prediction: nonzero breaks long stretches of text without
 * punctuation into phrases, each with its own pitch contour and no pause;
 * zero (the default) reads each stretch as one phrase. */
int KLATTARA_CALL klattara_set_phrase_prediction(KlattaraEngine *engine, int predict);

/* Speaks UTF-8 input, passing audio to callback in chunks as it is made.
 * reading is one of the KLATTARA_READ_ constants. Returns KLATTARA_OK,
 * KLATTARA_STOPPED, or an error.
 *
 * In ordinary text, [:phone klattara enable] turns on phoneme input, and
 * [:phone klattara disable] turns it off, on this engine until it is changed.
 * While it is on, a bracket of valid ARPAbet is spoken as one word:
 * "Say [HH AH0 L OW1] now." A program that reads text it doesn't control should
 * remove "[:" from it first.
 *
 * Phonetic input can be sung. After any sound, <milliseconds,pitch> gives it a
 * length and a pitch: [D AA1<400,20>]. A pitch of 1 to 39 is a note, 1 to 37 being
 * C2 to C5 by semitones and 38 and 39 also C5, sung with vibrato. A pitch of 40 to
 * 550 is in Hz, with no vibrato. <milliseconds> or <,pitch> gives only one of them,
 * and _<milliseconds> is a rest. Written lengths don't change with the rate. */
int KLATTARA_CALL klattara_speak(KlattaraEngine *engine, const char *input, int reading,
                                 KlattaraAudioCallback callback, void *user);

/* Writes how UTF-8 text will be pronounced, in bracketed ARPAbet, into
 * buffer if it fits in size bytes. Returns the size needed including the
 * terminating NUL, or an error. */
int KLATTARA_CALL klattara_transcribe(const char *text, char *buffer, int size);

#ifdef __cplusplus
}
#endif

#endif
