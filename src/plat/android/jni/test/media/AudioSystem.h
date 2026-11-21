/* Just enough of Android 2.2's media/AudioSystem.h for tts/TtsEngine.h:
 * the audio_format values (frameworks/base/include/media/AudioSystem.h). */
#ifndef KLATTARA_TEST_AUDIOSYSTEM_H
#define KLATTARA_TEST_AUDIOSYSTEM_H
#include <stdint.h>
#include <stddef.h>
namespace android {
class AudioSystem {
public:
    enum audio_format { INVALID_FORMAT = -1, FORMAT_DEFAULT = 0, PCM_16_BIT = 1, PCM_8_BIT = 2 };
};
}
#endif
