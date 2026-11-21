// The Klattara app: carries the speech synthesis extension (which is what puts
// Klattara's voices in VoiceOver Utility and Spoken Content), and edits the
// settings the extension speaks with.

import AVFoundation
import SwiftUI
import UniformTypeIdentifiers

@main
struct KlattaraApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup("Klattara") {
            ContentView(model: model)
                .frame(minWidth: 520, minHeight: 560)
        }
        .windowResizability(.contentSize)
    }
}

@MainActor
final class AppModel: ObservableObject {

    @Published var settings: KlattaraSettings {
        didSet {
            if settings != oldValue { persist() }
        }
    }
    @Published var testText = "Hello. This is Klattara, a Klatt-style formant synthesizer, speaking on the Mac."
    @Published var status = ""

    private let engine = AVAudioEngine()
    private var speaker: KlattaraSpeaker?
    private var source: AVAudioSourceNode?
    private let synthesizer = AVSpeechSynthesizer()

    init() {
        settings = KlattaraSettingsStore.shared.current
        // Write the file once even if nothing changes, so the extension finds
        // it; and ask the system to read the voice list again, which is what
        // makes the voices appear after an install or an update.
        persist()
        AVSpeechSynthesisProviderVoice.updateSpeechVoices()
    }

    private func persist() {
        do {
            try KlattaraSettingsStore.shared.save(settings)
        } catch {
            status = "Could not save the settings: \(error.localizedDescription)"
        }
    }

    var voice: KlattaraVoice {
        KlattaraVoice.all.indices.contains(settings.voice) ? KlattaraVoice.all[settings.voice] : KlattaraVoice.all[0]
    }

    var systemVoice: AVSpeechSynthesisVoice? {
        // The system's identifier has the extension's in front of ours.
        let own = voice.identifier
        return AVSpeechSynthesisVoice.speechVoices().first { $0.identifier == own || $0.identifier.hasSuffix("." + own) }
    }

    /// Speaks the test text straight through the engine, with these settings.
    func speakDirectly() {
        stop()
        // At the chosen rate itself; the audio engine converts for the device.
        let rate = Double(settings.sampleRate)
        let speaker = KlattaraSpeaker(outputRate: rate)
        let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1)!
        let node = AVAudioSourceNode(format: format) { _, _, frameCount, buffers in
            let list = UnsafeMutableAudioBufferListPointer(buffers)
            if let out = list[0].mData?.assumingMemoryBound(to: Float.self) {
                _ = speaker.render(into: out, frames: Int(frameCount))
            }
            return noErr
        }
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
        do {
            try engine.start()
        } catch {
            status = "Could not start audio: \(error.localizedDescription)"
            return
        }
        self.speaker = speaker
        source = node
        speaker.start(items: KlattaraSynthesis.items(forPlainText: testText), voice: voice, settings: settings)
        status = "Speaking directly with \(voice.fullName) at \(settings.sampleRate) hertz."
    }

    @Published var isSaving = false

    /// Asks where to save, then writes the text box's text as a WAV file, with
    /// the chosen voice, rate, pitch and sample rate.
    func saveWAV() {
        let panel = NSSavePanel()
        panel.title = "Save Speech as WAV"
        panel.allowedContentTypes = [.wav]
        panel.nameFieldStringValue = "Klattara \(voice.fullName) \(settings.sampleRate) Hz.wav"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let text = testText, voice = voice, settings = settings
        isSaving = true
        status = "Saving \(url.lastPathComponent)…"
        Task.detached(priority: .userInitiated) {
            let result: String
            do {
                let seconds = try KlattaraSynthesis.writeWAV(
                    items: KlattaraSynthesis.items(forPlainText: text), voice: voice, settings: settings, to: url)
                result = String(format: "Saved %@: %.1f seconds, %d hertz, 16-bit mono.",
                                url.lastPathComponent, seconds, settings.sampleRate)
            } catch {
                result = "Could not save \(url.lastPathComponent): \(error.localizedDescription)"
            }
            await MainActor.run {
                self.isSaving = false
                self.status = result
            }
        }
    }

    /// Speaks the test text through macOS, the way VoiceOver does.
    func speakThroughSystem() {
        stop()
        guard let systemVoice else {
            status = "\(voice.systemName) is not in the system's voice list yet. Try Refresh System Voices, then wait a moment."
            return
        }
        let utterance = AVSpeechUtterance(string: testText)
        utterance.voice = systemVoice
        synthesizer.speak(utterance)
        status = "Speaking through macOS with \(systemVoice.name)."
    }

    func stop() {
        synthesizer.stopSpeaking(at: .immediate)
        speaker?.cancel()
        speaker = nil
        if let source {
            engine.stop()
            engine.detach(source)
            self.source = nil
        }
    }

    func refreshSystemVoices() {
        AVSpeechSynthesisProviderVoice.updateSpeechVoices()
        let count = AVSpeechSynthesisVoice.speechVoices().filter { $0.identifier.hasPrefix("com.dgomeze42.klattara.voice.") }.count
        status = count > 0
            ? "\(count) Klattara voices are in the system's voice list."
            : "Asked macOS to read the voice list again. It can take a little while."
    }
}

struct ContentView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        Form {
            Section("Voice") {
                Picker("Voice to customize and test", selection: $model.settings.voice) {
                    ForEach(KlattaraVoice.all) { voice in
                        Text(voice.fullName).tag(voice.id)
                    }
                }
                Text("VoiceOver and other apps choose their own voice: VoiceOver Utility, Speech, then the voice menu or Add Voice. The voice settings further down are this voice's own; rate, pitch, flutter and the rest apply to every Klattara voice.")
                    .font(.caption).foregroundStyle(.secondary)
            }

            Section("Rate") {
                NumberField(title: "Rate in words per minute", value: $model.settings.rateWPM,
                            range: KlattaraSettings.wpmRange, step: 10, unit: "words per minute")
                Toggle("Follow VoiceOver's rate", isOn: $model.settings.followSystemRate)
                Text(model.settings.followSystemRate
                     ? "VoiceOver's rate and an app's rate scale this rate."
                     : "Always exactly this rate, whatever VoiceOver's rate is set to.")
                    .font(.caption).foregroundStyle(.secondary)
            }

            Section("Pitch") {
                Toggle("Use base pitch in hertz", isOn: $model.settings.useExactPitch)
                NumberField(title: "Base pitch in hertz", value: $model.settings.pitchHz,
                            range: KlattaraSettings.pitchRange, step: 5, unit: "hertz")
                    .disabled(!model.settings.useExactPitch)
                Toggle("Follow VoiceOver's pitch", isOn: $model.settings.followSystemPitch)
                NumberField(title: "Inflection, percent of normal", value: $model.settings.inflection,
                            range: KlattaraSettings.inflectionRange, step: 10, unit: "percent")
                LabeledContent {
                    HStack {
                        Slider(value: Binding(get: { Double(model.settings.flutter) },
                                              set: { model.settings.flutter = Int($0.rounded()) }),
                               in: Double(KlattaraSettings.flutterRange.lowerBound)...Double(KlattaraSettings.flutterRange.upperBound),
                               step: 5)
                            .accessibilityLabel("Flutter")
                            .accessibilityValue(model.settings.flutter == 0 ? "off" : "\(model.settings.flutter) percent")
                        Text(model.settings.flutter == 0 ? "Off" : "\(model.settings.flutter)%")
                            .monospacedDigit()
                            .frame(width: 44, alignment: .trailing)
                            .accessibilityHidden(true)
                    }
                } label: {
                    Text("Flutter")
                }
                Text("How much the pitch wavers, slowly and unevenly, as a shaky voice does. Off unless you turn it up. Inflection 0 keeps it off too.")
                    .font(.caption).foregroundStyle(.secondary)
            }

            Section("Audio") {
                Picker("Sample rate", selection: $model.settings.sampleRate) {
                    ForEach(KlattaraSettings.sampleRates, id: \.self) { rate in
                        Text(rate == KlattaraSettings.defaultSampleRate ? "\(rate) hertz (recommended)" : "\(rate) hertz")
                            .tag(rate)
                    }
                }
                Text("The rate Klattara synthesizes at, for VoiceOver, the Speak button and saved WAV files. Lower rates sound duller and older; higher rates add brightness. VoiceOver uses a new rate from its next sentence.")
                    .font(.caption).foregroundStyle(.secondary)
            }

            ForEach(KlattaraParameter.Group.allCases, id: \.self) { group in
                Section("\(group.rawValue): \(model.voice.fullName)") {
                    ForEach(KlattaraParameter.all.filter { $0.group == group }) { parameter in
                        ParameterRow(parameter: parameter, voice: model.voice, settings: $model.settings)
                    }
                    if group == KlattaraParameter.Group.allCases.last {
                        Button("Reset \(model.voice.fullName) to Its Own Settings") {
                            model.settings.resetParameters(for: model.voice)
                            announce("\(model.voice.fullName) is back to its own settings")
                        }
                        .disabled(model.settings.parameters(for: model.voice).isEmpty)
                    }
                }
            }

            Section("Reading") {
                NumberField(title: "Volume, percent", value: $model.settings.volume,
                            range: KlattaraSettings.volumeRange, step: 10, unit: "percent")
                Toggle("Read abbreviations in full", isOn: $model.settings.expandAbbreviations)
                Toggle("Phrase prediction", isOn: $model.settings.phrasePrediction)
            }

            Section("Test") {
                TextField("Text to speak", text: $model.testText, axis: .vertical)
                    .lineLimit(3...6)
                HStack {
                    Button("Speak") { model.speakDirectly() }
                        .keyboardShortcut(.defaultAction)
                    Button("Speak Through macOS") { model.speakThroughSystem() }
                    Button("Stop") { model.stop() }
                        .keyboardShortcut(.cancelAction)
                    Button("Save as WAV…") { model.saveWAV() }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(model.isSaving)
                    Spacer()
                    Button("Refresh System Voices") { model.refreshSystemVoices() }
                }
                if !model.status.isEmpty {
                    Text(model.status)
                        .accessibilityAddTraits(.updatesFrequently)
                }
            }
        }
        .formStyle(.grouped)
        .onChangeOf(model.status) { status in
            guard !status.isEmpty else { return }
            announce(status)
        }
    }
}

/// A whole number typed exactly, with a stepper beside it. A value typed past
/// either end goes to the end, and VoiceOver says so.
struct NumberField: View {
    let title: String
    @Binding var value: Int
    let range: ClosedRange<Int>
    let step: Int
    let unit: String

    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        LabeledContent {
            HStack(spacing: 6) {
                TextField(title, text: $text)
                    .labelsHidden()
                    .multilineTextAlignment(.trailing)
                    .monospacedDigit()
                    .frame(width: 80)
                    .focused($focused)
                    .onSubmit(apply)
                    .onChangeOf(focused) { isFocused in if !isFocused { apply() } }
                    .accessibilityLabel(title)
                    .accessibilityHint("\(range.lowerBound) to \(range.upperBound)")
                Stepper(title, value: $value, in: range, step: step)
                    .labelsHidden()
                    .accessibilityLabel(title)
                    .accessibilityValue("\(value) \(unit)")
            }
        } label: {
            Text("\(title) (\(range.lowerBound)–\(range.upperBound))")
        }
        .onAppear { text = "\(value)" }
        .onChangeOf(value) { newValue in if !focused { text = "\(newValue)" } }
    }

    private func apply() {
        guard let typed = Int(text.filter(\.isNumber)) else {
            text = "\(value)"
            return
        }
        let clamped = range.clamped(typed)
        value = clamped
        text = "\(clamped)"
        if clamped != typed {
            announce("\(clamped) \(unit), the \(typed < clamped ? "lowest" : "highest") allowed")
        }
    }
}

/// One of a voice's [:cv] settings: the voice's own value until changed, typed
/// exactly or stepped, with Reset to go back to the voice's own.
struct ParameterRow: View {
    let parameter: KlattaraParameter
    let voice: KlattaraVoice
    @Binding var settings: KlattaraSettings

    @State private var text = ""
    @FocusState private var focused: Bool

    private var own: Double? { parameter.defaultValue(for: voice) }
    private var custom: Double? { settings.parameters(for: voice)[parameter.code] }
    private var current: Double? { custom ?? own }

    private func describe(_ value: Double?) -> String {
        guard let value else { return "automatic" }
        if parameter.code == "ge" { return value >= 0.5 ? "male" : "female" }
        let number = Self.format(value)
        return parameter.unit.isEmpty ? number : "\(number) \(parameter.unit)"
    }

    private func set(_ value: Double?) {
        settings.setParameter(parameter.code, to: value, for: voice)
    }

    /// One decimal at most, and none when it would be ".0" (55.000001 is 55).
    static func format(_ value: Double) -> String {
        let tenths = (value * 10).rounded() / 10
        return tenths == tenths.rounded() ? String(Int(tenths)) : String(format: "%.1f", tenths)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            LabeledContent {
                HStack(spacing: 6) {
                    if parameter.code == "ge" {
                        Picker(parameter.name, selection: Binding(
                            get: { (current ?? 1) >= 0.5 ? 1 : 0 },
                            set: { set(Double($0)) })) {
                            Text("Female").tag(0)
                            Text("Male").tag(1)
                        }
                        .labelsHidden()
                        .frame(width: 110)
                        .accessibilityLabel(parameter.name)
                    } else {
                        TextField(parameter.name, text: $text, prompt: Text(own == nil ? "Automatic" : ""))
                            .labelsHidden()
                            .multilineTextAlignment(.trailing)
                            .monospacedDigit()
                            .frame(width: 80)
                            .focused($focused)
                            .onSubmit(apply)
                            .onChangeOf(focused) { isFocused in if !isFocused { apply() } }
                            .accessibilityLabel(parameter.name)
                            .accessibilityValue(describe(current))
                            .accessibilityHint("\(Int(parameter.range.lowerBound)) to \(Int(parameter.range.upperBound)). \(parameter.help)")
                        Stepper(parameter.name,
                                onIncrement: { step(+1) },
                                onDecrement: { step(-1) })
                            .labelsHidden()
                            .accessibilityLabel(parameter.name)
                            .accessibilityValue(describe(current))
                    }
                    Button("Reset") {
                        set(nil)
                        announce("\(parameter.name), \(voice.fullName)'s own, \(describe(own))")
                    }
                    .disabled(custom == nil)
                    .accessibilityLabel("Reset \(parameter.name)")
                }
            } label: {
                Text("\(parameter.name) (\(parameter.code))")
            }
            Text("\(parameter.help) \(voice.fullName)'s own: \(describe(own)).\(custom != nil ? " Changed." : "")")
                .font(.caption).foregroundStyle(.secondary)
        }
        .onAppear(perform: refresh)
        .onChangeOf(current) { _ in if !focused { refresh() } }
        .onChangeOf(voice) { _ in refresh() }
    }

    private func refresh() {
        guard let current else { text = ""; return }
        text = Self.format(current)
    }

    private func step(_ direction: Double) {
        let start = current ?? (parameter.range.lowerBound + parameter.range.upperBound) / 2
        let next = min(parameter.range.upperBound, max(parameter.range.lowerBound, (start + direction * parameter.step).rounded()))
        set(next)
        refresh()
    }

    private func apply() {
        let typed = text.trimmingCharacters(in: .whitespaces)
        if typed.isEmpty {
            // Emptied: back to the voice's own.
            if custom != nil { set(nil) }
            refresh()
            return
        }
        guard let value = Double(typed.filter { $0.isNumber || $0 == "." }) else { refresh(); return }
        let clamped = min(parameter.range.upperBound, max(parameter.range.lowerBound, value))
        if clamped != current { set(clamped) }
        refresh()
        if clamped != value {
            announce("\(parameter.name), \(describe(clamped)), the \(value < clamped ? "lowest" : "highest") allowed")
        }
    }
}

/// Speaks a short message through VoiceOver; the same on every macOS version.
@MainActor
func announce(_ message: String) {
    NSAccessibility.post(element: NSApp.mainWindow ?? NSApp as Any, notification: .announcementRequested,
                         userInfo: [.announcement: message, .priority: NSAccessibilityPriorityLevel.high.rawValue])
}

extension View {
    /// onChange with the new value, on macOS 13 and on 14 and newer alike.
    @ViewBuilder
    func onChangeOf<V: Equatable>(_ value: V, perform action: @escaping (V) -> Void) -> some View {
        if #available(macOS 14.0, *) {
            onChange(of: value) { _, newValue in action(newValue) }
        } else {
            onChange(of: value, perform: action)
        }
    }
}
