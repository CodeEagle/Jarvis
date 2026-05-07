import SwiftUI

struct ComposeView: View {
    @EnvironmentObject var appState: AppState
    @State private var input: String = ""
    @FocusState private var inputFocused: Bool

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Tokens.Space.l) {
                Text("Compose")
                    .font(Tokens.Font.h1)
                    .foregroundStyle(Tokens.Color.fgPrimary)

                composer

                if let err = appState.lastError {
                    errorBanner(err)
                }

                if let decision = appState.lastDecision {
                    DecisionCard(decision: decision)
                }

                Spacer(minLength: Tokens.Space.xxl)
            }
            .padding(Tokens.Space.xl)
        }
        .background(Tokens.Color.bgPrimary)
    }

    private var composer: some View {
        VStack(alignment: .trailing, spacing: Tokens.Space.s) {
            TextEditor(text: $input)
                .font(Tokens.Font.body)
                .scrollContentBackground(.hidden)
                .padding(Tokens.Space.m)
                .frame(minHeight: 80, maxHeight: 200)
                .background(
                    RoundedRectangle(cornerRadius: Tokens.Radius.control)
                        .fill(Tokens.Color.bgSecondary)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: Tokens.Radius.control)
                        .strokeBorder(
                            inputFocused ? Tokens.Color.accent.opacity(0.6) : Tokens.Color.border,
                            lineWidth: 1
                        )
                )
                .focused($inputFocused)
                .overlay(alignment: .topLeading) {
                    if input.isEmpty {
                        Text("和 Jarvis 说点什么……")
                            .font(Tokens.Font.body)
                            .foregroundStyle(Tokens.Color.fgMuted)
                            .padding(.horizontal, Tokens.Space.m + 4)
                            .padding(.vertical, Tokens.Space.m + 6)
                            .allowsHitTesting(false)
                    }
                }

            HStack(spacing: Tokens.Space.s) {
                Spacer()
                Text("⌘↩ 发送")
                    .font(Tokens.Font.caption)
                    .foregroundStyle(Tokens.Color.fgMuted)
                Button(action: send) {
                    HStack(spacing: Tokens.Space.xs) {
                        if appState.isBusy {
                            ProgressView()
                                .controlSize(.small)
                                .tint(.white)
                        }
                        Text(appState.isBusy ? "Routing…" : "Send")
                    }
                    .font(Tokens.Font.body)
                    .foregroundStyle(.white)
                    .padding(.horizontal, Tokens.Space.l)
                    .frame(height: 32)
                    .background(
                        RoundedRectangle(cornerRadius: Tokens.Radius.control)
                            .fill(canSend ? Tokens.Color.accent : Tokens.Color.accent.opacity(0.4))
                    )
                }
                .buttonStyle(.plain)
                .disabled(!canSend)
                .keyboardShortcut(.return, modifiers: .command)
            }
        }
    }

    private var canSend: Bool {
        !appState.isBusy && !input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
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
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: Tokens.Radius.control)
                .fill(Tokens.Color.statusError.opacity(0.08))
        )
        .overlay(
            RoundedRectangle(cornerRadius: Tokens.Radius.control)
                .strokeBorder(Tokens.Color.statusError.opacity(0.25), lineWidth: 1)
        )
    }

    private func send() {
        guard canSend else { return }
        let text = input
        Task { @MainActor in
            appState.isBusy = true
            appState.lastError = nil
            defer { appState.isBusy = false }
            do {
                let decision = try await appState.client.route(input: text)
                appState.lastDecision = decision
                input = ""
            } catch {
                appState.lastError = error.localizedDescription
            }
        }
    }
}

private struct DecisionCard: View {
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

            Text(decision.routerNotes)
                .font(Tokens.Font.body)
                .foregroundStyle(Tokens.Color.fgSecondary)
                .fixedSize(horizontal: false, vertical: true)

            HStack(spacing: Tokens.Space.l) {
                meta(label: "domain",  value: decision.domain)
                meta(label: "topic",   value: decision.topic)
                meta(label: "trace",   value: String(decision.traceId.prefix(12)))
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
