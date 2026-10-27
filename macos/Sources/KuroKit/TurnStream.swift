import Foundation

/// One event from a streaming turn (`POST /api/conversations/{id}/messages`).
public enum ChatEvent: Sendable {
    case token(String)
    case reasoning(String)
    /// The turn failed.
    case error(String)
    /// Something went wrong that did not stop the turn, such as a failed search.
    case notice(String)
    case toolCall(name: String, arguments: [String: JSONValue])
    case toolResult(name: String, ok: Bool, preview: String)
    case done
}

/// What a turn is asked to do.
public struct TurnRequest: Sendable {
    public var model: String?
    public var effort: Effort
    /// Tool groups on for this message: `web`, `memory`, `projects`.
    public var tools: [String]
    /// Search before answering rather than hoping the model asks.
    public var webSearch: Bool
    /// Skills named with `/` in the message.
    public var skills: [String]

    public init(model: String?, effort: Effort, tools: [String], webSearch: Bool = false, skills: [String] = []) {
        self.model = model
        self.effort = effort
        self.tools = tools
        self.webSearch = webSearch
        self.skills = skills
    }
}

/// Splits a server-sent event stream into `(event, data)` pairs.
///
/// Bytes are buffered until a blank line ends an event, so multi-byte characters
/// split across chunks stay intact.
public struct SSEParser {
    private var buffer: [UInt8] = []

    public init() {}

    public mutating func feed(_ byte: UInt8) -> (event: String?, data: String)? {
        buffer.append(byte)
        guard byte == 0x0A, endsEvent() else { return nil }
        defer { buffer.removeAll(keepingCapacity: true) }
        return Self.parse(String(decoding: buffer, as: UTF8.self))
    }

    private func endsEvent() -> Bool {
        let count = buffer.count
        if count >= 2, buffer[count - 2] == 0x0A { return true }
        return count >= 4 && buffer[count - 2] == 0x0D && buffer[count - 3] == 0x0A
    }

    static func parse(_ block: String) -> (event: String?, data: String)? {
        var name: String?
        var data: [String] = []
        for line in block.split(omittingEmptySubsequences: false, whereSeparator: \.isNewline) {
            if line.hasPrefix("event:") {
                name = line.dropFirst(6).trimmingCharacters(in: .whitespaces)
            } else if line.hasPrefix("data:") {
                let value = line.dropFirst(5)
                data.append(String(value.hasPrefix(" ") ? value.dropFirst() : value))
            }
        }
        return data.isEmpty ? nil : (name, data.joined(separator: "\n"))
    }
}

extension ChatEvent {
    /// Decode one SSE event. Unknown or malformed events are skipped.
    public static func decode(event: String?, data: String) -> ChatEvent? {
        guard let event,
              let object = try? JSONDecoder().decode([String: JSONValue].self, from: Data(data.utf8))
        else { return nil }
        let text = { (key: String) in object[key]?.stringValue ?? "" }

        switch event {
        case "token": return .token(text("content"))
        case "reasoning": return .reasoning(text("content"))
        case "error": return .error(text("message"))
        case "notice": return .notice(text("message"))
        case "tool_call":
            if case .object(let arguments) = object["arguments"] ?? .null {
                return .toolCall(name: text("name"), arguments: arguments)
            }
            return .toolCall(name: text("name"), arguments: [:])
        case "tool_result":
            return .toolResult(name: text("name"), ok: object["ok"]?.boolValue ?? false, preview: text("preview"))
        case "done": return .done
        default: return nil
        }
    }
}

extension KuroClient {
    /// Send a message, or rewrite `editing` and answer again from there, and
    /// stream the turn's events. Cancelling the consuming task stops the request.
    public func streamTurn(conversationId: String, content: String, request turn: TurnRequest, editing: String? = nil) -> AsyncThrowingStream<ChatEvent, Error> {
        var body: Body = [
            "content": content,
            "effort": turn.effort.rawValue,
            "tools": turn.tools,
            "web_search": turn.webSearch,
            "skills": turn.skills,
        ]
        if let model = turn.model { body["model"] = model }

        let path = editing.map { "api/conversations/\(conversationId)/messages/\($0)" } ?? "api/conversations/\(conversationId)/messages"
        let method = editing == nil ? "POST" : "PATCH"

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    var request = try urlRequest(method, path, body: body)
                    request.timeoutInterval = 60 * 60
                    let (bytes, response) = try await URLSession.shared.bytes(for: request)
                    let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                    guard (200..<300).contains(status) else {
                        var data = Data()
                        for try await byte in bytes { data.append(byte) }
                        throw Self.error(from: data, status: status)
                    }

                    var parser = SSEParser()
                    for try await byte in bytes {
                        if let (name, data) = parser.feed(byte), let event = ChatEvent.decode(event: name, data: data) {
                            continuation.yield(event)
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
