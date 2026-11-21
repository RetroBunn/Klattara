// What the system asks a speech synthesis extension to say arrives as SSML:
// the text, with VoiceOver's (or an app's) rate, pitch and volume as
// <prosody> attributes, pauses as <break>, and spelling as <say-as>.
// This turns it into pieces Klattara can speak.

import Foundation

public enum SpeechItem: Equatable, Sendable {
    case text(SpeechSpan)
    /// Silence, in seconds.
    case pause(Double)
}

public struct SpeechSpan: Equatable, Sendable {
    public var text: String
    /// Multipliers from <prosody>: 1 is as the user set it.
    public var rate: Double = 1
    public var pitch: Double = 1
    public var volume: Double = 1
    /// Character by character, from <say-as interpret-as="characters">.
    public var spelled = false
}

public enum SpeechMarkup {

    public static func parse(_ ssml: String) -> [SpeechItem] {
        let reader = Reader()
        let parser = XMLParser(data: Data(ssml.utf8))
        parser.delegate = reader
        if parser.parse() {
            return reader.items
        }
        // Not well-formed: speak the text with the tags taken out.
        let plain = ssml.replacingOccurrences(of: "<[^>]*>", with: " ", options: .regularExpression)
        return [.text(SpeechSpan(text: decodeEntities(plain)))]
    }

    private static func decodeEntities(_ s: String) -> String {
        s.replacingOccurrences(of: "&lt;", with: "<").replacingOccurrences(of: "&gt;", with: ">")
            .replacingOccurrences(of: "&quot;", with: "\"").replacingOccurrences(of: "&apos;", with: "'")
            .replacingOccurrences(of: "&amp;", with: "&")
    }

    /// A rate or pitch value as a multiplier: "150%", "+20%", "-10%", "1.5",
    /// or a keyword.
    static func scale(_ value: String, keywords: [String: Double]) -> Double? {
        let v = value.trimmingCharacters(in: .whitespaces).lowercased()
        if let k = keywords[v] { return k }
        if v.hasSuffix("%"), let n = Double(v.dropLast()) {
            return (v.hasPrefix("+") || v.hasPrefix("-")) ? max(0.05, 1 + n / 100) : max(0.05, n / 100)
        }
        if v.hasSuffix("db"), let n = Double(v.dropLast(2)) { return pow(10, n / 20) }
        if let n = Double(v), n > 0 { return n }
        return nil
    }

    static let rateWords: [String: Double] = ["x-slow": 0.5, "slow": 0.75, "medium": 1, "default": 1, "fast": 1.5, "x-fast": 2]
    static let pitchWords: [String: Double] = ["x-low": 0.7, "low": 0.85, "medium": 1, "default": 1, "high": 1.15, "x-high": 1.3]
    static let volumeWords: [String: Double] = ["silent": 0, "x-soft": 0.4, "soft": 0.7, "medium": 1, "default": 1, "loud": 1.3, "x-loud": 1.6]

    private final class Reader: NSObject, XMLParserDelegate {
        var items: [SpeechItem] = []
        private var stack: [SpeechSpan] = [SpeechSpan(text: "")]

        func parser(_ parser: XMLParser, didStartElement name: String, namespaceURI: String?,
                    qualifiedName: String?, attributes: [String: String] = [:]) {
            var span = stack.last!
            span.text = ""
            switch name.lowercased() {
            case "prosody":
                if let r = attributes["rate"].flatMap({ SpeechMarkup.scale($0, keywords: SpeechMarkup.rateWords) }) { span.rate *= r }
                if let p = attributes["pitch"].flatMap({ SpeechMarkup.scale($0, keywords: SpeechMarkup.pitchWords) }) { span.pitch *= p }
                if let v = attributes["volume"].flatMap({ SpeechMarkup.scale($0, keywords: SpeechMarkup.volumeWords) }) { span.volume *= v }
            case "say-as":
                let how = (attributes["interpret-as"] ?? "").lowercased()
                if how == "characters" || how == "spell-out" || how == "letters" { span.spelled = true }
            case "break":
                let seconds: Double
                if let time = attributes["time"]?.lowercased() {
                    if time.hasSuffix("ms") { seconds = (Double(time.dropLast(2)) ?? 0) / 1000 }
                    else if time.hasSuffix("s") { seconds = Double(time.dropLast()) ?? 0 }
                    else { seconds = 0.5 }
                } else {
                    switch (attributes["strength"] ?? "medium").lowercased() {
                    case "none": seconds = 0
                    case "x-weak": seconds = 0.1
                    case "weak": seconds = 0.2
                    case "strong": seconds = 0.75
                    case "x-strong": seconds = 1.2
                    default: seconds = 0.4
                    }
                }
                if seconds > 0 { items.append(.pause(min(seconds, 10))) }
            default:
                break
            }
            stack.append(span)
        }

        func parser(_ parser: XMLParser, didEndElement name: String, namespaceURI: String?, qualifiedName: String?) {
            if stack.count > 1 { stack.removeLast() }
        }

        func parser(_ parser: XMLParser, foundCharacters string: String) {
            guard !string.isEmpty else { return }
            var span = stack.last!
            // Join with the piece before when nothing about the voice changed.
            if case .text(var last)? = items.last,
               last.rate == span.rate, last.pitch == span.pitch,
               last.volume == span.volume, last.spelled == span.spelled {
                last.text += string
                items[items.count - 1] = .text(last)
            } else {
                span.text = string
                items.append(.text(span))
            }
        }
    }
}
