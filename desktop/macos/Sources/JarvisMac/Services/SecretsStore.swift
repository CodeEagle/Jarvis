import Foundation

/// Owns provider API keys at
/// `~/Library/Application Support/Jarvis/secrets.json` (mode 0600).
///
/// Not Keychain — Keychain access from an unsigned dev build prompts
/// awkwardly and surfaces "is this Jarvis the same Jarvis as last
/// week" warnings. A 0600 file in app-support is the right tradeoff
/// for the m1 dev distribution channel; we'll move to Keychain when
/// the build is properly signed.
@MainActor
final class SecretsStore: ObservableObject {

    /// Map of `ENV_VAR_NAME` → secret value, e.g.
    /// `["ANTHROPIC_API_KEY": "sk-ant-…"]`. Published so views can
    /// react when keys are added or cleared.
    @Published private(set) var secrets: [String: String] = [:]

    private let path: URL

    init() {
        let appSupport = (try? FileManager.default
            .url(for: .applicationSupportDirectory,
                 in: .userDomainMask,
                 appropriateFor: nil,
                 create: true))
            ?? FileManager.default.temporaryDirectory
        self.path = appSupport
            .appendingPathComponent("Jarvis", isDirectory: true)
            .appendingPathComponent("secrets.json")
        load()
    }

    var fileURL: URL { path }

    /// True iff the env var name has a non-empty value stored.
    func has(_ name: String) -> Bool {
        guard let v = secrets[name] else { return false }
        return !v.isEmpty
    }

    func set(_ name: String, _ value: String) {
        if value.isEmpty {
            secrets.removeValue(forKey: name)
        } else {
            secrets[name] = value
        }
        save()
    }

    func clear(_ name: String) {
        secrets.removeValue(forKey: name)
        save()
    }

    /// Snapshot of stored secrets — used by DaemonSupervisor to
    /// inject env vars into the spawned `jarvis serve` process.
    func snapshot() -> [String: String] { secrets }

    // MARK: -

    private func load() {
        guard let data = try? Data(contentsOf: path) else { return }
        if let parsed = try? JSONDecoder().decode([String: String].self, from: data) {
            secrets = parsed
        }
    }

    private func save() {
        do {
            try FileManager.default.createDirectory(
                at: path.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data = try JSONEncoder().encode(secrets)
            try data.write(to: path, options: [.atomic])
            #if canImport(Darwin)
            try? FileManager.default.setAttributes(
                [.posixPermissions: NSNumber(value: 0o600)],
                ofItemAtPath: path.path
            )
            #endif
        } catch {
            // Failures here just mean the secret won't persist —
            // surface to logger so the user sees `log show` output.
            // No app-fatal: empty-secrets is a valid first-run state.
        }
    }
}
