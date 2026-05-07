import SwiftUI

/// Compact status pill that mirrors `DaemonSupervisor.state`.
/// Shows "running" (we own the process) / "external" (someone else's
/// daemon answered first) / "starting…" / error.
struct DaemonStatusIndicator: View {
    @EnvironmentObject var daemon: DaemonSupervisor

    var body: some View {
        HStack(spacing: Tokens.Space.xs) {
            Circle()
                .fill(dotColor)
                .frame(width: 6, height: 6)
            Text(label)
                .font(Tokens.Font.caption)
                .foregroundStyle(textColor)
        }
        .padding(.horizontal, Tokens.Space.s)
        .frame(height: 20)
        .background(
            Capsule().fill(textColor.opacity(0.10))
        )
        .help(tooltip)
    }

    private var label: String {
        switch daemon.state {
        case .idle:        return "off"
        case .probing:     return "probing"
        case .spawning:    return "starting…"
        case .running:     return "running"
        case .external:    return "attached"
        case .failed:      return "failed"
        }
    }

    private var dotColor: Color {
        switch daemon.state {
        case .running, .external: return Tokens.Color.statusSuccess
        case .probing, .spawning: return Tokens.Color.statusRunning
        case .failed:             return Tokens.Color.statusError
        case .idle:               return Tokens.Color.fgMuted
        }
    }

    private var textColor: Color {
        switch daemon.state {
        case .running, .external: return Tokens.Color.statusSuccess
        case .probing, .spawning: return Tokens.Color.statusRunning
        case .failed:             return Tokens.Color.statusError
        case .idle:               return Tokens.Color.fgMuted
        }
    }

    private var tooltip: String {
        switch daemon.state {
        case .idle:           return "Daemon not started"
        case .probing:        return "Probing 127.0.0.1:7777"
        case .spawning:       return "Spawning embedded jarvis serve"
        case .running:        return "Embedded daemon running (this app spawned it)"
        case .external:       return "Attached to an existing daemon on 127.0.0.1:7777"
        case .failed(let msg): return "Daemon failed: \(msg)"
        }
    }
}
