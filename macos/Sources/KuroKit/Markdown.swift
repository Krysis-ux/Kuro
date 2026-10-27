import Foundation

/// A block of a rendered reply.
public enum MarkdownBlock: Hashable, Sendable {
    /// A paragraph. `indent` is the list nesting depth; `marker` (`•` or `3.`)
    /// is set on the first paragraph of a list item.
    case paragraph(AttributedString, indent: Int, marker: String?, quote: Bool)
    case heading(AttributedString, level: Int)
    case code(String, language: String?)
    /// The first row is the header.
    case table([[AttributedString]])
    case rule
}

/// Splits markdown into blocks using Foundation's parser, so inline styling
/// (bold, code, links) comes back as attributes on each block's text.
public enum Markdown {
    public static func blocks(_ text: String) -> [MarkdownBlock] {
        let options = AttributedString.MarkdownParsingOptions(
            allowsExtendedAttributes: false,
            interpretedSyntax: .full,
            failurePolicy: .returnPartiallyParsedIfPossible
        )
        guard let parsed = try? AttributedString(markdown: text, options: options) else {
            return [.paragraph(AttributedString(text), indent: 0, marker: nil, quote: false)]
        }

        var blocks: [MarkdownBlock] = []
        var tables: [Int: (index: Int, rows: [Int: [Int: AttributedString]])] = [:]
        var markedItems = Set<Int>()

        for (intent, range) in parsed.runs[\.presentationIntent] {
            let content = AttributedString(parsed[range])
            // Innermost first: a paragraph inside a list item inside a list.
            let components = intent?.components ?? []
            guard let leaf = components.first else {
                blocks.append(.paragraph(content, indent: 0, marker: nil, quote: false))
                continue
            }

            switch leaf.kind {
            case .codeBlock(let language):
                var code = String(content.characters)
                if code.hasSuffix("\n") { code.removeLast() }
                blocks.append(.code(code, language: language.flatMap { $0.isEmpty ? nil : $0 }))

            case .header(let level):
                blocks.append(.heading(content, level: level))

            case .thematicBreak:
                blocks.append(.rule)

            case .tableCell(let column):
                guard let table = components.first(where: { if case .table = $0.kind { return true } else { return false } }) else { continue }
                var row = -1
                for component in components {
                    if case .tableRow(let index) = component.kind { row = index }
                }
                if tables[table.identity] == nil {
                    blocks.append(.table([]))
                    tables[table.identity] = (blocks.count - 1, [:])
                }
                tables[table.identity]!.rows[row, default: [:]][column] = content

            default:
                var marker: String?
                var lists = 0
                for (position, component) in components.enumerated() {
                    switch component.kind {
                    case .orderedList, .unorderedList:
                        lists += 1
                    case .listItem(let ordinal) where markedItems.insert(component.identity).inserted && marker == nil:
                        let ordered = components.dropFirst(position + 1).first.map {
                            if case .orderedList = $0.kind { return true } else { return false }
                        } ?? false
                        marker = ordered ? "\(ordinal)." : "•"
                    default:
                        break
                    }
                }
                let quote = components.contains { if case .blockQuote = $0.kind { return true } else { return false } }
                blocks.append(.paragraph(content, indent: lists, marker: marker, quote: quote))
            }
        }

        for (_, table) in tables {
            let rows = table.rows.keys.sorted().map { key -> [AttributedString] in
                let cells = table.rows[key]!
                return (0...(cells.keys.max() ?? 0)).map { cells[$0] ?? AttributedString() }
            }
            blocks[table.index] = .table(rows)
        }
        return blocks
    }
}
