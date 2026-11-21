// Checks legacy_plugin.c, which lays out a C++ object by hand, against the real
// Android 2.2 header (test/tts/TtsEngine.h, from frameworks/base/include/tts): this
// is C++ code calling the plugin through TtsEngine's virtual functions, as Android's
// speech service does (packages/TtsService/jni/android_tts_SynthProxy.cpp). If the
// table were in the wrong order, these calls would reach the wrong functions.
//
// Built and run by build-legacy.sh on the build machine, with the real engine:
//   c++ -Itest legacy_plugin_abi_test.cpp legacy_plugin.c libklattara_engine.a

#include <tts/TtsEngine.h>

#include <cstdio>
#include <cstring>
#include <thread>
#include <chrono>

using namespace android;

static int pending_calls, done_calls, bytes, bad_buffers, wrong_format;
static uint32_t seen_rate;
static int8_t *expected_buffer;

static tts_callback_status callback(void *&userdata, uint32_t rate, uint32_t format, int channels,
                                    int8_t *&wav, size_t &size, tts_synth_status status) {
    (void)userdata;
    if (wav != expected_buffer) bad_buffers++;
    if (format != AudioSystem::PCM_16_BIT || channels != 1) wrong_format++;
    seen_rate = rate;
    bytes += (int)size;
    memset(wav, 0, size);
    if (status == TTS_SYNTH_DONE) {
        done_calls++;
        return TTS_CALLBACK_HALT;
    }
    pending_calls++;
    return TTS_CALLBACK_CONTINUE;
}

static int failures;
static void check(bool ok, const char *what) {
    printf("%s: %s\n", ok ? "ok" : "FAIL", what);
    if (!ok) failures++;
}

static void reset() {
    pending_calls = done_calls = bytes = bad_buffers = wrong_format = 0;
}

int main() {
    TtsEngine *engine = getTtsEngine();
    check(engine != nullptr, "getTtsEngine() returns the engine");
    check(engine->init(callback, "") == TTS_SUCCESS, "init()");

    check(engine->isLanguageAvailable("eng", "USA", "") == TTS_LANG_COUNTRY_AVAILABLE, "American English is available");
    check(engine->isLanguageAvailable("eng", "GBR", "") == TTS_LANG_AVAILABLE, "other English is available as the language");
    check(engine->isLanguageAvailable("fra", "FRA", "") == TTS_LANG_NOT_SUPPORTED, "French is not");
    check(engine->loadLanguage("eng", "USA", "") == TTS_SUCCESS && engine->setLanguage("eng", "USA", "") == TTS_SUCCESS,
          "loadLanguage() and setLanguage()");
    char lang[100], country[100], variant[100];
    engine->getLanguage(lang, country, variant);
    check(!strcmp(lang, "eng") && !strcmp(country, "USA") && !variant[0], "getLanguage() is eng, USA");

    AudioSystem::audio_format encoding = AudioSystem::INVALID_FORMAT;
    uint32_t rate = 0;
    int channels = 0;
    engine->setAudioFormat(encoding, rate, channels);
    check(encoding == AudioSystem::PCM_16_BIT && rate == 22050 && channels == 1, "setAudioFormat(): 16-bit mono, 22050 Hz");

    check(engine->setProperty("rate", "100", 12) == TTS_SUCCESS && engine->setProperty("pitch", "100", 12) == TTS_SUCCESS,
          "setProperty() takes rate and pitch");
    char value[16];
    size_t size = sizeof value;
    check(engine->getProperty("rate", value, &size) == TTS_SUCCESS && !strcmp(value, "100"), "getProperty() gives the rate back");
    size = 1;
    check(engine->getProperty("pitch", value, &size) == TTS_PROPERTY_SIZE_TOO_SMALL && size == 4,
          "getProperty() says how much room it needs");
    check(engine->setProperty("nonsense", "1", 2) == TTS_PROPERTY_UNSUPPORTED, "an unknown property is unsupported");

    int8_t buffer[2048];
    expected_buffer = buffer;
    const char *text = "Hello from Klattara on Android two point two. The quick brown fox jumps over the lazy dog.";

    reset();
    check(engine->synthesizeText(text, buffer, sizeof buffer, (void *)1) == TTS_SUCCESS, "synthesizeText()");
    int normal = bytes;
    check(done_calls == 1, "exactly one TTS_SYNTH_DONE");
    check(pending_calls > 10 && bad_buffers == 0 && wrong_format == 0 && seen_rate == 22050,
          "speech comes in the service's own buffer, 16-bit mono at 22050 Hz");
    printf("    %d bytes, %.2f seconds, in %d buffers\n", normal, normal / 2.0 / 22050, pending_calls + 1);

    engine->setProperty("rate", "200", 12);
    reset();
    engine->synthesizeText(text, buffer, sizeof buffer, (void *)1);
    check(done_calls == 1 && bytes < normal * 0.65, "rate 200 is about twice as fast");

    engine->setProperty("rate", "100", 12);
    reset();
    std::thread stopper([&] {
        std::this_thread::sleep_for(std::chrono::milliseconds(2));
        engine->stop();
    });
    engine->synthesizeText(text, buffer, sizeof buffer, (void *)1);
    stopper.join();
    check(done_calls == 1, "stop() from another thread still ends with exactly one TTS_SYNTH_DONE");
    check(bytes < normal, "and stops the speech early");

    reset();
    engine->synthesizeText(text, buffer, sizeof buffer, (void *)1);
    check(done_calls == 1 && bytes == normal, "the next request after a stop speaks in full");

    reset();
    engine->synthesizeText("Say [:volume 0] this.", buffer, sizeof buffer, (void *)1);
    check(done_calls == 1 && bytes > 0, "a command in the text is read, not obeyed");

    check(engine->shutdown() == TTS_SUCCESS, "shutdown()");
    printf(failures ? "%d FAILED\n" : "All legacy plugin checks passed.\n", failures);
    return failures != 0;
}
