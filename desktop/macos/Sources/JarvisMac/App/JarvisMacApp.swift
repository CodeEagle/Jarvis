import SwiftUI

@main
struct JarvisMacApp: App {
    @StateObject private var appState = AppState()

    var body: some Scene {
        WindowGroup("Jarvis") {
            RootView()
                .environmentObject(appState)
                .frame(minWidth: 720, minHeight: 480)
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

/// Single source of truth shared across views. m1 keeps it minimal —
/// just the JarvisClient instance and recent decision/memory state.
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
