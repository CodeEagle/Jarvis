import AVFoundation
import Foundation
import Speech
import os.log

/// Press-to-talk wrapper around `SFSpeechRecognizer` + `AVAudioEngine`.
///
/// m1 ships PTT only — auto-VAD, barge-in, and TTS land in m3. This
/// covers the bare-minimum voice loop the user asked for: hold the
/// mic, speak, release; the live transcript pipes through `partial`
/// → final transcript fires on release.
@MainActor
final class SpeechCoordinator: ObservableObject {

    enum Status: Equatable {
        case idle
        case requestingPermission
        case unavailable(String)   // permission denied / model missing
        case ready                  // permissions granted, mic warm
        case listening
    }

    @Published private(set) var status: Status = .idle
    @Published private(set) var partialTranscript: String = ""
    @Published private(set) var lastError: String?

    private let logger = Logger(subsystem: "ai.jarvis.mac", category: "speech")
    private let recognizer: SFSpeechRecognizer?
    private let audioEngine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?

    /// Honoured when the recognizer supports it (macOS 13+ does).
    /// Forces inference on-device — no audio leaves the machine.
    private let onDeviceOnly: Bool

    init(locale: Locale = Locale(identifier: "zh-CN"), onDeviceOnly: Bool = true) {
        // Prefer a Chinese recognizer; fall back to system default.
        self.recognizer = SFSpeechRecognizer(locale: locale)
            ?? SFSpeechRecognizer()
        self.onDeviceOnly = onDeviceOnly
    }

    // MARK: - Permissions

    func requestPermissions() async {
        status = .requestingPermission
        let speechAuth: SFSpeechRecognizerAuthorizationStatus =
            await withCheckedContinuation { cont in
                SFSpeechRecognizer.requestAuthorization { cont.resume(returning: $0) }
            }
        guard speechAuth == .authorized else {
            status = .unavailable(speechAuthMessage(speechAuth))
            return
        }
        // AVCaptureDevice mic permission — separate prompt.
        let micGranted: Bool = await withCheckedContinuation { cont in
            AVCaptureDevice.requestAccess(for: .audio) { cont.resume(returning: $0) }
        }
        guard micGranted else {
            status = .unavailable("microphone access denied — enable in System Settings → Privacy & Security → Microphone")
            return
        }
        guard let recognizer, recognizer.isAvailable else {
            status = .unavailable("speech recognizer unavailable on this device")
            return
        }
        status = .ready
    }

    // MARK: - Press-to-talk

    func start() {
        guard status == .ready || status == .idle else { return }
        do {
            try beginAudio()
            status = .listening
            partialTranscript = ""
            lastError = nil
        } catch {
            lastError = error.localizedDescription
            logger.error("start failed: \(error.localizedDescription, privacy: .public)")
            status = .unavailable(error.localizedDescription)
            tearDown()
        }
    }

    /// Stop the recording session and return the final transcript.
    /// Returns `nil` if recognition produced nothing usable.
    func stop() async -> String? {
        guard status == .listening else { return nil }
        status = .ready

        request?.endAudio()
        if audioEngine.isRunning {
            audioEngine.stop()
            audioEngine.inputNode.removeTap(onBus: 0)
        }

        // Wait briefly for the final result. The recognition task
        // delivers a final segment shortly after endAudio.
        let final = await waitForFinal(maxWait: .seconds(2))
        tearDown()
        let trimmed = (final ?? partialTranscript).trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    func cancel() {
        request?.endAudio()
        task?.cancel()
        if audioEngine.isRunning {
            audioEngine.stop()
            audioEngine.inputNode.removeTap(onBus: 0)
        }
        tearDown()
        status = .ready
    }

    // MARK: -

    private func beginAudio() throws {
        guard let recognizer else {
            throw NSError(domain: "Speech", code: 1, userInfo: [
                NSLocalizedDescriptionKey: "no speech recognizer available"
            ])
        }
        let req = SFSpeechAudioBufferRecognitionRequest()
        req.shouldReportPartialResults = true
        if onDeviceOnly, recognizer.supportsOnDeviceRecognition {
            req.requiresOnDeviceRecognition = true
        }
        self.request = req

        let inputNode = audioEngine.inputNode
        let format = inputNode.outputFormat(forBus: 0)
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buf, _ in
            self?.request?.append(buf)
        }

        audioEngine.prepare()
        try audioEngine.start()

        self.task = recognizer.recognitionTask(with: req) { [weak self] result, error in
            Task { @MainActor in
                guard let self else { return }
                if let result {
                    self.partialTranscript = result.bestTranscription.formattedString
                }
                if let error {
                    // Cancellation surfaces here; ignore non-fatal.
                    let nsError = error as NSError
                    if nsError.code != 203, nsError.code != 216 {
                        self.lastError = error.localizedDescription
                        self.logger.error("rec task error: \(error.localizedDescription, privacy: .public)")
                    }
                }
            }
        }
    }

    private func waitForFinal(maxWait: Duration) async -> String? {
        // Poll the recognition task's state for up to `maxWait`.
        // SFSpeechRecognitionTask.state values are
        //   .starting / .running / .finishing / .canceling / .completed
        let pollInterval: Duration = .milliseconds(50)
        var waited: Duration = .zero
        while waited < maxWait {
            switch task?.state {
            case .finishing?, .completed?, nil:
                return partialTranscript
            default:
                break
            }
            try? await Task.sleep(for: pollInterval)
            waited += pollInterval
        }
        return partialTranscript
    }

    private func tearDown() {
        request = nil
        task = nil
    }

    private func speechAuthMessage(_ s: SFSpeechRecognizerAuthorizationStatus) -> String {
        switch s {
        case .denied:       return "speech recognition denied — enable in System Settings → Privacy & Security → Speech Recognition"
        case .restricted:   return "speech recognition restricted by this device"
        case .notDetermined: return "speech recognition not granted"
        default:            return "speech recognition unavailable"
        }
    }
}
