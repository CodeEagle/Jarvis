import Foundation
import os.log

/// Owns the embedded `jarvis serve` subprocess.
///
/// On launch:
///   1. Probe `GET /healthz` on `baseURL` — if something already
///      responds, attach (don't spawn a duplicate).
///   2. Otherwise spawn `Contents/MacOS/jarvis serve` from inside the
///      app bundle, with `JARVIS_DB` pointed at
///      `~/Library/Application Support/Jarvis/jarvis.db`.
///   3. Poll `/healthz` until it responds (≤ 6 s) or give up.
///
/// On terminate: SIGTERM the child; macOS will SIGKILL on app exit
/// regardless, but a clean SIGTERM gives the daemon a chance to flush.
@MainActor
final class DaemonSupervisor: ObservableObject {

    enum State: Equatable {
        case idle
        case probing
        case spawning
        case running        // we own the process
        case external       // someone else is on 7777
        case failed(String)
    }

    @Published var state: State = .idle

    private var child: Process?
    private var stderrTask: Task<Void, Never>?
    private let baseURL: URL
    private let logger = Logger(subsystem: "ai.jarvis.mac", category: "daemon")

    /// Optional secrets source — when set, the supervisor reads it
    /// at spawn time and injects every entry as an env var into the
    /// child `jarvis serve` process. Kept weak-via-closure so the
    /// store's lifecycle doesn't depend on us.
    var secretsProvider: (() -> [String: String])?

    init(baseURL: URL = URL(string: "http://127.0.0.1:7777")!) {
        self.baseURL = baseURL
    }

    /// Detect existing daemon → attach; otherwise spawn the bundled
    /// binary and wait for `/healthz` to come up.
    func start() async {
        state = .probing
        if await probeHealth() {
            state = .external
            logger.info("attached to existing daemon at \(self.baseURL.absoluteString, privacy: .public)")
            return
        }
        do {
            try await spawn()
        } catch {
            state = .failed("spawn failed: \(error.localizedDescription)")
            logger.error("spawn failed: \(error.localizedDescription, privacy: .public)")
            return
        }
        // Wait up to 6 s for the daemon to bind 7777.
        for _ in 0..<30 {
            try? await Task.sleep(nanoseconds: 200_000_000)
            if await probeHealth() {
                state = .running
                logger.info("spawned daemon healthy")
                return
            }
        }
        state = .failed("daemon spawned but never reached /healthz")
    }

    /// Send SIGTERM if we own the process. Idempotent.
    func stop() {
        guard let p = child else { return }
        if p.isRunning { p.terminate() }
        child = nil
        stderrTask?.cancel()
        stderrTask = nil
        state = .idle
    }

    /// stop() + start() — used by SettingsView when the user changes
    /// the model or pastes a new API key.
    func restart() async {
        stop()
        // Brief pause so the previous process actually releases 7777.
        try? await Task.sleep(nanoseconds: 600_000_000)
        await start()
    }

    // MARK: -

    private func probeHealth() async -> Bool {
        let url = baseURL.appendingPathComponent("healthz")
        var req = URLRequest(url: url)
        req.timeoutInterval = 1
        do {
            let (_, resp) = try await URLSession.shared.data(for: req)
            return (resp as? HTTPURLResponse)?.statusCode == 200
        } catch {
            return false
        }
    }

    private func spawn() async throws {
        state = .spawning

        // Bundled Rust CLI lives in Contents/Resources/, NOT
        // Contents/MacOS/ — the .app's main executable is named
        // `Jarvis` (CFBundleExecutable) and macOS's case-insensitive
        // APFS would collide a Contents/MacOS/jarvis file with the
        // SwiftUI binary, clobbering it during the `cp` in CI.
        let binURL = Bundle.main.bundleURL
            .appendingPathComponent("Contents/Resources/jarvis-cli")
        guard FileManager.default.isExecutableFile(atPath: binURL.path) else {
            throw NSError(
                domain: "DaemonSupervisor", code: 1,
                userInfo: [NSLocalizedDescriptionKey:
                    "bundled jarvis binary missing at \(binURL.path)"]
            )
        }

        // Canonical app-support DB location — shared with any future
        // user-spawned `jarvis serve` once we change the CLI default.
        let appSupport = try FileManager.default
            .url(for: .applicationSupportDirectory,
                 in: .userDomainMask,
                 appropriateFor: nil,
                 create: true)
            .appendingPathComponent("Jarvis", isDirectory: true)
        try FileManager.default.createDirectory(
            at: appSupport, withIntermediateDirectories: true)
        let dbURL = appSupport.appendingPathComponent("jarvis.db")

        let p = Process()
        p.executableURL = binURL
        p.arguments = ["serve"]
        var env = ProcessInfo.processInfo.environment
        env["JARVIS_DB"] = dbURL.path
        // App-support is also where logs / config live; let the CLI
        // default JARVIS_LOG to info if the user hasn't overridden.
        env["JARVIS_LOG"] = env["JARVIS_LOG"] ?? "info"
        // Inject any user-provided secrets (API keys, etc.) so the
        // daemon can authenticate against upstream providers without
        // the user ever seeing a terminal.
        if let provided = secretsProvider?() {
            for (k, v) in provided {
                env[k] = v
            }
        }
        p.environment = env
        p.currentDirectoryURL = appSupport

        let stderr = Pipe()
        p.standardError = stderr
        p.standardOutput = FileHandle.nullDevice

        try p.run()
        child = p

        // Async-drain stderr to the unified log so `log show` / Console
        // can find diagnostics.
        stderrTask = Task.detached { [logger, stderr] in
            do {
                for try await line in stderr.fileHandleForReading.bytes.lines {
                    logger.info("[serve] \(line, privacy: .public)")
                }
            } catch {
                // Pipe closed when child exits — expected.
            }
        }
    }
}
