// The extension's principal class (NSExtensionPrincipalClass in Info.plist):
// what the system asks for the audio unit.

import AVFoundation
import AudioToolbox

public final class AudioUnitFactory: NSObject, AUAudioUnitFactory {

    var audioUnit: AUAudioUnit?

    public func beginRequest(with context: NSExtensionContext) {}

    @objc
    public func createAudioUnit(with componentDescription: AudioComponentDescription) throws -> AUAudioUnit {
        let unit = try KlattaraSpeechAudioUnit(componentDescription: componentDescription, options: [])
        audioUnit = unit
        return unit
    }
}
