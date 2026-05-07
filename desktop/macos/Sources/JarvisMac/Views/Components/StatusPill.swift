import SwiftUI

/// Capsule status indicator. Mirror of `docs/design/macos-visual.md`
/// §2 StatusPill — bg = status @ 12% opacity, text = status.
struct StatusPill: View {
    enum Kind {
        case success, warning, error, running, pending

        var color: Color {
            switch self {
            case .success: return Tokens.Color.statusSuccess
            case .warning: return Tokens.Color.statusWarning
            case .error:   return Tokens.Color.statusError
            case .running: return Tokens.Color.statusRunning
            case .pending: return Tokens.Color.fgMuted
            }
        }
    }

    let label: String
    let kind: Kind

    var body: some View {
        Text(label)
            .font(Tokens.Font.caption)
            .foregroundStyle(kind.color)
            .padding(.horizontal, Tokens.Space.s)
            .frame(height: 20)
            .background(
                Capsule().fill(kind.color.opacity(0.12))
            )
    }
}
