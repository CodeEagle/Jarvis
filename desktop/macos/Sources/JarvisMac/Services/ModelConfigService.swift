import Foundation
import os.log

/// Reads + writes provider/model config by shelling out to the
/// bundled `jarvis-cli` (`Contents/Resources/jarvis-cli`). This keeps
/// the source of truth in ONE place — `~/.jarvis/config.toml` — so
/// the GUI and the CLI never disagree.
@MainActor
final class ModelConfigService: ObservableObject {

    @Published private(set) var snapshot: ModelListPayload?
    @Published private(set) var isLoading: Bool = false
    @Published private(set) var lastError: String?

    private let logger = Logger(subsystem: "ai.jarvis.mac", category: "model-config")

    private var binaryURL: URL {
        Bundle.main.bundleURL
            .appendingPathComponent("Contents/Resources/jarvis-cli")
    }

    func refresh() async {
        isLoading = true
        defer { isLoading = false }
        do {
            let json = try await run(args: ["model", "list", "--json"])
            let payload = try JSONDecoder().decode(
                ModelListPayload.self, from: Data(json.utf8)
            )
            self.snapshot = payload
            self.lastError = nil
        } catch {
            self.lastError = error.localizedDescription
            logger.error("model list failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// Set the default model id (`provider/model`). Daemon picks it
    /// up after a restart.
    func setDefault(modelId: String) async throws {
        _ = try await run(args: ["model", "set", modelId])
        await refresh()
    }

    // MARK: -

    private func run(args: [String]) async throws -> String {
        let url = binaryURL
        guard FileManager.default.isExecutableFile(atPath: url.path) else {
            throw ModelConfigError.binaryMissing(url.path)
        }
        let p = Process()
        p.executableURL = url
        p.arguments = args
        let stdout = Pipe()
        let stderr = Pipe()
        p.standardOutput = stdout
        p.standardError = stderr
        p.standardInput = FileHandle.nullDevice
        try p.run()
        p.waitUntilExit()
        let outData = (try? stdout.fileHandleForReading.readToEnd()) ?? Data()
        let errData = (try? stderr.fileHandleForReading.readToEnd()) ?? Data()
        if p.terminationStatus != 0 {
            let stderrStr = String(data: errData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            throw ModelConfigError.nonZeroExit(
                code: Int(p.terminationStatus),
                stderr: stderrStr
            )
        }
        return String(data: outData, encoding: .utf8) ?? ""
    }
}

enum ModelConfigError: LocalizedError {
    case binaryMissing(String)
    case nonZeroExit(code: Int, stderr: String)

    var errorDescription: String? {
        switch self {
        case .binaryMissing(let p):
            return "embedded jarvis-cli missing at \(p)"
        case .nonZeroExit(let code, let stderr):
            return stderr.isEmpty
                ? "jarvis-cli exited with code \(code)"
                : "jarvis-cli exited \(code): \(stderr)"
        }
    }
}
