import AppKit
import SwiftUI
import UniformTypeIdentifiers
import KuroKit

/// The Kuro mark: eight petals over eight fainter ones, with the centre cut out.
struct KuroMark: View {
    var size: CGFloat = 24

    var body: some View {
        ZStack {
            Petals(rotation: 22.5, inset: true).opacity(0.55)
            Petals(rotation: 0, inset: false)
        }
        .mask(CentreHole().fill(style: FillStyle(eoFill: true)))
        .frame(width: size, height: size)
        .accessibilityLabel("Kuro")
    }

    private struct Petals: Shape {
        let rotation: Double
        /// The back ring's petals are shorter.
        let inset: Bool

        func path(in rect: CGRect) -> Path {
            let scale = rect.width / 100
            var petal = Path()
            petal.move(to: CGPoint(x: 50, y: 50))
            if inset {
                petal.addCurve(to: CGPoint(x: 50, y: 15), control1: CGPoint(x: 39, y: 42), control2: CGPoint(x: 38, y: 25))
                petal.addCurve(to: CGPoint(x: 50, y: 50), control1: CGPoint(x: 62, y: 25), control2: CGPoint(x: 61, y: 42))
            } else {
                petal.addCurve(to: CGPoint(x: 50, y: 6), control1: CGPoint(x: 36, y: 40), control2: CGPoint(x: 34, y: 18))
                petal.addCurve(to: CGPoint(x: 50, y: 50), control1: CGPoint(x: 66, y: 18), control2: CGPoint(x: 64, y: 40))
            }
            var path = Path()
            for step in 0..<8 {
                let angle = Angle.degrees(Double(step) * 45 + rotation).radians
                let turn = CGAffineTransform(translationX: 50, y: 50).rotated(by: angle).translatedBy(x: -50, y: -50)
                path.addPath(petal, transform: turn)
            }
            return path.applying(CGAffineTransform(scaleX: scale, y: scale))
        }
    }

    private struct CentreHole: Shape {
        func path(in rect: CGRect) -> Path {
            var path = Path(rect)
            let radius = rect.width * 0.09
            path.addEllipse(in: CGRect(x: rect.midX - radius, y: rect.midY - radius, width: radius * 2, height: radius * 2))
            return path
        }
    }
}

/// A small capsule label.
struct Tag: View {
    enum Tone { case neutral, live, warning, error }

    let text: String
    var tone: Tone = .neutral
    var systemImage: String?

    init(_ text: String, tone: Tone = .neutral, systemImage: String? = nil) {
        self.text = text
        self.tone = tone
        self.systemImage = systemImage
    }

    private var color: Color {
        switch tone {
        case .neutral: .secondary
        case .live: .green
        case .warning: .orange
        case .error: .red
        }
    }

    var body: some View {
        HStack(spacing: 3) {
            if let systemImage { Image(systemName: systemImage) }
            Text(text)
        }
        .font(.caption2.weight(.medium))
        .lineLimit(1)
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .foregroundStyle(color)
        .background(color.opacity(0.12), in: Capsule())
    }
}

struct StatusDot: View {
    let color: Color

    var body: some View {
        Circle().fill(color).frame(width: 7, height: 7)
    }

    static func color(for status: String) -> Color {
        switch status {
        case "connected", "ok": .green
        case "error": .red
        default: .secondary.opacity(0.5)
        }
    }
}

struct FitBadge: View {
    let fit: FitEstimate

    var body: some View {
        let tone: Tag.Tone = switch fit.verdict {
        case "great", "fits": .live
        case "tight": .warning
        default: .error
        }
        Tag(fit.label, tone: tone).help(fit.note)
    }
}

/// An error line, in the server's own words.
struct ErrorText: View {
    let message: String

    init(_ message: String) { self.message = message }

    var body: some View {
        Label(message, systemImage: "exclamationmark.triangle.fill")
            .font(.callout)
            .foregroundStyle(.red)
            .textSelection(.enabled)
    }
}

/// Copies text, and says so for a moment.
struct CopyButton: View {
    let text: String
    var label = "Copy"
    @State private var copied = false

    var body: some View {
        Button {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
            withAnimation(.snappy) { copied = true }
            Task {
                try? await Task.sleep(for: .seconds(1.5))
                withAnimation(.snappy) { copied = false }
            }
        } label: {
            Label(copied ? "Copied" : label, systemImage: copied ? "checkmark" : "doc.on.doc")
                .contentTransition(.symbolEffect(.replace))
        }
        .help(label)
    }
}

/// Monospaced text in a rounded well, with a copy button.
struct CodeSurface: View {
    let text: String
    var language: String?
    var maxHeight: CGFloat?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Spacer()
                CopyButton(text: text)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .controlSize(.small)
            }
            .padding(.horizontal, 12)
            .padding(.top, 6)

            ScrollView([.horizontal, .vertical]) {
                Text(text.isEmpty ? " " : text)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: true, vertical: true)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 10)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: maxHeight)
            .fixedSize(horizontal: false, vertical: maxHeight == nil)
        }
        .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.primary.opacity(0.07)))
    }
}

/// A divider that resizes the panel beside it. Double-click restores the default.
struct ResizeHandle: View {
    @Binding var width: CGFloat
    var range: ClosedRange<CGFloat> = 180...640
    var standard: CGFloat = 260
    /// The panel is to the right, so dragging left widens it.
    var panelOnRight = false
    @State private var start: CGFloat?

    var body: some View {
        Rectangle()
            .fill(Color.primary.opacity(0.08))
            .frame(width: 1)
            .padding(.horizontal, 3)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
            }
            .gesture(
                DragGesture(minimumDistance: 1)
                    .onChanged { drag in
                        let origin = start ?? width
                        start = origin
                        let delta = panelOnRight ? -drag.translation.width : drag.translation.width
                        width = min(max(origin + delta, range.lowerBound), range.upperBound)
                    }
                    .onEnded { _ in start = nil }
            )
            .onTapGesture(count: 2) { width = standard }
            .accessibilityLabel("Resize panel")
    }
}

/// A capsule toggle for the composer's switches.
struct PillToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button {
            configuration.isOn.toggle()
        } label: {
            configuration.label
                .font(.callout)
                .padding(.horizontal, 9)
                .padding(.vertical, 5)
                .foregroundStyle(configuration.isOn ? Color.accentColor : .secondary)
                .background(configuration.isOn ? Color.accentColor.opacity(0.14) : .clear, in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
    }
}

/// A quiet borderless button used in toolbars and composer rows.
struct QuietButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(Color.primary.opacity(configuration.isPressed ? 0.1 : 0), in: Capsule())
            .contentShape(Capsule())
    }
}

extension View {
    /// Liquid Glass on macOS 26 and later, a material card before that.
    @ViewBuilder
    func glassCard(cornerRadius: CGFloat = 22) -> some View {
        if #available(macOS 26.0, *) {
            glassEffect(.regular, in: .rect(cornerRadius: cornerRadius))
        } else {
            background(.regularMaterial, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous).strokeBorder(Color.primary.opacity(0.08)))
                .shadow(color: .black.opacity(0.08), radius: 12, y: 4)
        }
    }

    /// Run an action and show its error inline.
    func perform(_ failure: Binding<String?>, _ work: @escaping () async throws -> Void) {
        Task { @MainActor in
            do {
                failure.wrappedValue = nil
                try await work()
            } catch {
                failure.wrappedValue = error.localizedDescription
            }
        }
    }
}

/// Native open panels.
enum Pick {
    @MainActor
    static func folder(title: String, startingAt path: String? = nil) -> String? {
        let panel = NSOpenPanel()
        panel.title = title
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        if let path, !path.isEmpty { panel.directoryURL = URL(fileURLWithPath: path) }
        return panel.runModal() == .OK ? panel.url?.path : nil
    }

    @MainActor
    static func files(title: String, types: [UTType], multiple: Bool) -> [URL] {
        let panel = NSOpenPanel()
        panel.title = title
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = multiple
        panel.allowedContentTypes = types
        return panel.runModal() == .OK ? panel.urls : []
    }
}

/// An empty state that fills its space.
struct EmptyNote: View {
    let title: String
    let systemImage: String
    var message: String?

    var body: some View {
        ContentUnavailableView {
            Label(title, systemImage: systemImage)
        } description: {
            if let message { Text(message) }
        }
    }
}

/// Lays its children out in rows, wrapping to the next row when one fills up.
struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(subviews, width: proposal.width ?? .infinity).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let placed = arrange(subviews, width: bounds.width)
        for (subview, frame) in zip(subviews, placed.frames) {
            subview.place(at: CGPoint(x: bounds.minX + frame.minX, y: bounds.minY + frame.minY), proposal: ProposedViewSize(frame.size))
        }
    }

    private func arrange(_ subviews: Subviews, width: CGFloat) -> (frames: [CGRect], size: CGSize) {
        var frames: [CGRect] = []
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0, widest: CGFloat = 0
        for subview in subviews {
            // No child is offered more than the row, so a large image scales down rather than overflowing.
            let size = subview.sizeThatFits(ProposedViewSize(width: width.isFinite ? width : nil, height: nil))
            if x > 0 && x + size.width > width {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            frames.append(CGRect(origin: CGPoint(x: x, y: y), size: size))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
            widest = max(widest, x - spacing)
        }
        return (frames, CGSize(width: widest, height: y + rowHeight))
    }
}
