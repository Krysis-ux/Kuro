import AppKit
import SwiftUI
import KuroKit

/// One stored message.
struct MessageRow: View, Equatable {
    let message: Message
    let actionable: Bool
    let onFork: () -> Void
    let onEdit: (String) -> Void
    var onOpenPath: ((String) -> Void)?

    @State private var hovering = false
    @State private var editing = false
    @State private var showDetails = false

    static func == (lhs: MessageRow, rhs: MessageRow) -> Bool {
        lhs.message == rhs.message && lhs.actionable == rhs.actionable
    }

    var body: some View {
        Group {
            if message.role == "user" {
                user
            } else {
                assistant
            }
        }
        .onHover { hovering = $0 }
    }

    // MARK: User

    private var user: some View {
        HStack {
            Spacer(minLength: 80)
            VStack(alignment: .trailing, spacing: 6) {
                if editing {
                    MessageEditor(initial: message.content, onCancel: { editing = false }) { content in
                        editing = false
                        onEdit(content)
                    }
                } else {
                    Text(richText(message.content))
                        .textSelection(.enabled)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 9)
                        .background(Color.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    if actionable {
                        actions(edit: true)
                    }
                }
            }
        }
    }

    // MARK: Assistant

    @ViewBuilder private var assistant: some View {
        let tools = message.toolCalls ?? []
        // A reply that failed before producing anything is hidden rather than shown empty.
        if !message.content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || message.reasoningContent != nil || !tools.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                if !tools.isEmpty {
                    ToolActivity(
                        steps: tools.map { ($0.name, $0.arguments ?? [:], $0.ok ? Turn.ToolStep.State.done : .failed, $0.preview) },
                        startsOpen: false,
                        onOpenPath: onOpenPath
                    )
                }
                if let reasoning = message.reasoningContent, !reasoning.isEmpty {
                    Reasoning(text: reasoning, startsOpen: false)
                }
                MarkdownView(text: message.content).equatable()
                if let sources = message.webSources, !sources.isEmpty {
                    SourcesList(sources: sources)
                }
                if showDetails { details(tools: tools.count) }
                if actionable { actions(edit: false) }
            }
        }
    }

    private func details(tools: Int) -> some View {
        Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 4) {
            stat("Model", message.modelId)
            stat("Prompt tokens", message.usagePromptTokens.map(String.init))
            stat("Output tokens", message.usageCompletionTokens.map(String.init))
            stat("Speed", message.timingTokensPerSec.map { String(format: "%.1f tok/s", $0) })
            stat("First token", message.timingTtftMs.map { "\($0) ms" })
            stat("Total", message.timingTotalMs.map { String(format: "%.2f s", Double($0) / 1000) })
            stat("Tool calls", tools > 0 ? "\(tools)" : nil)
            stat("Web search", message.usedWebSearch ? "yes" : nil)
            stat("Stop reason", message.finishReason)
        }
        .font(.caption)
        .padding(10)
        .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 10))
    }

    @ViewBuilder private func stat(_ label: String, _ value: String?) -> some View {
        if let value {
            GridRow {
                Text(label).foregroundStyle(.secondary)
                Text(value).monospaced().textSelection(.enabled)
            }
        }
    }

    // MARK: Actions

    /// Space is always reserved, so nothing shifts when the row is hovered.
    private func actions(edit: Bool) -> some View {
        HStack(spacing: 2) {
            CopyButton(text: message.content).labelStyle(.iconOnly)
            Button { onFork() } label: { Image(systemName: "arrow.triangle.branch") }
                .help("Fork the conversation from here")
            if edit {
                Button { editing = true } label: { Image(systemName: "pencil") }
                    .help("Edit and resend")
            }
            if !edit, hasStats {
                Button { withAnimation(.snappy) { showDetails.toggle() } } label: { Image(systemName: "info.circle") }
                    .help(showDetails ? "Hide details" : "Tokens, speed and timing")
            }
        }
        .buttonStyle(.borderless)
        .controlSize(.small)
        .foregroundStyle(.secondary)
        .opacity(hovering || showDetails ? 1 : 0)
        .animation(.easeOut(duration: 0.15), value: hovering)
    }

    private var hasStats: Bool {
        message.usageCompletionTokens != nil || message.timingTokensPerSec != nil
    }

    /// A user's message is shown verbatim, with `/commands` and links marked.
    private func richText(_ text: String) -> AttributedString {
        var result = AttributedString()
        for segment in RichSegment.split(text) {
            switch segment {
            case .text(let value):
                result += AttributedString(value)
            case .link(let value):
                var link = AttributedString(value)
                link.link = URL(string: value.hasPrefix("www.") ? "https://\(value)" : value)
                result += link
            case .command(let value):
                var command = AttributedString(value)
                command.foregroundColor = .accentColor
                command.font = .body.weight(.medium)
                result += command
            }
        }
        return result
    }
}

/// Rewriting a sent message. Sending removes the replies after it, so an
/// unchanged or empty message just closes the editor.
private struct MessageEditor: View {
    let initial: String
    let onCancel: () -> Void
    let onSubmit: (String) -> Void
    @State private var draft = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .trailing, spacing: 8) {
            TextField("", text: $draft, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...12)
                .focused($focused)
                .padding(12)
                .frame(minWidth: 360)
                .background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .onSubmit(submit)
                .onExitCommand(perform: onCancel)
            HStack {
                Text("Sending replaces this message and removes the replies after it.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button("Cancel", action: onCancel)
                Button("Send", action: submit).buttonStyle(.borderedProminent)
            }
            .controlSize(.small)
        }
        .onAppear {
            draft = initial
            focused = true
        }
    }

    private func submit() {
        let content = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        if content.isEmpty || content == initial.trimmingCharacters(in: .whitespacesAndNewlines) {
            onCancel()
        } else {
            onSubmit(content)
        }
    }
}
