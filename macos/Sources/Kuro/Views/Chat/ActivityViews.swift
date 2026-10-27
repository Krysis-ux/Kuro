import SwiftUI
import KuroKit

typealias ToolStepInfo = (name: String, arguments: [String: JSONValue], state: Turn.ToolStep.State, preview: String)

/// What the model did before answering: one summary line that opens into steps.
/// Open while a turn streams, closed once the reply has arrived.
struct ToolActivity: View {
    let steps: [ToolStepInfo]
    let startsOpen: Bool
    var onOpenPath: ((String) -> Void)?
    @State private var open: Bool?

    var body: some View {
        let isOpen = open ?? startsOpen
        let running = steps.contains { $0.state == .running }
        let failed = steps.filter { $0.state == .failed }.count

        VStack(alignment: .leading, spacing: 8) {
            Button {
                withAnimation(.snappy) { open = !isOpen }
            } label: {
                HStack(spacing: 6) {
                    if running {
                        ProgressView().controlSize(.mini)
                    } else {
                        Image(systemName: "wrench.and.screwdriver")
                    }
                    Text(summary(running: running, failed: failed)).lineLimit(1)
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                }
                .font(.callout)
                .foregroundStyle(.secondary)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if isOpen {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Array(steps.enumerated()), id: \.offset) { _, step in
                        ToolStepRow(step: step, onOpenPath: onOpenPath)
                    }
                }
                .padding(.leading, 4)
                .transition(.opacity)
            }
        }
    }

    /// While working, say what it is working on.
    private func summary(running: Bool, failed: Int) -> String {
        if running, let current = steps.last {
            let action = ToolAction.describe(current.name, current.arguments)
            return action.target.isEmpty ? "\(action.verb)…" : "\(action.verb) \(action.target)…"
        }
        let count = "\(steps.count) step\(steps.count == 1 ? "" : "s")"
        return failed > 0 ? "\(count), \(failed) failed" : count
    }
}

/// One step. Its output is folded away unless it failed.
private struct ToolStepRow: View {
    let step: ToolStepInfo
    let onOpenPath: ((String) -> Void)?
    @State private var showOutput = false

    var body: some View {
        let action = ToolAction.describe(step.name, step.arguments)
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                mark
                Image(systemName: icon(action.kind)).foregroundStyle(.tertiary).frame(width: 14)
                Text(action.verb)
                if !action.target.isEmpty {
                    if let path = action.path, let onOpenPath {
                        Button(action.target) { onOpenPath(path) }
                            .buttonStyle(.link)
                            .font(.callout.monospaced())
                            .help("Open \(path)")
                    } else {
                        Text(action.target)
                            .font(.callout.monospaced())
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                if !step.preview.isEmpty, step.state != .failed {
                    Button(showOutput ? "hide" : "output") { showOutput.toggle() }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
            .font(.callout)

            if !step.preview.isEmpty, showOutput || step.state == .failed {
                CodeSurface(text: step.preview, maxHeight: 220)
            }
        }
    }

    @ViewBuilder private var mark: some View {
        switch step.state {
        case .running: ProgressView().controlSize(.mini).frame(width: 14)
        case .done: Image(systemName: "checkmark").foregroundStyle(.green).frame(width: 14)
        case .failed: Image(systemName: "xmark").foregroundStyle(.red).frame(width: 14)
        }
    }

    private func icon(_ kind: ToolAction.Kind) -> String {
        switch kind {
        case .read: "doc.text"
        case .edit, .write: "pencil"
        case .run: "terminal"
        case .search: "magnifyingglass"
        case .list: "list.bullet"
        case .web: "globe"
        case .other: "wrench.and.screwdriver"
        }
    }
}

struct Reasoning: View {
    let text: String
    let startsOpen: Bool
    @State private var open: Bool?

    var body: some View {
        let isOpen = open ?? startsOpen
        VStack(alignment: .leading, spacing: 8) {
            Button(isOpen ? "Hide thinking" : "Show thinking") {
                withAnimation(.snappy) { open = !isOpen }
            }
            .buttonStyle(.plain)
            .font(.callout)
            .foregroundStyle(.secondary)

            if isOpen {
                Text(text)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .padding(.leading, 12)
                    .overlay(alignment: .leading) {
                        Capsule().fill(Color.secondary.opacity(0.3)).frame(width: 2)
                    }
            }
        }
    }
}

/// Pages the turn drew on, listed by Kuro rather than by the model so they cannot be invented.
struct SourcesList: View {
    let sources: [WebSource]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Sources", systemImage: "globe")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(Array(sources.enumerated()), id: \.offset) { index, source in
                if let url = URL(string: source.url) {
                    Link(destination: url) {
                        HStack(spacing: 6) {
                            Text("\(index + 1).").foregroundStyle(.tertiary).monospacedDigit()
                            Text(source.title.isEmpty ? source.url : source.title).lineLimit(1)
                            Text(url.host()?.replacingOccurrences(of: "www.", with: "") ?? "")
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                            Image(systemName: "arrow.up.right").font(.caption2)
                        }
                        .font(.callout)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}
