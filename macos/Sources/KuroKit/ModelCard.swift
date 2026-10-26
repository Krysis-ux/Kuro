import Foundation

// MARK: - Types returned by `/api/models/details`

/// Everything the Hub publishes about one repository.
public struct HubModelCard: Codable, Sendable {
    public struct Gguf: Codable, Sendable {
        public let total: Int64?
        public let architecture: String?
        public let contextLength: Int?
        public let chatTemplate: String?
    }

    public let repo: String
    public let author: String?
    public let downloads: Int
    public let likes: Int
    public let createdAt: String?
    public let lastModified: String?
    public let pipelineTag: String?
    public let libraryName: String?
    public let license: String?
    public let tags: [String]
    public let gated: Bool
    public let baseModels: [String]
    public let datasets: [String]
    public let languages: [String]
    public let spaces: [String]
    public let gguf: Gguf?
    /// The model card as markdown, front matter removed.
    public let readme: String?
    public let papers: [HubPaper]
    /// Single-file GGUF weights, smallest first.
    public let files: [HubFile]
    /// Quantizations published only as split shards, which cannot be loaded yet.
    public let splitQuants: [String]
    public let hasProjector: Bool

    /// The largest file that runs comfortably, else the largest that merely fits.
    public var bestFit: HubFile? {
        files.last { $0.fit.verdict == "great" } ?? files.last { $0.fit.verdict == "fits" }
    }
}

/// One weights file, with how it fits this Mac.
public struct HubFile: Codable, Hashable, Sendable, Identifiable {
    public let filename: String
    public let quant: String?
    public let size: Int64
    public let fit: FitEstimate
    public let installed: Bool

    public var id: String { filename }

    /// `Q4_K_M` → `4-bit`, `BF16` → `16-bit`.
    public var bits: String? {
        quant?.firstMatch(of: #/\d+/#).map { "\($0.output)-bit" }
    }
}

public struct HubPaper: Codable, Hashable, Sendable, Identifiable {
    /// The arXiv id.
    public let id: String
    /// Missing when the Hub has no page for the paper; the arXiv link still works.
    public let title: String?
    public let summary: String?
    public let publishedAt: String?
    public let upvotes: Int?
    public let authors: [String]
}

// MARK: - Model card text

/// A piece of a model card, ready to draw.
public enum CardBlock: Hashable, Sendable {
    case markdown(String)
    /// Images side by side with nothing between them, such as a row of badges.
    case images([CardImage])
    case video(URL)
}

public struct CardImage: Hashable, Sendable {
    public let url: URL
    /// The card's own width for it in pixels, when it set one.
    public let width: Double?

    public init(url: URL, width: Double?) {
        self.url = url
        self.width = width
    }
}

/// Turns a Hub model card into blocks the native markdown renderer can draw.
///
/// Cards mix markdown with HTML — centred logos, badge rows, links, demo
/// videos — and Foundation's parser shows HTML as raw text and drops images.
/// So media is lifted out into its own blocks, the commonest tags become their
/// markdown equivalents, the rest are removed, and relative paths are pointed
/// at the repository. Code, fenced or inline, is left exactly as written.
public enum ModelCardText {
    public static func blocks(_ readme: String, repo: String) -> [CardBlock] {
        var blocks: [CardBlock] = []
        var pending = ""

        func flush() {
            let text = pending.replacing(#/\n{3,}/#, with: "\n\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty { blocks.append(.markdown(text)) }
            pending = ""
        }

        for (text, isCode) in fenced(readme) {
            if isCode {
                pending += text
                continue
            }
            for piece in media(in: markdown(fromHTML: text, repo: repo), repo: repo) {
                switch piece {
                case .text(let text):
                    pending += text
                case .image(let image):
                    // Only whitespace since the last image: same row.
                    if pending.allSatisfy(\.isWhitespace), case .images(let row)? = blocks.last {
                        blocks[blocks.count - 1] = .images(row + [image])
                        pending = ""
                    } else {
                        flush()
                        blocks.append(.images([image]))
                    }
                case .video(let url):
                    flush()
                    blocks.append(.video(url))
                }
            }
        }
        flush()
        return blocks
    }

    /// An absolute URL for a path written in the card. Links go to the file's
    /// page on the Hub; images and video to the raw file.
    public static func resolve(_ raw: String, repo: String, link: Bool) -> URL? {
        let trimmed = raw.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, !trimmed.lowercased().hasPrefix("javascript:") else { return nil }
        if trimmed.contains("://") || trimmed.hasPrefix("mailto:") || trimmed.hasPrefix("#") {
            return URL(string: trimmed)
        }
        if trimmed.hasPrefix("//") { return URL(string: "https:" + trimmed) }
        if trimmed.hasPrefix("/") { return URL(string: "https://huggingface.co" + trimmed) }
        let path = trimmed.hasPrefix("./") ? String(trimmed.dropFirst(2)) : trimmed
        return URL(string: "https://huggingface.co/\(repo)/\(link ? "blob" : "resolve")/main/\(path)")
    }

    // MARK: Steps

    /// Split into prose and fenced code, in order.
    static func fenced(_ text: String) -> [(String, Bool)] {
        var parts: [(String, Bool)] = []
        var current = ""
        var inCode = false
        for line in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            let fence = trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~")
            if fence && !inCode {
                parts.append((current, false))
                current = ""
                inCode = true
                current += line + "\n"
            } else if fence && inCode {
                current += line + "\n"
                parts.append((current, true))
                current = ""
                inCode = false
            } else {
                current += line + "\n"
            }
        }
        parts.append((current, inCode))
        return parts.filter { !$0.0.isEmpty }
    }

    /// The tags cards use most, rewritten as markdown; every other tag removed.
    static func markdown(fromHTML text: String, repo: String) -> String {
        // Inline code is set aside so `<tool_call>` in backticks survives.
        var spans: [String] = []
        var result = text.replacing(#/`[^`\n]+`/#) { match in
            spans.append(String(match.output))
            return "\u{E000}\(spans.count - 1)\u{E001}"
        }

        // Indented HTML would otherwise read as an indented code block once its tags are gone.
        result = result.split(separator: "\n", omittingEmptySubsequences: false).map { line in
            let trimmed = line.drop(while: { $0 == " " || $0 == "\t" })
            return trimmed.hasPrefix("<") ? String(trimmed) : String(line)
        }.joined(separator: "\n")

        result = result
            .replacing(#/<!--[\s\S]*?-->/#, with: "")
            .replacing(#/(?i)<a\b[^>]*?href\s*=\s*["']([^"']*)["'][^>]*>([\s\S]*?)<\/a>/#) { match in
                // A linked image (a badge) is shown as the image; the link has nowhere to go.
                if match.output.2.localizedCaseInsensitiveContains("<img") { return String(match.output.2) }
                let target = resolve(String(match.output.1), repo: repo, link: true)?.absoluteString ?? ""
                return "[\(match.output.2)](\(target))"
            }
            .replacing(#/\[\s*(!\[[^\]]*\]\([^)]*\))\s*\]\([^)]*\)/#) { String($0.output.1) }
            .replacing(#/(?i)<br\s*\/?>/#, with: "\n")
            .replacing(#/(?i)<(?:strong|b)>([\s\S]*?)<\/(?:strong|b)>/#) { "**\($0.output.1)**" }
            .replacing(#/(?i)<(?:em|i)>([\s\S]*?)<\/(?:em|i)>/#) { "*\($0.output.1)*" }
            .replacing(#/(?i)<code>([\s\S]*?)<\/code>/#) { "`\($0.output.1)`" }
            .replacing(#/(?i)<h([1-6])[^>]*>([\s\S]*?)<\/h[1-6]>/#) { match in
                "\n\n\(String(repeating: "#", count: Int(match.output.1) ?? 2)) \(match.output.2)\n\n"
            }
            .replacing(#/(?i)<li[^>]*>/#, with: "\n- ")
            .replacing(#/(?i)<\/t[dh]>/#, with: " · ")
            .replacing(#/(?i)<\/?(?:p|div|center|table|thead|tbody|tr|ul|ol|section|details|summary|blockquote)\b[^>]*>/#, with: "\n\n")
            // Media tags stay for `media(in:)`; everything else goes.
            .replacing(#/(?i)<(?!\/?(?:img|video|source)\b)\/?[a-z][^>]*>/#, with: "")

        // Relative markdown links, pointed at the repository.
        result = result.replacing(#/(!?)\[([^\]]*)\]\(\s*([^)\s]+)\s*\)/#) { match in
            // Images are left for `media(in:)`, which resolves them as files rather than pages.
            guard match.output.1.isEmpty else { return String(match.output.0) }
            let target = resolve(String(match.output.3), repo: repo, link: true)?.absoluteString ?? ""
            return "[\(match.output.2)](\(target))"
        }

        for (index, span) in spans.enumerated() {
            result = result.replacingOccurrences(of: "\u{E000}\(index)\u{E001}", with: span)
        }
        return result
    }

    enum Piece {
        case text(String)
        case image(CardImage)
        case video(URL)
    }

    /// Lift images and videos out of the text, in order.
    static func media(in text: String, repo: String) -> [Piece] {
        let pattern = #/(?i)<video\b[\s\S]*?(?:<\/video>|\/>|>(?![\s\S]*?<\/video>))|<img\b[^>]*>|!\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/#
        var pieces: [Piece] = []
        var cursor = text.startIndex

        for match in text.matches(of: pattern) {
            pieces.append(.text(String(text[cursor..<match.range.lowerBound])))
            cursor = match.range.upperBound

            let tag = String(match.output.0)
            let source = match.output.1.map(String.init) ?? attribute("src", in: tag)
            guard let source, let url = resolve(source, repo: repo, link: false) else { continue }

            if tag.lowercased().hasPrefix("<video") || isVideo(url) {
                pieces.append(.video(url))
            } else {
                let width = attribute("width", in: tag).flatMap { Double($0.trimmingCharacters(in: .letters)) }
                pieces.append(.image(CardImage(url: url, width: width)))
            }
        }
        pieces.append(.text(String(text[cursor...])))
        return pieces
    }

    /// An attribute's value, ignoring percentages (`width="60%"`), which have no pixel size.
    static func attribute(_ name: String, in tag: String) -> String? {
        guard let match = tag.firstMatch(of: try! Regex("(?i)\\b\(name)\\s*=\\s*[\"']([^\"']*)[\"']")),
              let value = match.output[1].substring.map(String.init),
              !value.hasSuffix("%") else { return nil }
        return value
    }

    static func isVideo(_ url: URL) -> Bool {
        ["mp4", "webm", "mov"].contains(url.pathExtension.lowercased())
    }
}
