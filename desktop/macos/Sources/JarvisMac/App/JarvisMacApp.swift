import SwiftUI
import AppKit

@main
struct JarvisMacApp: App {
    @StateObject private var appState     = AppState()
    @StateObject private var daemon       = DaemonSupervisor()
    @StateObject private var secrets      = SecretsStore()
    @StateObject private var modelConfig  = ModelConfigService()
    @StateObject private var speech       = SpeechCoordinator()

    init() {
        // No-op; .task on the WindowGroup wires daemon ↔ secrets so
        // every spawned `jarvis serve` inherits user-provided env
        // vars (API keys, etc.).
    }

    var body: some Scene {
        WindowGroup("Jarvis") {
            RootView()
                .environmentObject(appState)
                .environmentObject(daemon)
                .environmentObject(secrets)
                .environmentObject(modelConfig)
                .environmentObject(speech)
                .frame(minWidth: 720, minHeight: 480)
                .task {
                    // Wire SecretsStore → DaemonSupervisor before
                    // start(), so the very first spawn already sees
                    // user-saved API keys. The closure captures
                    // `secrets` weakly via @MainActor isolation.
                    daemon.secretsProvider = { [weak secrets] in
                        secrets?.snapshot() ?? [:]
                    }
                    await daemon.start()
                }
                .task {
                    // SIGTERM the daemon on app quit so SQLite WAL
                    // can flush. macOS would SIGKILL anyway but TERM
                    // is courteous.
                    let center = NotificationCenter.default
                    let stream = center.notifications(
                        named: NSApplication.willTerminateNotification
                    )
                    for await _ in stream {
                        daemon.stop()
                        break
                    }
                }
        }
        .windowStyle(.titleBar)
        .windowToolbarStyle(.unified)
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button("About Jarvis") {
                    NSApplication.shared.orderFrontStandardAboutPanel(nil)
                }
            }
        }
    }
}

/// Single source of truth shared across views.
@MainActor
final class AppState: ObservableObject {
    let client: JarvisClient

    @Published var lastDecision: RouteDecision?
    @Published var memories: [MemoryItem] = []
    @Published var lastError: String?
    @Published var isBusy: Bool = false

    init(baseURL: URL = URL(string: "http://127.0.0.1:7777")!) {
        self.client = JarvisClient(baseURL: baseURL)
    }
}
