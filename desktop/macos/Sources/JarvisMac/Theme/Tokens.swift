import SwiftUI

/// Design tokens. Mirrors `docs/design/macos-visual.md` §1. All
/// component code references these — never hard-code colors / fonts /
/// spacing in views.
enum Tokens {

    // ── Color (Light values; auto-darkening relies on Asset catalog
    // in later milestones — m1 ships the light palette only). ────────
    enum Color {
        static let bgPrimary   = SwiftUI.Color(red: 0xFA/255, green: 0xF9/255, blue: 0xF5/255)
        static let bgSecondary = SwiftUI.Color(red: 0xF5/255, green: 0xF4/255, blue: 0xEE/255)
        static let bgTertiary  = SwiftUI.Color(red: 0xEF/255, green: 0xED/255, blue: 0xE5/255)
        static let fgPrimary   = SwiftUI.Color(red: 0x1A/255, green: 0x18/255, blue: 0x17/255)
        static let fgSecondary = SwiftUI.Color(red: 0x4A/255, green: 0x45/255, blue: 0x3E/255)
        static let fgMuted     = SwiftUI.Color(red: 0x6E/255, green: 0x6A/255, blue: 0x60/255)
        static let border      = SwiftUI.Color(red: 0x1A/255, green: 0x18/255, blue: 0x17/255, opacity: 0.08)
        static let borderStrong = SwiftUI.Color(red: 0x1A/255, green: 0x18/255, blue: 0x17/255, opacity: 0.16)
        static let accent      = SwiftUI.Color(red: 0xD9/255, green: 0x77/255, blue: 0x57/255)
        static let accentHover = SwiftUI.Color(red: 0xC5/255, green: 0x67/255, blue: 0x45/255)
        static let accentMuted = SwiftUI.Color(red: 0xD9/255, green: 0x77/255, blue: 0x57/255, opacity: 0.12)

        static let statusSuccess = SwiftUI.Color(red: 0x5C/255, green: 0x8A/255, blue: 0x5A/255)
        static let statusWarning = SwiftUI.Color(red: 0xC9/255, green: 0x91/255, blue: 0x46/255)
        static let statusError   = SwiftUI.Color(red: 0xB6/255, green: 0x46/255, blue: 0x3F/255)
        static let statusRunning = SwiftUI.Color(red: 0x5C/255, green: 0x7A/255, blue: 0xAA/255)
    }

    // ── Type. Serif for H1/H2 (brand anchors), SF Pro for UI. ────────
    enum Font {
        static let h1        = SwiftUI.Font.custom("New York", size: 22).weight(.medium)
        static let h2        = SwiftUI.Font.custom("New York", size: 17).weight(.medium)
        static let body      = SwiftUI.Font.system(size: 15)
        static let bodyTight = SwiftUI.Font.system(size: 13)
        static let caption   = SwiftUI.Font.system(size: 11).weight(.medium)
        static let code      = SwiftUI.Font.system(size: 13, design: .monospaced)
    }

    // ── Spacing scale. 4 / 8 / 12 / 16 / 24 / 32 / 48. ──────────────
    enum Space {
        static let xs:  CGFloat = 4
        static let s:   CGFloat = 8
        static let m:   CGFloat = 12
        static let l:   CGFloat = 16
        static let xl:  CGFloat = 24
        static let xxl: CGFloat = 32
    }

    // ── Corner radius. ──────────────────────────────────────────────
    enum Radius {
        static let chip:    CGFloat = 4
        static let control: CGFloat = 8
        static let card:    CGFloat = 12
        static let sheet:   CGFloat = 16
        static let window:  CGFloat = 24
    }
}
