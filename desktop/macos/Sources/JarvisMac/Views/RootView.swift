import SwiftUI

/// Top-level container. m1 is intentionally simple — two tabs in a
/// single window. m2 will replace this with a three-pane NSSplitView
/// per the visual design doc.
struct RootView: View {
    @EnvironmentObject var appState: AppState
    @EnvironmentObject var daemon: DaemonSupervisor
    @State private var selection: Tab = .compose

    enum Tab: String, CaseIterable, Hashable {
        case compose = "Compose"
        case memory  = "Memory"
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().background(Tokens.Color.border)
            content
        }
        .background(Tokens.Color.bgPrimary.ignoresSafeArea())
    }

    private var header: some View {
        HStack(spacing: Tokens.Space.l) {
            Text("Jarvis")
                .font(Tokens.Font.h1)
                .foregroundStyle(Tokens.Color.fgPrimary)
            DaemonStatusIndicator()
            Spacer()
            ForEach(Tab.allCases, id: \.self) { tab in
                tabButton(tab)
            }
        }
        .padding(.horizontal, Tokens.Space.xl)
        .padding(.vertical, Tokens.Space.m)
        .frame(maxWidth: .infinity)
        .background(Tokens.Color.bgPrimary)
    }

    private func tabButton(_ tab: Tab) -> some View {
        let active = selection == tab
        return Button {
            selection = tab
        } label: {
            Text(tab.rawValue)
                .font(Tokens.Font.bodyTight)
                .foregroundStyle(active ? Tokens.Color.accent : Tokens.Color.fgSecondary)
                .padding(.horizontal, Tokens.Space.m)
                .padding(.vertical, Tokens.Space.xs + 2)
                .background(
                    RoundedRectangle(cornerRadius: Tokens.Radius.control)
                        .fill(active ? Tokens.Color.accentMuted : Color.clear)
                )
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var content: some View {
        switch selection {
        case .compose:
            ComposeView()
        case .memory:
            MemoryView()
        }
    }
}
