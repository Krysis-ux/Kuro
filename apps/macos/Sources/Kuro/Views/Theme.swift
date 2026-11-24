import AppKit
import SwiftUI

/// The app's colours: near-black charcoal in dark mode, plain white in light,
/// with one raised surface for cards and inputs.
enum Palette {
    /// Behind every page.
    static let canvas = dynamic(light: 0xFFFFFF, dark: 0x181818)
    /// Cards, the composer, list wells.
    static let raised = dynamic(light: 0xF5F5F4, dark: 0x232323)
    /// A card sitting on a card.
    static let sunken = dynamic(light: 0xECECEB, dark: 0x1C1C1C)
    /// Hairlines and card borders.
    static let line = Color.primary.opacity(0.08)
    static let hover = Color.primary.opacity(0.05)
    static let selected = Color.primary.opacity(0.09)
    /// The primary button: white on dark, black on light.
    static let ink = dynamic(light: 0x111111, dark: 0xF2F2F2)
    static let onInk = dynamic(light: 0xFFFFFF, dark: 0x111111)

    private static func dynamic(light: UInt32, dark: UInt32) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let hex = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
            return NSColor(
                srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255,
                alpha: 1
            )
        })
    }
}

// MARK: - The mark

/// The Kuro mark: a rounded k whose upper arm is a dot.
struct KuroMark: View {
    var size: CGFloat = 24

    var body: some View {
        KuroGlyph()
            .frame(width: size, height: size)
            .accessibilityLabel("Kuro")
    }
}

/// Drawn on a 100-unit grid. `packaging/macos/icon.svg` and the web `Logo.tsx`
/// use the same numbers; change all three together.
struct KuroGlyph: Shape {
    func path(in rect: CGRect) -> Path {
        let unit = min(rect.width, rect.height) / 100
        let origin = CGPoint(x: rect.midX - 50 * unit, y: rect.midY - 50 * unit)
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: origin.x + x * unit, y: origin.y + y * unit) }

        var strokes = Path()
        strokes.move(to: point(28, 20))
        strokes.addLine(to: point(28, 80))
        strokes.move(to: point(42, 52))
        strokes.addLine(to: point(70, 80))
        var mark = strokes.strokedPath(StrokeStyle(lineWidth: 15 * unit, lineCap: .round))
        mark.addEllipse(in: CGRect(origin: point(53, 19), size: CGSize(width: 26 * unit, height: 26 * unit)))
        return mark
    }
}

// MARK: - Surfaces

extension View {
    /// A raised rounded card with a hairline border.
    func card(cornerRadius: CGFloat = 12, fill: Color = Palette.raised) -> some View {
        background(fill, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous).strokeBorder(Palette.line))
    }

    /// The Form look used by every settings page: grouped cards on the page colour.
    func settingsForm() -> some View {
        formStyle(.grouped)
            .scrollContentBackground(.hidden)
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
    }
}

/// White-on-dark (black-on-light) filled button, for the one main action on a page.
struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.callout.weight(.medium))
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .foregroundStyle(Palette.onInk)
            .background(Palette.ink.opacity(configuration.isPressed ? 0.8 : 1), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .opacity(enabled ? 1 : 0.4)
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

/// A bordered quiet button, for everything that is not the main action.
struct SubtleButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.callout)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(configuration.isPressed ? Palette.selected : Palette.hover, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Palette.line))
            .opacity(enabled ? 1 : 0.4)
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

// MARK: - Help

/// A question mark that explains a feature in a popover, so the page itself
/// only needs a title and a few words.
struct HelpTip: View {
    let text: String
    @State private var open = false

    init(_ text: String) { self.text = text }

    var body: some View {
        Button { open.toggle() } label: {
            Image(systemName: "questionmark.circle")
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
        }
        .buttonStyle(.plain)
        .help(text)
        .accessibilityLabel("About this")
        .popover(isPresented: $open, arrowEdge: .bottom) {
            Text(text)
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
                .frame(width: 260, alignment: .leading)
                .padding(12)
        }
    }
}

/// A title with an optional help tip, for a Form section header or a row label.
struct TitleWithHelp: View {
    let title: String
    var help: String?

    init(_ title: String, help: String? = nil) {
        self.title = title
        self.help = help
    }

    var body: some View {
        HStack(spacing: 5) {
            Text(title)
            if let help { HelpTip(help) }
        }
    }
}

// MARK: - Page chrome

/// The large title at the top of a settings page, with a one-line subtitle and
/// room for the page's own buttons on the right.
struct PageHeader<Trailing: View>: View {
    let title: String
    var subtitle: String?
    var help: String?
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(alignment: .lastTextBaseline, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.system(size: 26, weight: .semibold))
                if subtitle != nil || help != nil {
                    HStack(spacing: 6) {
                        if let subtitle { Text(subtitle).foregroundStyle(.secondary) }
                        if let help { HelpTip(help) }
                    }
                    .font(.callout)
                }
            }
            Spacer(minLength: 12)
            trailing
        }
    }
}

extension PageHeader where Trailing == EmptyView {
    init(title: String, subtitle: String? = nil, help: String? = nil) {
        self.init(title: title, subtitle: subtitle, help: help) { EmptyView() }
    }
}

/// A row in a sidebar: icon, label, optional trailing text, and a soft pill when selected.
struct SidebarRow<Trailing: View>: View {
    let title: String
    let systemImage: String?
    let selected: Bool
    let action: () -> Void
    @ViewBuilder var trailing: Trailing
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                if let systemImage {
                    Image(systemName: systemImage)
                        .font(.system(size: 13))
                        .frame(width: 18)
                        .foregroundStyle(selected ? .primary : .secondary)
                }
                Text(title).lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 4)
                trailing
            }
            .font(.system(size: 13))
            .padding(.horizontal, 8)
            .frame(height: 30)
            .background(
                selected ? Palette.selected : hovering ? Palette.hover : .clear,
                in: RoundedRectangle(cornerRadius: 7, style: .continuous)
            )
            .contentShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

extension SidebarRow where Trailing == EmptyView {
    init(_ title: String, systemImage: String?, selected: Bool, action: @escaping () -> Void) {
        self.init(title: title, systemImage: systemImage, selected: selected, action: action) { EmptyView() }
    }
}

/// The small grey label above a group of sidebar rows.
struct SidebarHeading<Trailing: View>: View {
    let title: String
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack {
            Text(title).font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary)
            Spacer()
            trailing
        }
        .padding(.horizontal, 8)
        .padding(.top, 14)
        .padding(.bottom, 4)
    }
}

extension SidebarHeading where Trailing == EmptyView {
    init(_ title: String) { self.init(title: title) { EmptyView() } }
}
