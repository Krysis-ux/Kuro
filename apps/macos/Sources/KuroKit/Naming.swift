import Foundation

/// How model ids, sizes, counts and times are shown.
public enum Naming {
    static let freePrefix = "free:"
    static let remotePrefix = "cloud:"
    static let freeFlavours: Set<String> = ["auto", "coding", "reasoning", "fast"]

    /// One of the pooled free-tier models (`free:auto`, `free:coding`…).
    public static func isFree(_ id: String) -> Bool {
        id.hasPrefix(freePrefix) && freeFlavours.contains(String(id.dropFirst(freePrefix.count)))
    }

    /// Whether choosing this model sends the conversation off the machine.
    public static func isRemote(_ id: String) -> Bool {
        isFree(id) || (id.hasPrefix(remotePrefix) && id.dropFirst(remotePrefix.count).contains("/"))
    }

    /// The readable name: no quantization suffix, no connector prefix.
    public static func friendly(_ id: String) -> String {
        if isFree(id) {
            let flavour = id.dropFirst(freePrefix.count)
            return flavour == "auto" ? "Kuro Free" : "Kuro Free · \(flavour)"
        }
        if isRemote(id) {
            // Everything after the connector id is the provider's own name, which may contain slashes.
            let rest = id.dropFirst(remotePrefix.count)
            return String(rest[rest.index(after: rest.firstIndex(of: "/")!)...])
        }
        return String(id.split(separator: ":", maxSplits: 1).first ?? Substring(id))
    }

    public static func quant(of id: String) -> String? {
        guard !isRemote(id) else { return nil }
        let parts = id.split(separator: ":", maxSplits: 1)
        return parts.count > 1 ? parts[1].uppercased() : nil
    }

    /// The owner in a `publisher/name` id.
    public static func publisher(of id: String) -> String? {
        let name = friendly(id)
        guard let slash = name.firstIndex(of: "/"), slash != name.startIndex else { return nil }
        return String(name[..<slash])
    }

    /// The family a local model is grouped under: `qwen2.5-0.5b` and `qwen3-4b` are both `qwen`.
    public static func brand(of id: String, family: String?) -> String {
        if let publisher = publisher(of: id) { return publisher }
        let leading = friendly(id).split(whereSeparator: { $0 == "/" || $0 == ":" }).first.map(String.init) ?? id
        let word = leading.split(separator: "-").first.map(String.init) ?? leading
        let stripped = word.replacingOccurrences(of: #"[\d.]+$"#, with: "", options: .regularExpression)
        return stripped.count >= 2 ? stripped : (family ?? word)
    }

    public static func bytes(_ value: Int64?) -> String {
        guard let value, value > 0 else { return "—" }
        let units: [(String, Double)] = [("TB", pow(1024, 4)), ("GB", pow(1024, 3)), ("MB", pow(1024, 2)), ("KB", 1024)]
        for (label, size) in units where Double(value) >= size {
            return String(format: "%.1f %@", Double(value) / size, label)
        }
        return "\(value) B"
    }

    /// `1.2M`, `34k`, `912`.
    public static func count(_ value: Int) -> String {
        if value >= 1_000_000 { return String(format: "%.1fM", Double(value) / 1_000_000) }
        if value >= 1_000 { return "\(Int((Double(value) / 1_000).rounded()))k" }
        return "\(value)"
    }

    /// `70B` rather than `70.0B`; `6.7B` kept.
    public static func params(_ billions: Double) -> String {
        billions.rounded() == billions ? "\(Int(billions))B" : String(format: "%.1fB", billions)
    }

    static let isoWithFraction: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    public static func date(_ iso: String) -> Date? {
        isoWithFraction.date(from: iso) ?? ISO8601DateFormatter().date(from: iso)
    }

    /// `just now`, `5m ago`, `3h ago`, `2d ago`, then a short date.
    public static func relative(_ iso: String, now: Date = Date()) -> String {
        guard let then = date(iso) else { return "" }
        let seconds = max(0, Int(now.timeIntervalSince(then)))
        if seconds < 60 { return "just now" }
        if seconds < 3600 { return "\(seconds / 60)m ago" }
        if seconds < 86_400 { return "\(seconds / 3600)h ago" }
        if seconds < 7 * 86_400 { return "\(seconds / 86_400)d ago" }
        return then.formatted(.dateTime.month(.abbreviated).day())
    }

    public static func uptime(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m \(seconds % 60)s" }
        return "\(seconds / 3600)h \((seconds % 3600) / 60)m"
    }

    /// The last path component, for naming a workspace after its folder.
    public static func leaf(of path: String) -> String {
        path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? ""
    }
}

/// A tool call, described as an action rather than a wire name.
public struct ToolAction: Sendable {
    public enum Kind: Sendable { case read, edit, write, run, search, list, web, other }

    public let kind: Kind
    public let verb: String
    /// The file, command or query it acted on.
    public let target: String
    /// Set when `target` is a workspace path that can be opened.
    public let path: String?

    public static func describe(_ name: String, _ arguments: [String: JSONValue]) -> ToolAction {
        let text = { (key: String) -> String? in
            guard let value = arguments[key]?.stringValue, !value.isEmpty else { return nil }
            return value
        }
        let path = text("path") ?? text("file") ?? text("file_path")

        switch name {
        case "read_file":
            var range = ""
            if let from = arguments["start_line"]?.intValue {
                range = ":\(from)" + (arguments["end_line"]?.intValue.map { "-\($0)" } ?? "+")
            }
            return ToolAction(kind: .read, verb: "Read", target: (path ?? "a file") + range, path: path)
        case "write_file": return ToolAction(kind: .write, verb: "Wrote", target: path ?? "a file", path: path)
        case "edit_file": return ToolAction(kind: .edit, verb: "Edited", target: path ?? "a file", path: path)
        case "run_command": return ToolAction(kind: .run, verb: "Ran", target: text("command") ?? "", path: nil)
        case "start_server": return ToolAction(kind: .run, verb: "Started", target: text("command") ?? "", path: nil)
        case "stop_server": return ToolAction(kind: .run, verb: "Stopped", target: text("command") ?? "a process", path: nil)
        case "check_server": return ToolAction(kind: .run, verb: "Checked", target: text("command") ?? "what is running", path: nil)
        case "search_files": return ToolAction(kind: .search, verb: "Searched for", target: text("query") ?? text("pattern") ?? "", path: nil)
        case "find_files": return ToolAction(kind: .search, verb: "Looked for", target: text("pattern") ?? "", path: nil)
        case "project_tree": return ToolAction(kind: .list, verb: "Listed", target: path ?? "the project", path: nil)
        case "web_search": return ToolAction(kind: .web, verb: "Searched the web for", target: text("query") ?? "", path: nil)
        case "fetch_url", "read_url": return ToolAction(kind: .web, verb: "Fetched", target: text("url") ?? "", path: nil)
        case "remember": return ToolAction(kind: .other, verb: "Remembered", target: text("fact") ?? text("content") ?? "", path: nil)
        case "recall": return ToolAction(kind: .other, verb: "Recalled", target: text("query") ?? "", path: nil)
        default:
            // An MCP tool this build has never seen: its name and first string argument.
            let first = arguments.keys.sorted().compactMap { arguments[$0]?.stringValue }.first ?? ""
            return ToolAction(kind: .other, verb: name.replacingOccurrences(of: "_", with: " "), target: first, path: nil)
        }
    }
}

/// One node of a workspace's file tree.
public struct FileNode: Identifiable, Hashable, Sendable {
    public let name: String
    /// Relative path; directories end in `/`.
    public let path: String
    public var isDirectory: Bool
    public var children: [FileNode]

    public var id: String { path }

    /// Build a tree from the flat path list the server returns. Folders first, then files.
    public static func tree(from paths: [String]) -> [FileNode] {
        var roots: [FileNode] = []
        for path in paths {
            let isDirectory = path.hasSuffix("/")
            let parts = path.split(separator: "/").map(String.init)
            insert(parts[...], fullPath: path, isDirectory: isDirectory, prefix: "", into: &roots)
        }
        return sorted(roots)
    }

    private static func insert(_ parts: ArraySlice<String>, fullPath: String, isDirectory: Bool, prefix: String, into level: inout [FileNode]) {
        guard let name = parts.first else { return }
        let last = parts.count == 1
        let here = prefix + name
        var index = level.firstIndex { $0.name == name }
        if index == nil {
            level.append(FileNode(name: name, path: last && !isDirectory ? fullPath : here + "/", isDirectory: !last || isDirectory, children: []))
            index = level.count - 1
        }
        // A directory can be listed after a file inside it.
        if !last { level[index!].isDirectory = true }
        insert(parts.dropFirst(), fullPath: fullPath, isDirectory: isDirectory, prefix: here + "/", into: &level[index!].children)
    }

    private static func sorted(_ nodes: [FileNode]) -> [FileNode] {
        nodes
            .map { node in
                var copy = node
                copy.children = sorted(node.children)
                return copy
            }
            .sorted { $0.isDirectory != $1.isDirectory ? $0.isDirectory : $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }
}

/// Split a command line into arguments, keeping quoted runs together so a
/// path with a space in it stays one argument.
public func shellWords(_ raw: String) -> [String] {
    var words: [String] = []
    var current = ""
    var quote: Character?
    for character in raw.trimmingCharacters(in: .whitespaces) {
        if let open = quote {
            if character == open { quote = nil } else { current.append(character) }
        } else if character == "\"" || character == "'" {
            quote = character
        } else if character == " " {
            if !current.isEmpty { words.append(current) }
            current = ""
        } else {
            current.append(character)
        }
    }
    if !current.isEmpty { words.append(current) }
    return words
}

/// Whether decoded text is really a binary file: a NUL byte, or more than 2%
/// replacement characters.
public func looksBinary(_ text: String) -> Bool {
    let sample = text.prefix(4000)
    if sample.contains("\u{0}") { return true }
    let replacements = sample.filter { $0 == "\u{FFFD}" }.count
    return Double(replacements) > Double(sample.count) * 0.02
}
