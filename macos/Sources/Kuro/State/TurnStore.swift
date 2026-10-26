import Foundation
import KuroKit

/// One conversation's turn as the transcript shows it.
///
/// A turn belongs to the server, not to a view: leaving the page keeps it
/// running and coming back shows it where it got to.
@MainActor @Observable
final class Turn {
    struct ToolStep: Identifiable {
        enum State { case running, done, failed }

        let id = UUID()
        let name: String
        let arguments: [String: JSONValue]
        var state: State
        var preview: String
    }

    struct Live {
        var content = ""
        var reasoning = ""
        var tools: [ToolStep] = []
        /// When the last event arrived. Drives the "still working" timer.
        var touchedAt = Date()
    }

    /// The user's message, shown until the server's copy is reloaded.
    struct Pending {
        let content: String
        /// The message being rewritten; it and everything after it are hidden.
        let replacing: String?
    }

    var live: Live?
    var error: String?
    var notices: [String] = []
    var pending: Pending?
    fileprivate(set) var isRunning = false

    @ObservationIgnored fileprivate var task: Task<Void, Never>?

    /// Messages as they should appear right now: the stored ones, cut at a
    /// rewrite, plus the message being sent.
    func display(_ stored: [Message], conversationId: String) -> [Message] {
        guard let pending else { return stored }
        var kept = stored
        if let replacing = pending.replacing, let cut = stored.firstIndex(where: { $0.id == replacing }) {
            kept = Array(stored[..<cut])
        }
        // The server stores the message as the turn starts, so a reload mid-turn may already have it.
        if let last = kept.last, last.role == "user", last.content == pending.content { return kept }
        return kept + [Message(id: "pending", conversationId: conversationId, role: "user", content: pending.content)]
    }
}

/// Turns in flight, keyed by conversation.
@MainActor @Observable
final class TurnStore {
    /// Conversations with a turn running, for the sidebar.
    private(set) var running: Set<String> = []

    @ObservationIgnored private var turns: [String: Turn] = [:]
    @ObservationIgnored weak var app: AppModel?

    /// How often streamed text is pushed to the screen.
    private nonisolated static let flushInterval: TimeInterval = 0.05

    func turn(_ conversationId: String) -> Turn {
        if let turn = turns[conversationId] { return turn }
        let turn = Turn()
        turns[conversationId] = turn
        return turn
    }

    func stop(_ conversationId: String) {
        turns[conversationId]?.task?.cancel()
    }

    /// Run one turn: a new message, or a rewrite of `editing` and everything after it.
    func run(_ conversationId: String, content: String, request: TurnRequest, editing: String? = nil, workspace: String? = nil) {
        let turn = turn(conversationId)
        guard turn.task == nil, let app else { return }

        turn.error = nil
        turn.notices = []
        turn.live = Turn.Live()
        turn.pending = Turn.Pending(content: content, replacing: editing)
        turn.isRunning = true
        running.insert(conversationId)

        let events = app.client.streamTurn(conversationId: conversationId, content: content, request: request, editing: editing)
        turn.task = Task { @MainActor [weak self] in
            var content = ""
            var reasoning = ""
            var flushedAt = Date.distantPast

            @MainActor func flush(force: Bool = false) {
                guard force || Date().timeIntervalSince(flushedAt) > Self.flushInterval else { return }
                turn.live?.content += content
                turn.live?.reasoning += reasoning
                turn.live?.touchedAt = Date()
                content = ""
                reasoning = ""
                flushedAt = Date()
            }

            do {
                for try await event in events {
                    switch event {
                    case .token(let text):
                        content += text
                        flush()
                    case .reasoning(let text):
                        reasoning += text
                        flush()
                    case .toolCall(let name, let arguments):
                        flush(force: true)
                        turn.live?.tools.append(Turn.ToolStep(name: name, arguments: arguments, state: .running, preview: ""))
                    case .toolResult(let name, let ok, let preview):
                        flush(force: true)
                        // Parallel calls to one tool are legal; the result belongs to the newest still running.
                        if let index = turn.live?.tools.lastIndex(where: { $0.name == name && $0.state == .running }) {
                            turn.live?.tools[index].state = ok ? .done : .failed
                            turn.live?.tools[index].preview = preview
                        }
                    case .notice(let message):
                        turn.notices.append(message)
                    case .error(let message):
                        turn.error = message
                    case .done:
                        break
                    }
                }
            } catch {
                if !Task.isCancelled, (error as? URLError)?.code != .cancelled {
                    turn.error = error.localizedDescription
                }
            }
            flush(force: true)

            turn.task = nil
            turn.isRunning = false
            self?.running.remove(conversationId)
            await self?.app?.turnFinished(conversationId, workspace: workspace)
            // Cleared only after the stored messages are back, so nothing flickers.
            turn.live = nil
            turn.pending = nil
        }
    }

    /// Record an error against a conversation that is not running a turn (a failed fork, say).
    func fail(_ conversationId: String, _ message: String) {
        turn(conversationId).error = message
    }
}
