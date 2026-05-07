import SwiftUI
import AppKit

@main
struct JarvisMacApp: App {
    @StateObject private var appState = AppState()
    @StateObject private var daemon = DaemonSupervisor()

    var body: some Scene {
        WindowGroup("Jarvis") {
            RootView()
                .environmentObject(appState)
                .environmentObject(daemon)
                .frame(minWidth: 720, minHeight: 480)
                .task {
                    // 1. Spawn / attach the embedded daemon as soon as
                    //    the first window appears.
                    await daemon.start()
                }
                .task {
                    // 2. Mirror NSApplication.willTerminate into a
                    //    structured shutdown of the daemon. macOS will
                    //    SIGKILL the child anyway, but a SIGTERM gives
                    //    SQLite a chance to flush WAL.
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
