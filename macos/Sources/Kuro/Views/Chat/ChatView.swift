import SwiftUI
import KuroKit

struct ChatView: View {
    @Environment(AppModel.self) private var app
    let conversationId: String?
    @State private var failure: String?

    var body: some View {
        @Bindable var prefs = app.prefs
        let turn = conversationId.map { app.turns.turn($0) }
        let stored = conversationId.flatMap { app.messages[$0] } ?? []
        let history = turn?.display(stored, conversationId: conversationId ?? "") ?? stored
        // An error counts as content: a turn that failed before anything was
        // stored must still say so.
        let isEmpty = conversationId == nil
            || (history.isEmpty && turn?.isRunning != true && turn?.error == nil && turn?.notices.isEmpty != false)

        let composer = Composer(
            draftKey: "chat:\(conversationId ?? "new")",
            coding: false,
            model: $prefs.chatModel,
            effort: $prefs.chatEffort,
            effortNote: "More effort means longer answers and more room to search before replying.",
            toggles: [
                ComposerToggle(
                    id: "web", command: "web", label: "Web", systemImage: "globe", isOn: $prefs.webSearch,
                    help: "Search the web before answering. Queries leave this Mac."
                ),
                ComposerToggle(
                    id: "folders", command: "folders", label: "Folders", systemImage: "folder", isOn: $prefs.readFolders,
                    help: "Let the model read the folders opened in Code. Reading only — chat never changes a file."
                ),
            ],
            isRunning: turn?.isRunning ?? false,
            onSend: { content, skills in send(content, skills: skills) },
            onStop: { if let conversationId { app.turns.stop(conversationId) } }
        )

        Group {
            if isEmpty {
                welcome(composer)
            } else if let conversationId, let turn {
                Transcript(
                    conversationId: conversationId,
                    history: history,
                    turn: turn,
                    onFork: fork,
                    onEdit: { messageId, content in send(content, skills: [], editing: messageId) }
                )
                .safeAreaInset(edge: .bottom) {
                    composer
                        .frame(maxWidth: 760)
                        .padding(.horizontal, 20)
                        .padding(.bottom, 14)
                }
            }
        }
        .navigationTitle(title)
        .task(id: conversationId) {
            if let conversationId { await app.loadMessages(conversationId) }
        }
    }

    private var title: String {
        guard let conversationId else { return "New Chat" }
        return app.conversations.first { $0.id == conversationId }?.title ?? "Chat"
    }

    private func welcome(_ composer: Composer) -> some View {
        VStack(spacing: 14) {
            Spacer()
            KuroMark(size: 46)
            Text("Kuro")
                .font(.system(size: 30, weight: .semibold, design: .default))
            Text(welcomeLine)
                .foregroundStyle(.secondary)
                .padding(.bottom, 14)
            composer.frame(maxWidth: 700)
            if let failure { ErrorText(failure) }
            Spacer()
            Spacer()
        }
        .padding(.horizontal, 32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var welcomeLine: String {
        if !app.models.models.isEmpty { return "Ask anything. Everything runs on this Mac." }
        if !app.models.remote.isEmpty { return "No local models yet — a connected provider is standing in." }
        return "Install a model to get started, or connect a provider."
    }

    /// Send a message, creating the conversation first when there is none yet.
    private func send(_ content: String, skills: [String], editing: String? = nil) {
        let prefs = app.prefs
        Task {
            var target = conversationId
            if target == nil {
                do {
                    target = try await app.client.createConversation(model: prefs.chatModel).id
                    app.invalidate(.conversations)
                } catch {
                    failure = error.localizedDescription
                    return
                }
            }
            guard let target else { return }
            app.turns.run(
                target,
                content: content,
                request: TurnRequest(
                    model: prefs.chatModel,
                    effort: prefs.chatEffort,
                    tools: prefs.chatToolGroups,
                    webSearch: prefs.webSearch,
                    skills: skills
                ),
                editing: editing
            )
            if conversationId == nil { app.go(.chat(target)) }
        }
    }

    private func fork(_ messageId: String) {
        guard let conversationId else { return }
        Task {
            if let branch = await app.fork(conversationId, at: messageId) { app.go(.chat(branch.id)) }
        }
    }
}

/// The scrolling conversation. Shared by Chat and Code.
struct Transcript: View {
    let conversationId: String
    let history: [Message]
    let turn: Turn
    let onFork: (String) -> Void
    let onEdit: (String, String) -> Void
    /// Opens a file the turn touched. Only Code has somewhere to show one.
    var onOpenPath: ((String) -> Void)?

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 26) {
                    ForEach(history) { message in
                        MessageRow(
                            message: message,
                            // A turn in progress, and the stand-in for a message
                            // being sent, have nothing to fork or edit yet.
                            actionable: message.id != "pending" && !turn.isRunning,
                            onFork: { onFork(message.id) },
                            onEdit: { onEdit(message.id, $0) },
                            onOpenPath: onOpenPath
                        )
                        .equatable()
                    }

                    LiveTurnRow(turn: turn, onOpenPath: onOpenPath)

                    ForEach(Array(turn.notices.enumerated()), id: \.offset) { _, notice in
                        Label(notice, systemImage: "info.circle")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    if let error = turn.error { ErrorText(error) }

                    Color.clear.frame(height: 1).id("bottom")
                }
                .frame(maxWidth: 760)
                .padding(.horizontal, 28)
                .padding(.top, 24)
                .padding(.bottom, 12)
                .frame(maxWidth: .infinity)
            }
            .defaultScrollAnchor(.bottom)
            .onChange(of: history.count) {
                withAnimation(.snappy) { proxy.scrollTo("bottom", anchor: .bottom) }
            }
        }
    }
}

/// The reply being generated. Kept in its own view so streamed text only
/// re-renders this row.
private struct LiveTurnRow: View {
    let turn: Turn
    let onOpenPath: ((String) -> Void)?

    var body: some View {
        if let live = turn.live, turn.isRunning {
            VStack(alignment: .leading, spacing: 12) {
                if !live.tools.isEmpty {
                    ToolActivity(steps: live.tools.map { ($0.name, $0.arguments, $0.state, $0.preview) }, startsOpen: true, onOpenPath: onOpenPath)
                }
                if !live.reasoning.isEmpty {
                    Reasoning(text: live.reasoning, startsOpen: true)
                }
                if !live.content.isEmpty {
                    MarkdownView(text: live.content)
                }
                // Shown for the whole turn, not only before the first token: the
                // gaps between tool calls are when people wonder if it stopped.
                Working(
                    label: live.tools.contains { $0.state == .running } ? "Working" : live.content.isEmpty ? "Thinking" : "Still going",
                    since: live.touchedAt
                )
            }
        }
    }
}

/// The heartbeat under a turn in progress. Counts from the last event, and
/// stays quiet for the first few seconds.
private struct Working: View {
    let label: String
    let since: Date

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let quiet = Int(context.date.timeIntervalSince(since))
            HStack(spacing: 8) {
                Image(systemName: "ellipsis")
                    .symbolEffect(.variableColor.iterative, options: .repeating)
                Text("\(label)…")
                if quiet >= 5 {
                    Text("\(quiet)s").monospacedDigit().foregroundStyle(.tertiary)
                }
            }
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        .accessibilityLabel(label)
    }
}
