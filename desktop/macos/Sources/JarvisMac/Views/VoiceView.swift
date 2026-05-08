import SwiftUI

/// Voice-first home view. Press the mic, speak, release — the
/// transcript hits `/router/input` and the route decision renders
/// below as a card. Falls back to a text input row if the user denies
/// microphone or speech-recognition permission.
///
/// m1 scope: STT only. TTS, auto-VAD, and barge-in live in m3.
struct VoiceView: View {
    @EnvironmentObject var appState: AppState
    @EnvironmentObject var speech: SpeechCoordinator
    @EnvironmentObject var daemon: DaemonSupervisor

    @State private var fallbackText: String = ""
    @State private var lastSentText: String?
    @FocusState private var fallbackFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            mainArea
            Divider().background(Tokens.Color.border)
            footer
        }
        .background(Tokens.Color.bgPrimary)
        .task {
            // Surface the permission prompt on first arrival so the
            // user understands what we need.
            if speech.status == .idle {
                await speech.requestPermissions()
            }
        }
    }

    // MARK: -

    private var mainArea: some View {
        ScrollView {
            VStack(spacing: Tokens.Space.xl) {
                Spacer(minLength: Tokens.Space.xxl)

                micButton
                    .frame(width: 96, height: 96)

                statusLine

                if !speech.partialTranscript.isEmpty {
                    Text(speech.partialTranscript)
                        .font(Tokens.Font.h2)
                        .foregroundStyle(Tokens.Color.fgPrimary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: 560)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if let sent = lastSentText {
                    sentRow(sent)
                }

                if let decision = appState.lastDecision {
                    DecisionCard(decision: decision)
                        .frame(maxWidth: 640)
                }

                if let err = appState.lastError {
                    errorBanner(err)
                }

                Spacer(minLength: Tokens.Space.xxl)
            }
            .padding(Tokens.Space.xl)
            .frame(maxWidth: .infinity)
        }
    }

    // MARK: - Mic button

    @ViewBuilder
    private var micButton: some View {
        switch speech.status {
        case .idle, .requestingPermission:
            Circle()
                .fill(Tokens.Color.bgSecondary)
                .overlay(
                    Image(systemName: "mic.slash")
                        .font(.system(size: 32, weight: .light))
                        .foregroundStyle(Tokens.Color.fgMuted)
                )
                .overlay(
                    Circle().strokeBorder(Tokens.Color.border, lineWidth: 1)
                )
        case .unavailable:
            Circle()
                .fill(Tokens.Color.statusError.opacity(0.10))
                .overlay(
                    Image(systemName: "exclamationmark.triangle")
                        .font(.system(size: 28, weight: .light))
                        .foregroundStyle(Tokens.Color.statusError)
                )
                .overlay(
                    Circle().strokeBorder(Tokens.Color.statusError.opacity(0.4), lineWidth: 1)
                )
        case .ready:
            Button(action: {}) {
                Circle()
                    .fill(Tokens.Color.bgSecondary)
                    .overlay(
                        Image(systemName: "mic")
                            .font(.system(size: 32, weight: .light))
                            .foregroundStyle(Tokens.Color.accent)
                    )
                    .overlay(
                        Circle().strokeBorder(Tokens.Color.accent.opacity(0.4), lineWidth: 1)
                    )
            }
            .buttonStyle(.plain)
            .simultaneousGesture(pressGesture)
        case .listening:
            Circle()
                .fill(Tokens.Color.accent.opacity(0.18))
                .overlay(
                    Image(systemName: "waveform")
                        .font(.system(size: 32, weight: .regular))
                        .foregroundStyle(Tokens.Color.accent)
                )
                .overlay(
                    Circle().strokeBorder(Tokens.Color.accent, lineWidth: 2)
                )
                .simultaneousGesture(pressGesture)
        }
    }

    /// Press-and-hold: start on press; finalise on release.
    private var pressGesture: some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { _ in
                if speech.status == .ready {
                    speech.start()
                }
            }
            .onEnded { _ in
                if speech.status == .listening {
                    Task { @MainActor in
                        if let text = await speech.stop() {
                            await sendInput(text)
                        }
                    }
                }
            }
    }

    private var statusLine: some View {
        Group {
            switch speech.status {
            case .idle:
                Text("Tap to grant microphone access")
                    .foregroundStyle(Tokens.Color.fgMuted)
            case .requestingPermission:
                Text("Granting access…")
                    .foregroundStyle(Tokens.Color.fgMuted)
            case .ready:
                Text("Hold the mic and speak — release to send")
                    .foregroundStyle(Tokens.Color.fgMuted)
            case .listening:
                Text("Listening… release to send")
                    .foregroundStyle(Tokens.Color.accent)
            case .unavailable(let msg):
                Text(msg)
                    .foregroundStyle(Tokens.Color.statusError)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 480)
            }
        }
        .font(Tokens.Font.body)
    }

    // MARK: - Sent / error rows

    private func sentRow(_ text: String) -> some View {
        HStack(alignment: .top, spacing: Tokens.Space.s) {
            Text("you")
                .font(Tokens.Font.caption)
                .foregroundStyle(Tokens.Color.fgMuted)
                .frame(width: 32, alignment: .trailing)
            Text(text)
                .font(Tokens.Font.body)
                .foregroundStyle(Tokens.Color.fgPrimary)
                .multilineTextAlignment(.leading)
            Spacer(minLength: 0)
        }
        .padding(Tokens.Space.m)
        .background(
            RoundedRectangle(cornerRadius: Tokens.Radius.card)
                .fill(Tokens.Color.bgSecondary)
        )
        .frame(maxWidth: 640)
    }

    private func errorBanner(_ msg: String) -> some View {
        HStack(alignment: .top, spacing: Tokens.Space.s) {
            Image(systemName: "exclamationmark.triangle")
                .foregroundStyle(Tokens.Color.statusError)
            Text(msg)
                .font(Tokens.Font.bodyTight)
                .foregroundStyle(Tokens.Color.fgSecondary)
        }
        .padding(Tokens.Space.m)
        .frame(maxWidth: 640, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: Tokens.Radius.control)
                .fill(Tokens.Color.statusError.opacity(0.08))
        )
    }

    // MARK: - Footer (text fallback)

    private var footer: some View {
        HStack(spacing: Tokens.Space.s) {
            TextField("Or type and press ↩", text: $fallbackText)
                .textFieldStyle(.plain)
                .font(Tokens.Font.body)
                .padding(.horizontal, Tokens.Space.m)
                .frame(height: 32)
                .background(
                    RoundedRectangle(cornerRadius: Tokens.Radius.control)
                        .fill(Tokens.Color.bgSecondary)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: Tokens.Radius.control)
                        .strokeBorder(Tokens.Color.border, lineWidth: 1)
                )
                .focused($fallbackFocused)
                .onSubmit { sendFallback() }
            Button(action: sendFallback) {
                Text("Send")
                    .font(Tokens.Font.body)
                    .foregroundStyle(.white)
                    .padding(.horizontal, Tokens.Space.l)
                    .frame(height: 32)
                    .background(
                        RoundedRectangle(cornerRadius: Tokens.Radius.control)
                            .fill(canSendFallback ? Tokens.Color.accent : Tokens.Color.accent.opacity(0.4))
                    )
            }
            .buttonStyle(.plain)
            .disabled(!canSendFallback)
            .keyboardShortcut(.return, modifiers: [])
        }
        .padding(Tokens.Space.m)
        .background(Tokens.Color.bgPrimary)
    }

    private var canSendFallback: Bool {
        !appState.isBusy && !fallbackText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func sendFallback() {
        let text = fallbackText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        fallbackText = ""
        Task { await sendInput(text) }
    }

    @MainActor
    private func sendInput(_ text: String) async {
        lastSentText = text
        appState.isBusy = true
        appState.lastError = nil
        defer { appState.isBusy = false }
        do {
            let decision = try await appState.client.route(input: text)
            appState.lastDecision = decision
        } catch {
            appState.lastError = error.localizedDescription
        }
    }
}

// MARK: - Decision card (re-used from old ComposeView)

struct DecisionCard: View {
    let decision: RouteDecision

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            HStack(spacing: Tokens.Space.s) {
                Text(decision.agentType.capitalized)
                    .font(Tokens.Font.h2)
                    .foregroundStyle(Tokens.Color.fgPrimary)
                StatusPill(label: decision.primaryIntent, kind: .running)
                Spacer()
                Text("conf \(String(format: "%.2f", decision.confidence))")
                    .font(Tokens.Font.caption)
                    .foregroundStyle(Tokens.Color.fgMuted)
            }

            if !decision.routerNotes.isEmpty {
                Text(decision.routerNotes)
                    .font(Tokens.Font.body)
                    .foregroundStyle(Tokens.Color.fgSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack(spacing: Tokens.Space.l) {
                meta(label: "domain", value: decision.domain)
                if !decision.topic.isEmpty {
                    meta(label: "topic", value: decision.topic)
                }
                meta(label: "trace", value: String(decision.traceId.prefix(12)))
            }
        }
        .padding(Tokens.Space.l)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: Tokens.Radius.card)
                .fill(Tokens.Color.bgPrimary)
        )
        .overlay(
            RoundedRectangle(cornerRadius: Tokens.Radius.card)
                .strokeBorder(Tokens.Color.border, lineWidth: 1)
        )
    }

    private func meta(label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(Tokens.Font.caption)
                .foregroundStyle(Tokens.Color.fgMuted)
            Text(value)
                .font(Tokens.Font.code)
                .foregroundStyle(Tokens.Color.fgPrimary)
        }
    }
}
