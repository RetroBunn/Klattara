/* The JNI bridge between Klattara's Java side (com.dgomeze42.klattara.KlattaraNative)
 * and the engine (klattara.h, klattara_android.h). Built into libklattara.so with the
 * Rust static library by src/plat/android/build-native.sh.
 *
 * Text crosses as UTF-8 byte arrays rather than Java strings: JNI's string functions
 * use "modified UTF-8", which mangles emoji and other characters past U+FFFF.
 *
 * Audio is gathered here into chunks of the size the caller asks for (the TTS
 * service passes what Android's synthesis callback takes at once), so Java is called
 * once a chunk rather than for every few samples, and each call can stop speech. */

#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../../windows/klattara.h"
#include "klattara_android.h"

#define JNI_FN(name) Java_com_dgomeze42_klattara_KlattaraNative_##name

/* ---- Android 4.x on 32-bit ARM ------------------------------------------------
 * Rust's standard library and the unwinder refer to dl_iterate_phdr, which 32-bit
 * ARM Android only has from 5.0 (API 21): without it, the library would not load on
 * 4.x at all. This stand-in hands over to the real one when the system has it.
 * Without it, it reports this library alone, from its own program headers in
 * memory, found with dladdr (which every Android has). That is all the unwinder
 * needs, since Rust's code only ever unwinds through its own frames, so an internal
 * error is still caught and returned as an error code rather than ending the app.
 * Hidden, so it serves this library alone. */
#if defined(__arm__)
#include <dlfcn.h>
#include <elf.h>
#include <link.h>

typedef int (*dl_iterate_phdr_fn)(int (*)(struct dl_phdr_info *, size_t, void *), void *);

__attribute__((visibility("hidden")))
int dl_iterate_phdr(int (*callback)(struct dl_phdr_info *, size_t, void *), void *data) {
    static dl_iterate_phdr_fn real;
    static int looked;
    if (!looked) {
        void *libdl = dlopen("libdl.so", RTLD_NOW);
        if (libdl) real = (dl_iterate_phdr_fn)dlsym(libdl, "dl_iterate_phdr");
        looked = 1;
    }
    if (real && real != dl_iterate_phdr) return real(callback, data);

    Dl_info self;
    if (!dladdr((void *)&dl_iterate_phdr, &self) || !self.dli_fbase) return 0;
    const Elf32_Ehdr *header = (const Elf32_Ehdr *)self.dli_fbase;
    if (memcmp(header->e_ident, ELFMAG, SELFMAG) != 0) return 0;
    const Elf32_Phdr *phdr = (const Elf32_Phdr *)((const char *)header + header->e_phoff);
    /* The load bias: where the library is, less where its first segment asked to be. */
    Elf32_Addr lowest = (Elf32_Addr)-1;
    for (int i = 0; i < header->e_phnum; i++)
        if (phdr[i].p_type == PT_LOAD && phdr[i].p_vaddr < lowest) lowest = phdr[i].p_vaddr;
    if (lowest == (Elf32_Addr)-1) return 0;
    struct dl_phdr_info info;
    memset(&info, 0, sizeof info);
    info.dlpi_addr = (Elf32_Addr)(uintptr_t)self.dli_fbase - lowest;
    info.dlpi_name = self.dli_fname;
    info.dlpi_phdr = phdr;
    info.dlpi_phnum = header->e_phnum;
    return callback(&info, sizeof info, data);
}
#endif

/* ---- Helpers --------------------------------------------------------------- */

/* A byte array as a NUL-terminated string the caller frees; NULL if out of memory. */
static char *utf8_of(JNIEnv *env, jbyteArray bytes) {
    if (!bytes) return NULL;
    jsize n = (*env)->GetArrayLength(env, bytes);
    char *s = malloc((size_t)n + 1);
    if (!s) return NULL;
    (*env)->GetByteArrayRegion(env, bytes, 0, n, (jbyte *)s);
    s[n] = 0;
    return s;
}

static jstring string_of(JNIEnv *env, const char *s) {
    return s ? (*env)->NewStringUTF(env, s) : NULL;
}

/* ---- Audio ------------------------------------------------------------------ */

typedef struct {
    JNIEnv *env;
    jobject sink;
    jmethodID on_audio;   /* int onAudio(byte[] pcm, int length) */
    jbyteArray array;
    uint8_t *pending;     /* little-endian 16-bit PCM waiting to go */
    int pending_bytes;
    int chunk_bytes;
    int stopped;
} AudioContext;

/* Hands what is pending to Java. Returns nonzero if Java asked to stop. */
static int flush(AudioContext *c) {
    if (c->pending_bytes == 0 || c->stopped) return c->stopped;
    JNIEnv *env = c->env;
    (*env)->SetByteArrayRegion(env, c->array, 0, c->pending_bytes, (const jbyte *)c->pending);
    jint result = (*env)->CallIntMethod(env, c->sink, c->on_audio, c->array, c->pending_bytes);
    c->pending_bytes = 0;
    if ((*env)->ExceptionCheck(env) || result != 0) c->stopped = 1;
    return c->stopped;
}

static int on_audio(void *user, const int16_t *samples, int count) {
    AudioContext *c = user;
    for (int i = 0; i < count; i++) {
        if (c->pending_bytes + 2 > c->chunk_bytes && flush(c)) return 1;
        uint16_t s = (uint16_t)samples[i];
        c->pending[c->pending_bytes++] = (uint8_t)(s & 0xff);
        c->pending[c->pending_bytes++] = (uint8_t)(s >> 8);
    }
    return c->stopped;
}

static int discard_audio(void *user, const int16_t *samples, int count) {
    (void)user; (void)samples; (void)count;
    return 0;
}

/* ---- Engine ------------------------------------------------------------------- */

JNIEXPORT jlong JNICALL JNI_FN(create)(JNIEnv *env, jclass cls, jint sample_rate) {
    (void)env; (void)cls;
    return (jlong)(intptr_t)klattara_create(sample_rate);
}

JNIEXPORT void JNICALL JNI_FN(destroy)(JNIEnv *env, jclass cls, jlong engine) {
    (void)env; (void)cls;
    klattara_destroy((KlattaraEngine *)(intptr_t)engine);
}

JNIEXPORT jstring JNICALL JNI_FN(version)(JNIEnv *env, jclass cls) {
    (void)cls;
    return string_of(env, klattara_version());
}

JNIEXPORT jint JNICALL JNI_FN(voiceCount)(JNIEnv *env, jclass cls) {
    (void)env; (void)cls;
    return klattara_voice_count();
}

JNIEXPORT jstring JNICALL JNI_FN(voiceName)(JNIEnv *env, jclass cls, jint index) {
    (void)cls;
    return string_of(env, klattara_voice_name(index));
}

JNIEXPORT jstring JNICALL JNI_FN(voiceFullName)(JNIEnv *env, jclass cls, jint index) {
    (void)cls;
    return string_of(env, klattara_voice_full_name(index));
}

JNIEXPORT jdouble JNICALL JNI_FN(voicePitch)(JNIEnv *env, jclass cls, jint index) {
    (void)env; (void)cls;
    return klattara_voice_pitch(index);
}

JNIEXPORT jint JNICALL JNI_FN(voiceGender)(JNIEnv *env, jclass cls, jint index) {
    (void)env; (void)cls;
    return klattara_voice_gender(index);
}

JNIEXPORT jdouble JNICALL JNI_FN(voiceSetting)(JNIEnv *env, jclass cls, jint index, jbyteArray code) {
    (void)cls;
    char *c = utf8_of(env, code);
    double value = c ? klattara_android_voice_setting(index, c) : 0.0 / 0.0;
    free(c);
    return value;
}

#define SETTER(name, call, type)                                                        \
    JNIEXPORT jint JNICALL JNI_FN(name)(JNIEnv *env, jclass cls, jlong engine, type v) { \
        (void)env; (void)cls;                                                            \
        return call((KlattaraEngine *)(intptr_t)engine, v);                              \
    }

SETTER(setVoice, klattara_set_voice, jint)
SETTER(setPitch, klattara_set_pitch, jdouble)
SETTER(setInflection, klattara_set_inflection, jdouble)
SETTER(setVolume, klattara_set_volume, jdouble)
SETTER(setRate, klattara_set_rate, jdouble)

JNIEXPORT jint JNICALL JNI_FN(setAbbreviations)(JNIEnv *env, jclass cls, jlong engine, jboolean expand) {
    (void)env; (void)cls;
    return klattara_set_abbreviations((KlattaraEngine *)(intptr_t)engine, expand ? 1 : 0);
}

JNIEXPORT jint JNICALL JNI_FN(setPhrasePrediction)(JNIEnv *env, jclass cls, jlong engine, jboolean predict) {
    (void)env; (void)cls;
    return klattara_set_phrase_prediction((KlattaraEngine *)(intptr_t)engine, predict ? 1 : 0);
}

/* Speaks commands only (they make no sound), such as "[:cv hs 90]". */
JNIEXPORT jint JNICALL JNI_FN(command)(JNIEnv *env, jclass cls, jlong engine, jbyteArray utf8) {
    (void)cls;
    char *text = utf8_of(env, utf8);
    if (!text) return KLATTARA_INVALID;
    int result = klattara_speak((KlattaraEngine *)(intptr_t)engine, text, KLATTARA_READ_TEXT, discard_audio, NULL);
    free(text);
    return result;
}

/* Speaks text, handing 16-bit little-endian PCM to sink.onAudio(byte[], int) in
 * chunks of at most chunkBytes. Returns KLATTARA_OK, KLATTARA_STOPPED, or an error. */
JNIEXPORT jint JNICALL JNI_FN(speak)(JNIEnv *env, jclass cls, jlong engine, jbyteArray utf8,
                                     jboolean spelled, jint chunk_bytes, jobject sink) {
    (void)cls;
    if (!sink || chunk_bytes < 2) return KLATTARA_INVALID;
    char *text = utf8_of(env, utf8);
    if (!text) return KLATTARA_INVALID;
    AudioContext c;
    memset(&c, 0, sizeof c);
    c.env = env;
    c.sink = sink;
    c.chunk_bytes = chunk_bytes & ~1;
    jclass sink_class = (*env)->GetObjectClass(env, sink);
    c.on_audio = (*env)->GetMethodID(env, sink_class, "onAudio", "([BI)I");
    (*env)->DeleteLocalRef(env, sink_class);
    c.array = (*env)->NewByteArray(env, c.chunk_bytes);
    c.pending = malloc((size_t)c.chunk_bytes);
    int result = KLATTARA_INVALID;
    if (c.on_audio && c.array && c.pending) {
        result = klattara_speak((KlattaraEngine *)(intptr_t)engine, text,
                                spelled ? KLATTARA_READ_SPELLED : KLATTARA_READ_TEXT, on_audio, &c);
        if (result >= 0 && !c.stopped && flush(&c)) result = KLATTARA_STOPPED;
        if (c.stopped) result = KLATTARA_STOPPED;
    }
    free(c.pending);
    if (c.array) (*env)->DeleteLocalRef(env, c.array);
    free(text);
    return result;
}

/* ---- Dictionaries --------------------------------------------------------------- */

JNIEXPORT jlong JNICALL JNI_FN(dictionaryNew)(JNIEnv *env, jclass cls) {
    (void)env; (void)cls;
    return (jlong)(intptr_t)klattara_dictionary_new();
}

JNIEXPORT void JNICALL JNI_FN(dictionaryFree)(JNIEnv *env, jclass cls, jlong dictionary) {
    (void)env; (void)cls;
    klattara_dictionary_free((KlattaraDictionary *)(intptr_t)dictionary);
}

/* Returns {taken in, left out, first left-out line}, or null on failure. */
JNIEXPORT jintArray JNICALL JNI_FN(dictionaryLoad)(JNIEnv *env, jclass cls, jlong dictionary, jbyteArray utf8) {
    (void)cls;
    char *text = utf8_of(env, utf8);
    if (!text) return NULL;
    int results[3] = {0, 0, 0};
    int ok = klattara_dictionary_load((KlattaraDictionary *)(intptr_t)dictionary, text, results);
    free(text);
    if (ok != 0) return NULL;
    jintArray out = (*env)->NewIntArray(env, 3);
    if (out) (*env)->SetIntArrayRegion(env, out, 0, 3, (const jint *)results);
    return out;
}

JNIEXPORT jint JNICALL JNI_FN(dictionarySize)(JNIEnv *env, jclass cls, jlong dictionary) {
    (void)env; (void)cls;
    return klattara_dictionary_size((const KlattaraDictionary *)(intptr_t)dictionary);
}

/* The text with the dictionary applied and brackets made safe, as UTF-8 bytes.
 * A zero dictionary only makes it safe. */
JNIEXPORT jbyteArray JNICALL JNI_FN(dictionaryApply)(JNIEnv *env, jclass cls, jlong dictionary, jbyteArray utf8) {
    (void)cls;
    char *text = utf8_of(env, utf8);
    if (!text) return NULL;
    const KlattaraDictionary *d = (const KlattaraDictionary *)(intptr_t)dictionary;
    int needed = klattara_dictionary_apply(d, text, NULL, 0);
    jbyteArray out = NULL;
    if (needed > 0) {
        char *buffer = malloc((size_t)needed);
        if (buffer && klattara_dictionary_apply(d, text, buffer, needed) == needed) {
            out = (*env)->NewByteArray(env, needed - 1);
            if (out) (*env)->SetByteArrayRegion(env, out, 0, needed - 1, (const jbyte *)buffer);
        }
        free(buffer);
    }
    free(text);
    return out;
}
