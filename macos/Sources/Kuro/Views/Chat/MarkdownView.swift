import SwiftUI
import KuroKit

/// A rendered reply. Equatable so finished messages are not re-parsed while
/// another one streams.
struct MarkdownView: View, Equatable {
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(Markdown.blocks(text).enumerated()), id: \.offset) { _, block in
                view(for: block)
            }
        }
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder private func view(for block: MarkdownBlock) -> some View {
        switch block {
        case .paragraph(let content, let indent, let marker, let quote):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                if indent > 0 {
                    Text(marker ?? "")
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                        .frame(width: 20, alignment: .trailing)
                }
                Text(styled(content))
                    .lineSpacing(3)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.leading, CGFloat(max(0, indent - 1)) * 22)
            .padding(.leading, quote ? 14 : 0)
            .foregroundStyle(quote ? .secondary : .primary)
            .overlay(alignment: .leading) {
                if quote { Capsule().fill(Color.secondary.opacity(0.35)).frame(width: 3) }
            }

        case .heading(let content, let level):
            Text(styled(content))
                .font(level == 1 ? .title2.weight(.semibold) : level == 2 ? .title3.weight(.semibold) : .headline)
                .padding(.top, 6)

        case .code(let code, let language):
            CodeSurface(text: code, language: language)

        case .table(let rows):
            ScrollView(.horizontal) {
                Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 8) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                        GridRow {
                            ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                                Text(styled(cell)).fontWeight(index == 0 ? .semibold : .regular)
                            }
                        }
                        if index == 0 { Divider() }
                    }
                }
                .padding(12)
            }
            .background(Color.primary.opacity(0.03), in: RoundedRectangle(cornerRadius: 10))

        case .rule:
            Divider().padding(.vertical, 4)
        }
    }

    /// Give inline code a tinted background; bold, italics and links are
    /// already understood by `Text`.
    private func styled(_ content: AttributedString) -> AttributedString {
        var result = content
        for run in result.runs where run.inlinePresentationIntent?.contains(.code) == true {
            result[run.range].font = .system(.body, design: .monospaced)
            result[run.range].backgroundColor = Color.primary.opacity(0.07)
        }
        return result
    }
}
