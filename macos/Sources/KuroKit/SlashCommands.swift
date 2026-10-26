import Foundation

/// The `/` palette in the composer.
///
/// A command either flips a switch for this message (`/web`) or names a skill
/// (`/rust`). Skills stay in the text where they were written, so the model
/// reads the request and the reason for the guidance together.
public struct SlashCommand: Hashable, Sendable, Identifiable {
    public enum Kind: Hashable, Sendable { case toggle, skill }

    public let name: String
    public let hint: String
    public let aliases: [String]
    /// Shown on the right: `on`, `off`, or `in message`.
    public let state: String?
    public let kind: Kind

    public var id: String { "\(kind):\(name)" }

    public init(name: String, hint: String, aliases: [String] = [], state: String? = nil, kind: Kind) {
        self.name = name
        self.hint = hint
        self.aliases = aliases
        self.state = state
        self.kind = kind
    }
}

/// A `/command` being typed and where it sits, as UTF-16 offsets (what
/// `NSTextView` uses for the caret).
public struct CommandToken: Equatable, Sendable {
    /// What follows the slash, lowercased.
    public let query: String
    /// Offset of the `/`.
    public let start: Int
    /// Offset just past the word.
    public let end: Int
}

public enum Slash {
    /// The command the caret is inside, if any.
    ///
    /// A command is a word that starts with `/`, so `https://x.com` and `and/or`
    /// never open the palette. A word with a second slash is a path, not a command.
    public static func token(in text: String, caret: Int) -> CommandToken? {
        let units = Array(text.utf16)
        let position = max(0, min(caret, units.count))
        let isSpace = { (unit: UInt16) in unit == 0x20 || unit == 0x0A || unit == 0x09 || unit == 0x0D }

        var start = position
        while start > 0, !isSpace(units[start - 1]) { start -= 1 }
        guard start < units.count, units[start] == 0x2F else { return nil }

        var end = position
        while end < units.count, !isSpace(units[end]) { end += 1 }

        let query = String(decoding: units[(start + 1)..<end], as: UTF16.self)
        guard !query.contains("/") else { return nil }
        return CommandToken(query: query.lowercased(), start: start, end: end)
    }

    /// Commands matching the query: prefix matches first, then substring matches.
    public static func match(_ commands: [SlashCommand], query: String) -> [SlashCommand] {
        guard !query.isEmpty else { return commands }
        let ranked: [(SlashCommand, Int)] = commands.compactMap { command in
            let names = [command.name] + command.aliases
            if names.contains(where: { $0.hasPrefix(query) }) { return (command, 0) }
            if names.contains(where: { $0.contains(query) }) { return (command, 1) }
            return nil
        }
        return ranked.enumerated()
            .sorted { ($0.element.1, $0.offset) < ($1.element.1, $1.offset) }
            .map(\.element.0)
    }

    /// Skill slugs written into a message as `/name`, matched against the catalogue
    /// so a path or a date cannot turn into a skill.
    public static func skillsNamed(in text: String, known: Set<String>) -> [String] {
        let pattern = try! NSRegularExpression(pattern: #"(^|[\s(\[{])/([A-Za-z][\w-]*)"#)
        let range = NSRange(text.startIndex..., in: text)
        var found: [String] = []
        for match in pattern.matches(in: text, range: range) {
            guard let slugRange = Range(match.range(at: 2), in: text) else { continue }
            let slug = text[slugRange].lowercased()
            if known.contains(slug), !found.contains(slug) { found.append(slug) }
        }
        return found
    }

    /// Remove every `/slug` word from a message, leaving the rest alone.
    public static func removing(_ slug: String, from text: String) -> String {
        let escaped = NSRegularExpression.escapedPattern(for: slug)
        let word = try! NSRegularExpression(pattern: #"(^|[\s(\[{])/"# + escaped + #"\b[^\S\n]?"#, options: .caseInsensitive)
        let range = NSRange(text.startIndex..., in: text)
        let removed = word.stringByReplacingMatches(in: text, range: range, withTemplate: "$1")
        let spaces = try! NSRegularExpression(pattern: #"[^\S\n]{2,}"#)
        return spaces.stringByReplacingMatches(in: removed, range: NSRange(removed.startIndex..., in: removed), withTemplate: " ")
    }

    /// Replace the token's text with `replacement`. When removing a word from
    /// between two spaces, one space goes with it. Returns the new text and caret.
    public static func replace(_ token: CommandToken, in text: String, with replacement: String) -> (text: String, caret: Int) {
        let units = Array(text.utf16)
        let before = String(decoding: units[..<token.start], as: UTF16.self)
        var after = String(decoding: units[token.end...], as: UTF16.self)
        if replacement.isEmpty, before.last?.isWhitespace == true, let first = after.first, first == " " || first == "\t" {
            after.removeFirst()
        }
        return (before + replacement + after, before.utf16.count + replacement.utf16.count)
    }
}

/// A user's message, split so `/commands` and links can be styled.
public enum RichSegment: Hashable, Sendable {
    case text(String)
    case link(String)
    case command(String)

    public static func split(_ text: String) -> [RichSegment] {
        let pattern = try! NSRegularExpression(pattern: #"(https?://[^\s<>]+|www\.[^\s<>]+)|(/[A-Za-z][\w-]*)"#)
        let ns = text as NSString
        var segments: [RichSegment] = []
        var cursor = 0

        for match in pattern.matches(in: text, range: NSRange(location: 0, length: ns.length)) {
            let whole = match.range
            if match.range(at: 2).location != NSNotFound {
                // A command must start a word and must not be followed by another `/`.
                let previous = whole.location == 0 ? " " : ns.substring(with: NSRange(location: whole.location - 1, length: 1))
                let next = whole.upperBound < ns.length ? ns.substring(with: NSRange(location: whole.upperBound, length: 1)) : ""
                if !(previous.first?.isWhitespace ?? false) && !"([{".contains(previous) { continue }
                if next == "/" { continue }
            }
            if whole.location > cursor {
                segments.append(.text(ns.substring(with: NSRange(location: cursor, length: whole.location - cursor))))
            }
            let found = ns.substring(with: whole)
            if match.range(at: 1).location != NSNotFound {
                // Trailing punctuation belongs to the sentence, not the URL.
                let trimmed = found.replacingOccurrences(of: #"[.,;:!?)\]}'"]+$"#, with: "", options: .regularExpression)
                segments.append(.link(trimmed))
                if trimmed.count < found.count { segments.append(.text(String(found.dropFirst(trimmed.count)))) }
            } else {
                segments.append(.command(found))
            }
            cursor = whole.upperBound
        }
        if cursor < ns.length { segments.append(.text(ns.substring(from: cursor))) }
        return segments
    }
}
