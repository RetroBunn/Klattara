// Settings shared by the Klattara app, which edits them, and the speech
// synthesis extension, which speaks with them.
//
// They are two processes, and the extension is sandboxed (macOS will not load
// an unsandboxed one), so the settings live in a file in the App Group
// container both are entitled to. The extension reads it before every
// utterance, re-parsing only when the file has changed.
//
// The precise rate and pitch follow the NVDA add-on's settings panel: an exact
// rate in words per minute (30 to 1920, the engine's measured range) and an
// exact base pitch in hertz (40 to 550). Both are kept as whole numbers.

import Foundation
import Security

public struct KlattaraSettings: Codable, Equatable, Sendable {

    public static let wpmRange = 30...1920
    public static let pitchRange = 40...550
    public static let inflectionRange = 0...250
    public static let volumeRange = 0...200
    public static let flutterRange = 0...100
    /// Every rate the engine is tuned for, in Hz: the same twenty the NVDA
    /// add-on offers. 22050 is the engine's recommended rate.
    public static let sampleRates = [
        8000, 10000, 11000, 11025, 12000, 16000, 20000, 22000, 22050, 24000,
        32000, 40000, 44000, 44100, 48000, 64000, 80000, 88000, 88200, 96000,
    ]
    public static let defaultSampleRate = 22050

    /// The rate the engine synthesizes at, for the system voices, the app's
    /// Speak button and saved WAV files alike.
    public var sampleRate: Int = KlattaraSettings.defaultSampleRate

    /// The voice the app's test field speaks with; the system picks its own.
    public var voice: Int = 0
    /// The speaking rate in words per minute. 190 is Klattara's normal rate.
    public var rateWPM: Int = 190
    /// Let VoiceOver's rate, and an app's, scale `rateWPM`. Off: always exactly
    /// `rateWPM`, whatever VoiceOver's rate is set to.
    public var followSystemRate: Bool = true
    /// Speak at `pitchHz` rather than at each voice's own pitch.
    public var useExactPitch: Bool = false
    /// The exact base pitch in hertz, when `useExactPitch` is on.
    public var pitchHz: Int = 110
    /// Let VoiceOver's pitch, and an app's, move the pitch.
    public var followSystemPitch: Bool = true
    /// Pitch movement, percent of normal: 0 is monotone.
    public var inflection: Int = 100
    /// Flutter, percent: how much the pitch wavers, slowly and unevenly, as a
    /// shaky voice does. Off unless turned on, as in the NVDA add-on; the
    /// engine's voices would otherwise bring their own (25, David 60).
    public var flutter: Int = 0
    /// Volume, percent: 100 is normal.
    public var volume: Int = 100
    /// Read abbreviations in full: "Dr." as "doctor".
    public var expandAbbreviations: Bool = true
    /// Break long stretches without punctuation into phrases.
    public var phrasePrediction: Bool = false
    /// Each voice's customizations of its [:cv] settings (KlattaraParameter),
    /// by the voice's short name in lower case ("steve"), then by code ("hs").
    /// Only what was changed is here; everything else is the voice's own.
    public var voiceParameters: [String: [String: Double]] = [:]

    public init() {}

    /// A voice's customizations.
    public func parameters(for voice: KlattaraVoice) -> [String: Double] {
        voiceParameters[voice.name.lowercased()] ?? [:]
    }

    /// Sets one of a voice's customizations, or with nil puts it back to the
    /// voice's own. Richness and open quotient set the same thing, so setting
    /// one drops the other.
    public mutating func setParameter(_ code: String, to value: Double?, for voice: KlattaraVoice) {
        let key = voice.name.lowercased()
        var own = voiceParameters[key] ?? [:]
        own[code] = value
        if value != nil, let partner = KlattaraParameter.partners[code] { own[partner] = nil }
        voiceParameters[key] = own.isEmpty ? nil : own
    }

    /// Puts all of a voice's settings back to its own.
    public mutating func resetParameters(for voice: KlattaraVoice) {
        voiceParameters[voice.name.lowercased()] = nil
    }

    // Every key optional, so a file from an older version still loads.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = KlattaraSettings()
        voice = try c.decodeIfPresent(Int.self, forKey: .voice) ?? d.voice
        rateWPM = try c.decodeIfPresent(Int.self, forKey: .rateWPM) ?? d.rateWPM
        followSystemRate = try c.decodeIfPresent(Bool.self, forKey: .followSystemRate) ?? d.followSystemRate
        useExactPitch = try c.decodeIfPresent(Bool.self, forKey: .useExactPitch) ?? d.useExactPitch
        pitchHz = try c.decodeIfPresent(Int.self, forKey: .pitchHz) ?? d.pitchHz
        followSystemPitch = try c.decodeIfPresent(Bool.self, forKey: .followSystemPitch) ?? d.followSystemPitch
        flutter = try c.decodeIfPresent(Int.self, forKey: .flutter) ?? d.flutter
        inflection = try c.decodeIfPresent(Int.self, forKey: .inflection) ?? d.inflection
        volume = try c.decodeIfPresent(Int.self, forKey: .volume) ?? d.volume
        expandAbbreviations = try c.decodeIfPresent(Bool.self, forKey: .expandAbbreviations) ?? d.expandAbbreviations
        phrasePrediction = try c.decodeIfPresent(Bool.self, forKey: .phrasePrediction) ?? d.phrasePrediction
        sampleRate = try c.decodeIfPresent(Int.self, forKey: .sampleRate) ?? d.sampleRate
        voiceParameters = (try? c.decodeIfPresent([String: [String: Double]].self, forKey: .voiceParameters)) ?? [:]
        clamp()
    }

    /// Brings every value into its range.
    public mutating func clamp() {
        rateWPM = Self.wpmRange.clamped(rateWPM)
        pitchHz = Self.pitchRange.clamped(pitchHz)
        inflection = Self.inflectionRange.clamped(inflection)
        volume = Self.volumeRange.clamped(volume)
        flutter = Self.flutterRange.clamped(flutter)
        voice = max(0, voice)
        if !Self.sampleRates.contains(sampleRate) { sampleRate = Self.defaultSampleRate }
        // Known settings only, each within its range.
        voiceParameters = voiceParameters.compactMapValues { own in
            var kept: [String: Double] = [:]
            for (code, value) in own {
                guard let p = KlattaraParameter.named(code), value.isFinite else { continue }
                kept[code] = min(p.range.upperBound, max(p.range.lowerBound, value))
            }
            return kept.isEmpty ? nil : kept
        }
    }
}

extension ClosedRange where Bound == Int {
    public func clamped(_ value: Int) -> Int { Swift.min(upperBound, Swift.max(lowerBound, value)) }
}

public enum KlattaraStorage {

    /// The App Group, from Info.plist (set from the KLATTARA_APP_GROUP build
    /// setting): it must start with the signing team's identifier.
    public static let appGroup: String =
        (Bundle.main.object(forInfoDictionaryKey: "KlattaraAppGroup") as? String) ?? ""

    /// The group container. A build with no team identifier (ad-hoc signed) is
    /// never let into a team-prefixed group container, so it uses
    /// ~/Library/Application Support/Klattara instead, which the entitlements
    /// open to the sandboxed extension.
    public static var directory: URL {
        if hasTeamIdentifier, !appGroup.isEmpty,
           let url = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup) {
            return url
        }
        #if os(macOS)
        var home = NSHomeDirectory()
        if let pw = getpwuid(getuid()) { home = String(cString: pw.pointee.pw_dir) }
        return URL(fileURLWithPath: home).appendingPathComponent("Library/Application Support/Klattara", isDirectory: true)
        #else
        // Without the group (a build signed without it), each side keeps its own.
        return URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Application Support/Klattara", isDirectory: true)
        #endif
    }

    public static var settingsURL: URL { directory.appendingPathComponent("settings.json") }

    /// Whether this process is signed with a team identifier. On iPhone and
    /// iPad every installed app is, and the group container is simply there or not.
    #if os(iOS)
    public static let hasTeamIdentifier = true
    #else
    public static let hasTeamIdentifier: Bool = {
        var code: SecCode?
        guard SecCodeCopySelf([], &code) == errSecSuccess, let code else { return false }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode else { return false }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(staticCode, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
              let dict = info as? [String: Any] else { return false }
        return !((dict[kSecCodeInfoTeamIdentifier as String] as? String) ?? "").isEmpty
    }()
    #endif
}

/// Loads and saves the settings file. Cheap enough to consult for every
/// utterance: it re-reads only when the file's modification date moves.
public final class KlattaraSettingsStore: @unchecked Sendable {

    public static let shared = KlattaraSettingsStore()

    private let lock = NSLock()
    private var cached = KlattaraSettings()
    private var stamp: Date?
    private var loaded = false

    public var current: KlattaraSettings {
        lock.lock()
        defer { lock.unlock() }
        let url = KlattaraStorage.settingsURL
        let now = (try? FileManager.default.attributesOfItem(atPath: url.path)[.modificationDate]) as? Date
        if loaded && now == stamp { return cached }
        loaded = true
        stamp = now
        if let data = try? Data(contentsOf: url),
           let decoded = try? JSONDecoder().decode(KlattaraSettings.self, from: data) {
            cached = decoded
        }
        return cached
    }

    public func save(_ settings: KlattaraSettings) throws {
        var settings = settings
        settings.clamp()
        lock.lock()
        defer { lock.unlock() }
        try FileManager.default.createDirectory(at: KlattaraStorage.directory, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(settings).write(to: KlattaraStorage.settingsURL, options: .atomic)
        cached = settings
        loaded = true
        stamp = (try? FileManager.default.attributesOfItem(atPath: KlattaraStorage.settingsURL.path)[.modificationDate]) as? Date
    }
}
