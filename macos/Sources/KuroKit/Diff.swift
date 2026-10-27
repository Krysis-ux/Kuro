import Foundation

/// A line-by-line diff of two texts, collapsed to the changed regions.
public enum LineDiff {
    public enum Kind: Sendable { case same, added, removed }

    public struct Row: Hashable, Sendable {
        public let kind: Kind
        /// Line number in the old text, when the line is there.
        public let before: Int?
        /// Line number in the new text, when the line is there.
        public let after: Int?
        public let text: String
    }

    /// A row, or a run of unchanged lines that was folded away.
    public enum Item: Hashable, Sendable {
        case row(Row)
        case gap(Int)
    }

    /// Unchanged lines kept either side of a change.
    public static let context = 3

    /// Above this many differing lines per side, the middle is reported as one
    /// block replacement instead of running the O(n·m) table.
    static let tableLimit = 2000

    /// Longest-common-subsequence diff over lines. The common prefix and suffix
    /// are stripped first, so a typical edit only tables a few lines.
    public static func rows(before: String, after: String) -> [Row] {
        let left = before.components(separatedBy: "\n")
        let right = after.components(separatedBy: "\n")

        var head = 0
        while head < left.count, head < right.count, left[head] == right[head] { head += 1 }
        var tail = 0
        while tail < left.count - head, tail < right.count - head,
              left[left.count - 1 - tail] == right[right.count - 1 - tail] { tail += 1 }

        let midLeft = Array(left[head..<(left.count - tail)])
        let midRight = Array(right[head..<(right.count - tail)])

        var rows: [Row] = (0..<head).map { Row(kind: .same, before: $0 + 1, after: $0 + 1, text: left[$0]) }

        if midLeft.count > tableLimit || midRight.count > tableLimit {
            rows += midLeft.enumerated().map { Row(kind: .removed, before: head + $0.offset + 1, after: nil, text: $0.element) }
            rows += midRight.enumerated().map { Row(kind: .added, before: nil, after: head + $0.offset + 1, text: $0.element) }
        } else {
            rows += lcs(midLeft, midRight, offset: head)
        }

        for index in 0..<tail {
            rows.append(Row(kind: .same, before: left.count - tail + index + 1, after: right.count - tail + index + 1,
                            text: left[left.count - tail + index]))
        }
        return rows
    }

    private static func lcs(_ left: [String], _ right: [String], offset: Int) -> [Row] {
        let width = right.count + 1
        var table = [Int](repeating: 0, count: (left.count + 1) * width)
        for i in stride(from: left.count - 1, through: 0, by: -1) {
            for j in stride(from: right.count - 1, through: 0, by: -1) {
                table[i * width + j] = left[i] == right[j]
                    ? table[(i + 1) * width + j + 1] + 1
                    : max(table[(i + 1) * width + j], table[i * width + j + 1])
            }
        }

        var rows: [Row] = []
        var i = 0
        var j = 0
        while i < left.count, j < right.count {
            if left[i] == right[j] {
                rows.append(Row(kind: .same, before: offset + i + 1, after: offset + j + 1, text: left[i]))
                i += 1
                j += 1
            } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
                rows.append(Row(kind: .removed, before: offset + i + 1, after: nil, text: left[i]))
                i += 1
            } else {
                rows.append(Row(kind: .added, before: nil, after: offset + j + 1, text: right[j]))
                j += 1
            }
        }
        rows += left[i...].enumerated().map { Row(kind: .removed, before: offset + i + $0.offset + 1, after: nil, text: $0.element) }
        rows += right[j...].enumerated().map { Row(kind: .added, before: nil, after: offset + j + $0.offset + 1, text: $0.element) }
        return rows
    }

    /// Fold runs of unchanged lines further than `context` from any change.
    public static func collapse(_ rows: [Row]) -> [Item] {
        var keep = Set<Int>()
        for (index, row) in rows.enumerated() where row.kind != .same {
            for near in max(0, index - context)...min(rows.count - 1, index + context) { keep.insert(near) }
        }

        var items: [Item] = []
        var skipped = 0
        for (index, row) in rows.enumerated() {
            if keep.contains(index) {
                if skipped > 0 { items.append(.gap(skipped)) }
                skipped = 0
                items.append(.row(row))
            } else {
                skipped += 1
            }
        }
        if skipped > 0 { items.append(.gap(skipped)) }
        return items
    }

    /// Unified-diff style text, for copying.
    public static func text(_ items: [Item]) -> String {
        items.map { item in
            switch item {
            case .gap: return "…"
            case .row(let row):
                let sign = row.kind == .added ? "+" : row.kind == .removed ? "-" : " "
                return sign + row.text
            }
        }.joined(separator: "\n")
    }
}
