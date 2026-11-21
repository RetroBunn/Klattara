// The voice-quality settings Klattara documents for its [:cv] command
// (klattara.h, and commands.rs in the engine), as the app offers them.
//
// Every voice has its own value of each, and choosing a voice brings those
// back, so customizations are kept per voice and sent as one [:cv] command
// after the voice is chosen. Pitch (pi), inflection (in) and flutter (fl)
// have their own controls in the app and are not repeated here.

import CKlattara
import Foundation

public struct KlattaraParameter: Identifiable, Hashable, Sendable {
    public enum Group: String, CaseIterable, Sendable {
        case quality = "Voice quality"
        case sources = "Source levels"
        case melody = "Melody"
        case formants = "Upper formants"
    }

    /// The [:cv] code, such as "hs".
    public let code: String
    public let name: String
    public let help: String
    public let range: ClosedRange<Double>
    public let step: Double
    public let unit: String
    public let group: Group

    public var id: String { code }

    /// The voice's own value; nil where the voice leaves it to the engine.
    public func defaultValue(for voice: KlattaraVoice) -> Double? {
        let value = klattara_mac_voice_setting(Int32(voice.id), code)
        return value.isFinite ? value : nil
    }

    public static let all: [KlattaraParameter] = [
        .init(code: "ge", name: "Gender", help: "The model of the vocal tract: female or male. It changes nothing else, so set the pitch too.",
              range: 0...1, step: 1, unit: "", group: .quality),
        .init(code: "hs", name: "Head size", help: "Percent of normal. Smaller sounds younger and higher, larger deeper.",
              range: 64...135, step: 1, unit: "percent", group: .quality),
        .init(code: "rg", name: "Roughness", help: "From none to the most: unevenness from one pulse of the voice to the next.",
              range: 0...100, step: 1, unit: "", group: .quality),
        .init(code: "br", name: "Breathiness", help: "Breath running through the voice, in decibels.",
              range: 0...72, step: 1, unit: "decibels", group: .quality),
        .init(code: "lx", name: "Lax breathiness", help: "Breath at the ends of sentences, percent.",
              range: 0...100, step: 1, unit: "percent", group: .quality),
        .init(code: "rc", name: "Richness", help: "Percent. More is thinner and buzzier, less fuller. Sets the open quotient, from the other end.",
              range: 0...100, step: 1, unit: "percent", group: .quality),
        .init(code: "oq", name: "Open quotient", help: "Percent of each period the vocal folds are open. Sets the same thing as richness.",
              range: 20...90, step: 1, unit: "percent", group: .quality),
        .init(code: "sm", name: "Smoothness", help: "Percent. More softens the voice, but not the consonants.",
              range: 0...100, step: 1, unit: "percent", group: .quality),

        .init(code: "vvl", name: "Voicing level", help: "Percent of normal: vowels and everything the vocal folds sound. 0 is off.",
              range: 0...200, step: 5, unit: "percent", group: .sources),
        .init(code: "fvl", name: "Frication level", help: "Percent of normal: the hiss of S and F, and the bursts of stops.",
              range: 0...200, step: 5, unit: "percent", group: .sources),
        .init(code: "avl", name: "Aspiration level", help: "Percent of normal: H, and the puff after P, T and K.",
              range: 0...200, step: 5, unit: "percent", group: .sources),
        .init(code: "nvl", name: "Nasal level", help: "Percent of normal: M, N and NG.",
              range: 0...200, step: 5, unit: "percent", group: .sources),

        .init(code: "fa", name: "Baseline fall", help: "How far the pitch falls through a phrase, 0 to 90. 18 is normal (DECtalk's bf).",
              range: 0...90, step: 1, unit: "", group: .melody),
        .init(code: "ha", name: "Hat rise", help: "The slow rise over a phrase, 0 to 100. 18 is normal (DECtalk's hr).",
              range: 0...100, step: 1, unit: "", group: .melody),
        .init(code: "st", name: "Stress rise", help: "The size of each accent, 0 to 100. 32 is normal (DECtalk's sr).",
              range: 0...100, step: 1, unit: "", group: .melody),
        .init(code: "qk", name: "Quickness", help: "How quickly the pitch moves, percent. 40 is normal (DECtalk's qu).",
              range: 0...100, step: 1, unit: "percent", group: .melody),

        .init(code: "f4", name: "F4", help: "The fourth formant, in hertz. Automatic follows head size and gender.",
              range: 2000...6000, step: 50, unit: "hertz", group: .formants),
        .init(code: "b4", name: "F4 bandwidth", help: "In hertz.",
              range: 100...4000, step: 10, unit: "hertz", group: .formants),
        .init(code: "f5", name: "F5", help: "The fifth formant, in hertz.",
              range: 2500...6000, step: 50, unit: "hertz", group: .formants),
        .init(code: "b5", name: "F5 bandwidth", help: "In hertz.",
              range: 100...4000, step: 10, unit: "hertz", group: .formants),
        .init(code: "f6", name: "F6", help: "The sixth formant, in hertz.",
              range: 3000...9000, step: 50, unit: "hertz", group: .formants),
        .init(code: "b6", name: "F6 bandwidth", help: "In hertz.",
              range: 100...4000, step: 10, unit: "hertz", group: .formants),
    ]

    public static func named(_ code: String) -> KlattaraParameter? { all.first { $0.code == code } }

    /// The two codes that set the same thing; setting one clears the other.
    public static let partners: [String: String] = ["rc": "oq", "oq": "rc"]

    /// The [:cv] command for a voice's customizations, or nil if there are none.
    public static func command(for overrides: [String: Double]) -> String? {
        let parts = all.compactMap { p -> String? in
            guard let value = overrides[p.code] else { return nil }
            let clamped = min(p.range.upperBound, max(p.range.lowerBound, value))
            // Whole numbers as such; anything else to full precision, so a value
            // taken from a voice (richness 35.714…) is that voice's exactly.
            let text = clamped == clamped.rounded() ? String(Int(clamped)) : String(format: "%.10g", clamped)
            return "\(p.code) \(text)"
        }
        return parts.isEmpty ? nil : "[:cv \(parts.joined(separator: " "))]"
    }
}
