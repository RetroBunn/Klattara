/* Klattara as a text-to-speech engine plugin for Android 2.2 to 3.2.
 *
 * Before Android 4.0 a third-party voice was a native library that Android's own
 * speech service (packages/TtsService) loads: it looks for an activity answering
 * android.intent.action.START_TTS_ENGINE, here com.dgomeze42.klattara.Klattara,
 * loads /data/data/<package>/lib/libtts<activity name, lower case>.so, here
 * libttsklattara.so, calls its getTtsEngine() and then drives the object that
 * returns through the C++ interface in frameworks/base/include/tts/TtsEngine.h,
 * identical in Android 2.2, 2.3 and 3.2.
 *
 * That interface is a C++ class with virtual functions. This is C, so the object is
 * laid out by hand as the ARM C++ ABI (the Itanium ABI) has it, which is what the
 * service's compiler expects: its first word points into a table of the virtual
 * functions in the order the class declares them, after two words (offset to top,
 * type info) that come before the point it refers to. The virtual destructor takes
 * two slots, the complete-object and the deleting destructor. Each function takes
 * the object as its first argument; C++ references are pointers. test/
 * legacy_plugin_abi_test.cpp checks this layout against the real header.
 *
 * How the service uses it (packages/TtsService/jni/android_tts_SynthProxy.cpp):
 * synthesizeText gets a 2048-byte buffer to fill; each time it is full the engine
 * calls the callback, which plays or writes it and clears it, and the buffer is
 * reused. The last call says TTS_SYNTH_DONE, exactly once: the service frees its
 * record of the request then, so a second one, or a null buffer, would free it
 * twice. stop() comes on another thread while synthesizeText runs.
 *
 * The settings are the app's, read from files/klattara.conf next to this library
 * (src/plat/android/rust/src/legacy.rs), since this runs in the service's process.
 */

#include <dlfcn.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../../windows/klattara.h"

/* From TtsEngine.h. */
enum { TTS_SYNTH_DONE = 0, TTS_SYNTH_PENDING = 1 };
enum { TTS_CALLBACK_HALT = 0, TTS_CALLBACK_CONTINUE = 1 };
enum {
    TTS_SUCCESS = 0,
    TTS_FAILURE = -1,
    TTS_PROPERTY_UNSUPPORTED = -4,
    TTS_PROPERTY_SIZE_TOO_SMALL = -5,
};
enum {
    TTS_LANG_COUNTRY_AVAILABLE = 1,
    TTS_LANG_AVAILABLE = 0,
    TTS_LANG_NOT_SUPPORTED = -2,
};
/* AudioSystem::PCM_16_BIT in Android 2.2 to 3.2 (media/AudioSystem.h). */
#define PCM_16_BIT 1

/* synthDoneCB_t: (void *&userdata, uint32_t rate, uint32_t format, int channels,
 *                 int8_t *&wav, size_t &bufferSize, tts_synth_status status) */
typedef int (*synth_done_cb)(void **userdata, uint32_t rate, uint32_t format, int channels,
                             int8_t **wav, size_t *buffer_size, int status);

/* The settings, in Rust (rust/src/legacy.rs). */
typedef struct KlattaraLegacy KlattaraLegacy;
KlattaraLegacy *klattara_legacy_new(const char *path);
void klattara_legacy_free(KlattaraLegacy *legacy);
int klattara_legacy_refresh(KlattaraLegacy *legacy);
int klattara_legacy_prepare(KlattaraLegacy *legacy, KlattaraEngine *engine, int system_rate, int system_pitch);
char *klattara_legacy_text(const KlattaraLegacy *legacy, const char *text);
void klattara_legacy_free_text(char *text);

typedef struct Engine {
    const void *const *vtable;
    synth_done_cb callback;
    KlattaraLegacy *settings;
    KlattaraEngine *engine;
    int engine_rate;
    int system_rate;   /* Android's "rate" property, 100 being normal */
    int system_pitch;  /* and "pitch" */
    volatile int stop_requested;
} Engine;

#define EXPORT __attribute__((visibility("default")))

/* ---- Speaking ------------------------------------------------------------------- */

typedef struct {
    Engine *self;
    void *userdata;
    int8_t *buffer;
    size_t capacity;
    size_t filled;
    uint32_t rate;
    int halted;
} Speech;

/* Hands the full buffer to the service. Returns nonzero to stop. */
static int pass_on(Speech *s, int status) {
    size_t size = s->filled;
    int8_t *wav = s->buffer;
    int result = s->self->callback(&s->userdata, s->rate, PCM_16_BIT, 1, &wav, &size, status);
    if (wav) s->buffer = wav;
    s->filled = 0;
    if (result == TTS_CALLBACK_HALT) s->halted = 1;
    return s->halted;
}

static int on_audio(void *user, const int16_t *samples, int count) {
    Speech *s = user;
    for (int i = 0; i < count; i++) {
        if (s->self->stop_requested || s->halted) return 1;
        if (s->filled + 2 > s->capacity && pass_on(s, TTS_SYNTH_PENDING)) return 1;
        memcpy(s->buffer + s->filled, &samples[i], 2);
        s->filled += 2;
    }
    return s->self->stop_requested || s->halted;
}

/* The engine at the settings' rate, made again if the rate has changed. */
static KlattaraEngine *engine_for(Engine *self) {
    int rate = klattara_legacy_refresh(self->settings);
    if (!self->engine || self->engine_rate != rate) {
        if (self->engine) klattara_destroy(self->engine);
        self->engine = klattara_create(rate);
        self->engine_rate = rate;
    }
    return self->engine;
}

/* ---- The virtual functions, in TtsEngine.h's order -------------------------------- */

static void destroy_complete(Engine *self) { (void)self; }
static void destroy_deleting(Engine *self) { (void)self; }

/* The settings file: files/klattara.conf beside the lib folder this library is in. */
static char *settings_path(void) {
    Dl_info info;
    if (!dladdr((void *)&settings_path, &info) || !info.dli_fname) return NULL;
    const char *lib = strrchr(info.dli_fname, '/');
    if (!lib || lib == info.dli_fname) return NULL;
    size_t dir = (size_t)(lib - info.dli_fname);           /* ".../<package>/lib" */
    const char *slash = NULL;
    for (const char *p = info.dli_fname; p < info.dli_fname + dir; p++)
        if (*p == '/') slash = p;                           /* before "lib" */
    if (!slash) return NULL;
    size_t base = (size_t)(slash - info.dli_fname);
    const char *tail = "/files/klattara.conf";
    char *path = malloc(base + strlen(tail) + 1);
    if (!path) return NULL;
    memcpy(path, info.dli_fname, base);
    strcpy(path + base, tail);
    return path;
}

static int init(Engine *self, synth_done_cb callback, const char *config) {
    (void)config;
    self->callback = callback;
    self->system_rate = 100;
    self->system_pitch = 100;
    self->stop_requested = 0;
    char *path = settings_path();
    self->settings = klattara_legacy_new(path);
    free(path);
    return self->settings ? TTS_SUCCESS : TTS_FAILURE;
}

static int shutdown(Engine *self) {
    if (self->engine) klattara_destroy(self->engine);
    if (self->settings) klattara_legacy_free(self->settings);
    self->engine = NULL;
    self->settings = NULL;
    return TTS_SUCCESS;
}

static int stop(Engine *self) {
    self->stop_requested = 1;
    return TTS_SUCCESS;
}

static int is_english(const char *lang) {
    return lang && (strcmp(lang, "eng") == 0 || strcmp(lang, "en") == 0);
}

static int is_language_available(Engine *self, const char *lang, const char *country, const char *variant) {
    (void)self; (void)variant;
    if (!is_english(lang)) return TTS_LANG_NOT_SUPPORTED;
    if (country && (strcmp(country, "USA") == 0 || strcmp(country, "US") == 0)) return TTS_LANG_COUNTRY_AVAILABLE;
    return TTS_LANG_AVAILABLE;
}

static int load_language(Engine *self, const char *lang, const char *country, const char *variant) {
    return is_language_available(self, lang, country, variant) >= 0 ? TTS_SUCCESS : TTS_FAILURE;
}

static int set_language(Engine *self, const char *lang, const char *country, const char *variant) {
    return load_language(self, lang, country, variant);
}

/* The service passes three buffers of 100 bytes. */
static int get_language(Engine *self, char *language, char *country, char *variant) {
    (void)self;
    if (language) strcpy(language, "eng");
    if (country) strcpy(country, "USA");
    if (variant) variant[0] = 0;
    return TTS_SUCCESS;
}

static int set_audio_format(Engine *self, int *encoding, uint32_t *rate, int *channels) {
    if (encoding) *encoding = PCM_16_BIT;
    if (rate) *rate = (uint32_t)klattara_legacy_refresh(self->settings);
    if (channels) *channels = 1;
    return TTS_SUCCESS;
}

static int set_property(Engine *self, const char *property, const char *value, size_t size) {
    (void)size;
    if (!property || !value) return TTS_FAILURE;
    if (strcmp(property, "rate") == 0) {
        int v = atoi(value);
        if (v > 0) self->system_rate = v;
        return TTS_SUCCESS;
    }
    if (strcmp(property, "pitch") == 0) {
        int v = atoi(value);
        if (v > 0) self->system_pitch = v;
        return TTS_SUCCESS;
    }
    if (strcmp(property, "engineConfig") == 0 || strcmp(property, "volume") == 0) return TTS_SUCCESS;
    return TTS_PROPERTY_UNSUPPORTED;
}

static int get_property(Engine *self, const char *property, char *value, size_t *iosize) {
    if (!property || !iosize) return TTS_FAILURE;
    int v;
    if (strcmp(property, "rate") == 0) v = self->system_rate;
    else if (strcmp(property, "pitch") == 0) v = self->system_pitch;
    else return TTS_PROPERTY_UNSUPPORTED;
    char text[16];
    int n = snprintf(text, sizeof text, "%d", v);
    if (!value || *iosize < (size_t)n + 1) {
        *iosize = (size_t)n + 1;
        return TTS_PROPERTY_SIZE_TOO_SMALL;
    }
    memcpy(value, text, (size_t)n + 1);
    *iosize = (size_t)n + 1;
    return TTS_SUCCESS;
}

static int synthesize_text(Engine *self, const char *text, int8_t *buffer, size_t buffer_size, void *userdata) {
    Speech s;
    memset(&s, 0, sizeof s);
    s.self = self;
    s.userdata = userdata;
    s.buffer = buffer;
    s.capacity = buffer_size & ~(size_t)1;
    self->stop_requested = 0;

    KlattaraEngine *engine = self->settings ? engine_for(self) : NULL;
    s.rate = (uint32_t)self->engine_rate;
    int result = TTS_FAILURE;
    if (engine && buffer && s.capacity >= 2 && text) {
        klattara_legacy_prepare(self->settings, engine, self->system_rate, self->system_pitch);
        char *prepared = klattara_legacy_text(self->settings, text);
        if (prepared) {
            klattara_speak(engine, prepared, KLATTARA_READ_TEXT, on_audio, &s);
            klattara_legacy_free_text(prepared);
            result = TTS_SUCCESS;
        }
    }
    /* Exactly one TTS_SYNTH_DONE, whatever happened: the service frees its record
     * of the request then. What is still in the buffer goes with it, unless
     * speech was stopped. */
    if (self->stop_requested || s.halted) s.filled = 0;
    if (!s.halted) {
        if (!s.buffer) s.buffer = buffer;
        pass_on(&s, TTS_SYNTH_DONE);
    }
    return result;
}

/* ---- The object --------------------------------------------------------------------- */

static const void *const engine_vtable[] = {
    0,                               /* offset to top */
    0,                               /* type info (the service uses none) */
    (const void *)destroy_complete,  /* ~TtsEngine(), complete object */
    (const void *)destroy_deleting,  /* ~TtsEngine(), deleting */
    (const void *)init,
    (const void *)shutdown,
    (const void *)stop,
    (const void *)is_language_available,
    (const void *)load_language,
    (const void *)set_language,
    (const void *)get_language,
    (const void *)set_audio_format,
    (const void *)set_property,
    (const void *)get_property,
    (const void *)synthesize_text,
};

static Engine the_engine = { &engine_vtable[2], 0, 0, 0, 0, 100, 100, 0 };

/* extern "C" TtsEngine* getTtsEngine(): what the service looks up by name. */
EXPORT void *getTtsEngine(void) {
    return &the_engine;
}
