import SwiftUI

/// Provider + model + API key configuration. Replaces the need for
/// `jarvis model set / login` in a terminal.
struct SettingsView: View {
    @EnvironmentObject var modelConfig: ModelConfigService
    @EnvironmentObject var secrets: SecretsStore
    @EnvironmentObject var daemon: DaemonSupervisor

    @State private var modelInput: String = ""
    @State private var apiKeyDraft: String = ""
    @State private var apiKeyEnvDraft: String = "ANTHROPIC_API_KEY"
    @State private var saveError: String?
    @State private var isRestarting: Bool = false

    /// Common preset models — picking one fills the model field and
    /// suggests the right env var. User can still type any custom id.
    private static let presets: [(label: String, id: String, env: String?)] = [
        ("Claude Sonnet 4.6 (API key)",  "anthropic/claude-sonnet-4-6", "ANTHROPIC_API_KEY"),
        ("Claude Opus 4.7 (API key)",    "anthropic/claude-opus-4-7",   "ANTHROPIC_API_KEY"),
        ("GPT-4o mini (API key)",        "openai/gpt-4o-mini",          "OPENAI_API_KEY"),
        ("Gemini 2.5 Flash (API key)",   "gemini/gemini-2.5-flash",     "GEMINI_API_KEY"),
        ("Claude via `claude` CLI (OAuth)", "claude-cli/sonnet",        nil),
        ("Codex CLI (OAuth, judge only)", "codex/local",                nil),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Tokens.Space.xl) {
                Text("Settings")
                    .font(Tokens.Font.h1)
                    .foregroundStyle(Tokens.Color.fgPrimary)

                modelSection
                apiKeySection
                providersSection

                Spacer(minLength: Tokens.Space.xxl)
            }
            .padding(Tokens.Space.xl)
            .frame(maxWidth: 720, alignment: .leading)
        }
        .background(Tokens.Color.bgPrimary)
        .task {
            await modelConfig.refresh()
            modelInput = modelConfig.snapshot?.defaultModel ?? ""
        }
    }

    // MARK: - Model

    private var modelSection: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            Text("Default model")
                .font(Tokens.Font.h2)
                .foregroundStyle(Tokens.Color.fgPrimary)
            Text("Format: `provider/model`. Pick a preset to autofill, or type any id.")
                .font(Tokens.Font.bodyTight)
                .foregroundStyle(Tokens.Color.fgMuted)

            HStack(spacing: Tokens.Space.s) {
                Menu {
                    ForEach(Self.presets, id: \.id) { preset in
                        Button(preset.label) {
                            modelInput = preset.id
                            if let env = preset.env {
                                apiKeyEnvDraft = env
                            }
                        }
                    }
                } label: {
                    HStack(spacing: Tokens.Space.xs) {
                        Image(systemName: "list.bullet")
                        Text("Presets")
                    }
                    .font(Tokens.Font.bodyTight)
                    .foregroundStyle(Tokens.Color.fgPrimary)
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
                }
                .menuStyle(.borderlessButton)
                .fixedSize()

                TextField("anthropic/claude-sonnet-4-6", text: $modelInput)
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

                Button(action: saveModel) {
                    Text(isRestarting ? "Saving…" : "Save")
                        .font(Tokens.Font.body)
                        .foregroundStyle(.white)
                        .padding(.horizontal, Tokens.Space.l)
                        .frame(height: 32)
                        .background(
                            RoundedRectangle(cornerRadius: Tokens.Radius.control)
                                .fill(canSave ? Tokens.Color.accent : Tokens.Color.accent.opacity(0.4))
                        )
                }
                .buttonStyle(.plain)
                .disabled(!canSave || isRestarting)
            }

            if let cur = modelConfig.snapshot?.defaultModel {
                Text("Currently in config: \(cur)")
                    .font(Tokens.Font.caption)
                    .foregroundStyle(Tokens.Color.fgMuted)
            }
            if let err = modelConfig.lastError ?? saveError {
                Text(err)
                    .font(Tokens.Font.bodyTight)
                    .foregroundStyle(Tokens.Color.statusError)
            }
        }
    }

    // MARK: - API key

    private var apiKeySection: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            Text("API key")
                .font(Tokens.Font.h2)
                .foregroundStyle(Tokens.Color.fgPrimary)
            Text("Stored at \(secrets.fileURL.path) (mode 0600). The daemon reads it as the env var below.")
                .font(Tokens.Font.bodyTight)
                .foregroundStyle(Tokens.Color.fgMuted)

            HStack(spacing: Tokens.Space.s) {
                TextField("ANTHROPIC_API_KEY", text: $apiKeyEnvDraft)
                    .textFieldStyle(.plain)
                    .font(Tokens.Font.code)
                    .frame(width: 220)
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

                SecureField("sk-ant-…", text: $apiKeyDraft)
                    .textFieldStyle(.plain)
                    .font(Tokens.Font.code)
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

                Button(action: saveSecret) {
                    Text("Save key")
                        .font(Tokens.Font.body)
                        .foregroundStyle(Tokens.Color.fgPrimary)
                        .padding(.horizontal, Tokens.Space.l)
                        .frame(height: 32)
                        .background(
                            RoundedRectangle(cornerRadius: Tokens.Radius.control)
                                .fill(Tokens.Color.bgSecondary)
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: Tokens.Radius.control)
                                .strokeBorder(Tokens.Color.border, lineWidth: 1)
                        )
                }
                .buttonStyle(.plain)
                .disabled(apiKeyDraft.isEmpty || apiKeyEnvDraft.isEmpty || isRestarting)
            }

            keyStatus
        }
    }

    private var keyStatus: some View {
        let hasKey = secrets.has(apiKeyEnvDraft)
        return HStack(spacing: Tokens.Space.xs) {
            Circle()
                .fill(hasKey ? Tokens.Color.statusSuccess : Tokens.Color.fgMuted)
                .frame(width: 6, height: 6)
            Text(hasKey
                 ? "\(apiKeyEnvDraft) is set."
                 : "\(apiKeyEnvDraft) is empty.")
                .font(Tokens.Font.caption)
                .foregroundStyle(Tokens.Color.fgMuted)
            if hasKey {
                Button("Clear") {
                    secrets.clear(apiKeyEnvDraft)
                    Task { await daemon.restart() }
                }
                .buttonStyle(.plain)
                .font(Tokens.Font.caption)
                .foregroundStyle(Tokens.Color.statusError)
            }
        }
    }

    // MARK: - Providers list

    private var providersSection: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            Text("Providers")
                .font(Tokens.Font.h2)
                .foregroundStyle(Tokens.Color.fgPrimary)
            Text("Reflects `\(modelConfig.snapshot?.configPath ?? "~/.jarvis/config.toml")`.")
                .font(Tokens.Font.bodyTight)
                .foregroundStyle(Tokens.Color.fgMuted)

            if let providers = modelConfig.snapshot?.providers, !providers.isEmpty {
                VStack(alignment: .leading, spacing: Tokens.Space.s) {
                    ForEach(providers) { p in providerRow(p) }
                }
            } else {
                Text("(no providers in config yet — pick a preset above and Save to record one)")
                    .font(Tokens.Font.bodyTight)
                    .foregroundStyle(Tokens.Color.fgMuted)
            }
        }
    }

    private func providerRow(_ p: ProviderInfo) -> some View {
        HStack(spacing: Tokens.Space.m) {
            Circle()
                .fill(p.authed ? Tokens.Color.statusSuccess : Tokens.Color.statusError)
                .frame(width: 8, height: 8)
            Text(p.name).font(Tokens.Font.body).foregroundStyle(Tokens.Color.fgPrimary)
            if let kind = p.oauthKind {
                Text(kind == "device_flow" ? "oauth (device flow)"
                     : kind == "cli_subprocess" ? "oauth via CLI"
                     : kind)
                    .font(Tokens.Font.caption)
                    .foregroundStyle(Tokens.Color.fgMuted)
            } else if let env = p.apiKeyEnv {
                Text("env=\(env)")
                    .font(Tokens.Font.code)
                    .foregroundStyle(Tokens.Color.fgMuted)
            }
            Spacer()
            Text(p.authed ? "ready" : "not authed")
                .font(Tokens.Font.caption)
                .foregroundStyle(p.authed ? Tokens.Color.statusSuccess : Tokens.Color.statusError)
        }
        .padding(.horizontal, Tokens.Space.m)
        .padding(.vertical, Tokens.Space.s)
        .background(
            RoundedRectangle(cornerRadius: Tokens.Radius.control)
                .fill(Tokens.Color.bgSecondary)
        )
    }

    // MARK: - Actions

    private var canSave: Bool {
        let trimmed = modelInput.trimmingCharacters(in: .whitespaces)
        return trimmed.contains("/") && !trimmed.hasPrefix("/") && !trimmed.hasSuffix("/")
    }

    private func saveModel() {
        let id = modelInput.trimmingCharacters(in: .whitespaces)
        Task { @MainActor in
            isRestarting = true
            saveError = nil
            defer { isRestarting = false }
            do {
                try await modelConfig.setDefault(modelId: id)
                await daemon.restart()
            } catch {
                saveError = error.localizedDescription
            }
        }
    }

    private func saveSecret() {
        let env = apiKeyEnvDraft.trimmingCharacters(in: .whitespaces)
        let key = apiKeyDraft.trimmingCharacters(in: .whitespaces)
        guard !env.isEmpty, !key.isEmpty else { return }
        secrets.set(env, key)
        apiKeyDraft = ""
        Task { await daemon.restart() }
    }
}
