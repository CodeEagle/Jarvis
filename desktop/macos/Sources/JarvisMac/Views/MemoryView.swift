import SwiftUI

struct MemoryView: View {
    @EnvironmentObject var appState: AppState
    @State private var loadError: String?
    @State private var loading: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.l) {
            HStack {
                Text("Memory · global")
                    .font(Tokens.Font.h1)
                    .foregroundStyle(Tokens.Color.fgPrimary)
                Spacer()
                Button(action: refresh) {
                    HStack(spacing: Tokens.Space.xs) {
                        if loading {
                            ProgressView().controlSize(.small)
                        } else {
                            Image(systemName: "arrow.clockwise")
                        }
                        Text(loading ? "Loading…" : "Refresh")
                    }
                    .font(Tokens.Font.bodyTight)
                    .foregroundStyle(Tokens.Color.fgPrimary)
                    .padding(.horizontal, Tokens.Space.m)
                    .frame(height: 28)
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
                .disabled(loading)
            }

            if let err = loadError {
                Text(err)
                    .font(Tokens.Font.bodyTight)
                    .foregroundStyle(Tokens.Color.statusError)
            }

            if appState.memories.isEmpty && !loading {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: Tokens.Space.m) {
                        ForEach(appState.memories) { mem in
                            MemoryRow(item: mem)
                        }
                    }
                    .padding(.bottom, Tokens.Space.xl)
                }
            }
        }
        .padding(Tokens.Space.xl)
        .background(Tokens.Color.bgPrimary)
        .task { await load() }
    }

    private var emptyState: some View {
        VStack(spacing: Tokens.Space.m) {
            Spacer(minLength: Tokens.Space.xl)
            Text("没有记忆")
                .font(Tokens.Font.h2)
                .foregroundStyle(Tokens.Color.fgPrimary)
            Text("用 `jarvis memory write <text>` 写入第一条，或者在 Compose 里说\"记住……\"。")
                .font(Tokens.Font.body)
                .foregroundStyle(Tokens.Color.fgMuted)
                .multilineTextAlignment(.center)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private func refresh() {
        Task { await load() }
    }

    @MainActor
    private func load() async {
        loading = true
        loadError = nil
        defer { loading = false }
        do {
            let items = try await appState.client.memories(scope: "global")
            appState.memories = items
        } catch {
            loadError = error.localizedDescription
        }
    }
}

private struct MemoryRow: View {
    let item: MemoryItem

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.s) {
            Text(item.content)
                .font(Tokens.Font.h2)
                .foregroundStyle(Tokens.Color.fgPrimary)
                .fixedSize(horizontal: false, vertical: true)

            HStack(spacing: Tokens.Space.m) {
                if let t = item.type {
                    Text(t).font(Tokens.Font.caption).foregroundStyle(Tokens.Color.fgMuted)
                }
                if let s = item.trustScore {
                    Text(String(format: "trust %.2f", s))
                        .font(Tokens.Font.caption)
                        .foregroundStyle(Tokens.Color.fgMuted)
                }
                if let tier = item.tier {
                    Text("tier \(tier)")
                        .font(Tokens.Font.caption)
                        .foregroundStyle(Tokens.Color.fgMuted)
                }
                Spacer()
                Text(String(item.id.prefix(16)))
                    .font(Tokens.Font.code)
                    .foregroundStyle(Tokens.Color.fgMuted)
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
}
