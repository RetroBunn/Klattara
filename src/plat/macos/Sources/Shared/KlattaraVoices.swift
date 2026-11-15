// Klattara's voices, as the engine defines them, the default first.

import CKlattara
import Foundation

public struct KlattaraVoice: Identifiable, Hashable, Sendable {
    public let id: Int
    /// The engine's short name, such as "Steve".
    public let name: String
    /// The full name, such as "Standard Steve".
    public let fullName: String
    public let pitch: Double
    public let isFemale: Bool

    /// The name the system shows, in VoiceOver Utility's voice list and
    /// Spoken Content: "Klattara Standard Steve".
    public var systemName: String { "Klattara \(fullName)" }

    /// The identifier the system knows the voice by. It never changes, so a
    /// voice chosen in VoiceOver stays chosen across updates.
    public var identifier: String { "com.dgomeze42.klattara.voice.\(name.lowercased())" }

    public static let all: [KlattaraVoice] = (0..<Int(klattara_voice_count())).compactMap { i in
        let index = Int32(i)
        guard let name = klattara_voice_name(index), let full = klattara_voice_full_name(index) else { return nil }
        return KlattaraVoice(id: i,
                             name: String(cString: name),
                             fullName: String(cString: full),
                             pitch: klattara_voice_pitch(index),
                             isFemale: klattara_voice_gender(index) == KLATTARA_FEMALE)
    }

    /// The voice an identifier names; the default voice if none. The system
    /// puts the extension's own identifier in front of a voice's
    /// ("com.dgomeze42.klattara.voices.com.dgomeze42.klattara.voice.steve"),
    /// so a voice is matched by how the identifier ends.
    public static func voice(forIdentifier identifier: String) -> KlattaraVoice {
        all.first { identifier == $0.identifier || identifier.hasSuffix("." + $0.identifier) } ?? all[0]
    }
}
