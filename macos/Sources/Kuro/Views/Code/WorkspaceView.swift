import SwiftUI
import KuroKit

/// Working in one workspace: files and changes on the left, the conversation in
/// the middle, and what is running on the right.
struct WorkspaceView: View {
    @Environment(AppModel.self) private var app
    let workspaceId: String

    @State private var detail: WorkspaceDetail?
    @State private var missing = false
    @State private var conversationId: String?
    @State private var openFile: String?
    @State private var processes: [RunningProcess] = []
    @State private var failure: String?

    /// The conversation never gets narrower than this; side panels give way first.
    private static let conversationMinimum: CGFloat = 380

    var body: some View {
        Group {
            if let workspace = detail?.workspace {
                content(workspace)
            } else if missing {
                VStack {
                    EmptyNote(title: "That workspace is gone", systemImage: "folder.badge.questionmark")
                    Button("Back to Workspaces") { app.go(.code) }
                }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: app.stamp(.workspace(workspaceId))) { await load() }
        .task(id: workspaceId) { await pollProcesses() }
    }

    private func content(_ workspace: Workspace) -> some View {
        @Bindable var prefs = app.prefs
        let live = processes.filter(\.running).count

        // A plain HStack with drag handles: HSplitView sizes itself from its
        // children's ideal widths and pushed the window's sidebar off screen.
        // Side panels shrink together when the window is too narrow for them.
        return GeometryReader { proxy in
            let panels = (prefs.filesOpen ? prefs.filesWidth : 0)
                + (openFile != nil ? prefs.viewerWidth : 0)
                + (prefs.runningOpen ? prefs.runningWidth : 0)
            let room = proxy.size.width - Self.conversationMinimum - 24
            let scale = panels > 0 ? min(1, max(0, room) / panels) : 1

            HStack(spacing: 0) {
                if prefs.filesOpen {
                    WorkspaceSidebar(workspace: workspace, onOpen: { openFile = $0 })
                        .frame(width: prefs.filesWidth * scale)
                    ResizeHandle(width: $prefs.filesWidth)
                }
                if let openFile {
                    FileViewer(workspaceId: workspaceId, path: openFile) { self.openFile = nil }
                        .frame(width: prefs.viewerWidth * scale)
                    ResizeHandle(width: $prefs.viewerWidth, range: 260...900, standard: 440)
                }
                conversation(workspace)
                    .frame(maxWidth: .infinity)
                // Not `.inspector`: one that is open as the view appears sends AppKit into a layout loop.
                if prefs.runningOpen {
                    ResizeHandle(width: $prefs.runningWidth, range: 300...760, standard: 420, panelOnRight: true)
                    RunningPanel(workspace: workspace, processes: processes, view: $prefs.runningView) {
                        Task { await refreshProcesses() }
                    }
                    .frame(width: prefs.runningWidth * scale)
                    .background(.background.secondary)
                }
            }
        }
        .navigationTitle(workspace.name)
        .navigationSubtitle(workspace.rootPath)
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button { app.go(.code) } label: { Label("Workspaces", systemImage: "chevron.backward") }
                    .help("All workspaces")
            }
            ToolbarItemGroup(placement: .primaryAction) {
                Toggle(isOn: $prefs.filesOpen) { Label("Files", systemImage: "sidebar.left") }
                    .help(prefs.filesOpen ? "Hide files and changes" : "Show files and changes")
                conversationsMenu
                Button {
                    prefs.runningView = .terminal
                    prefs.runningOpen = true
                } label: { Label("Terminal", systemImage: "terminal") }
                    .help("Run a command and read its output")
                Button {
                    prefs.runningView = .browser
                    prefs.runningOpen = true
                } label: { Label("Browser", systemImage: "globe") }
                    .help("Look at whatever this workspace is serving")
                Button {
                    prefs.runningOpen.toggle()
                } label: {
                    Label(live > 0 ? "\(live) Running" : "Running", systemImage: live > 0 ? "play.circle.fill" : "play.circle")
                }
                .help(live > 0 ? "\(live) running" : "Nothing is running")
            }
        }
    }

    private var conversationsMenu: some View {
        Menu {
            Button("New Conversation", systemImage: "plus") { startConversation() }
            if let conversations = detail?.conversations, !conversations.isEmpty {
                Divider()
                ForEach(conversations) { conversation in
                    Button {
                        conversationId = conversation.id
                        Task { await app.loadMessages(conversation.id) }
                    } label: {
                        if conversation.id == conversationId {
                            Label(conversation.title, systemImage: "checkmark")
                        } else {
                            Text(conversation.title)
                        }
                    }
                }
            }
        } label: {
            Label("Conversations", systemImage: "bubble.left.and.bubble.right")
        }
        .help("Conversations in this workspace")
    }

    @ViewBuilder private func conversation(_ workspace: Workspace) -> some View {
        @Bindable var prefs = app.prefs
        let turn = conversationId.map { app.turns.turn($0) }
        let stored = conversationId.flatMap { app.messages[$0] } ?? []
        let history = turn?.display(stored, conversationId: conversationId ?? "") ?? stored
        let empty = history.isEmpty && turn?.isRunning != true && turn?.error == nil && turn?.notices.isEmpty != false

        Group {
            if empty || conversationId == nil {
                VStack(spacing: 12) {
                    Image(systemName: workspace.mode.systemImage)
                        .font(.system(size: 28, weight: .light))
                        .foregroundStyle(.tertiary)
                    Text(workspace.mode.emptyLine)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: 420)
                    if let failure { ErrorText(failure) }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let conversationId, let turn {
                Transcript(
                    conversationId: conversationId,
                    history: history,
                    turn: turn,
                    onFork: { messageId in
                        Task {
                            if let branch = await app.fork(conversationId, at: messageId) {
                                self.conversationId = branch.id
                                await app.loadMessages(branch.id)
                                app.invalidate(.workspace(workspaceId))
                            }
                        }
                    },
                    onEdit: { messageId, content in send(content, skills: [], editing: messageId) },
                    onOpenPath: { openFile = $0 }
                )
            }
        }
        .safeAreaInset(edge: .bottom) {
            Composer(
                draftKey: "code:\(workspaceId)",
                coding: true,
                model: $prefs.codeModel,
                effort: $prefs.codeEffort,
                effortNote: "More effort means more rounds of reading, editing and running, and pulls in the skills that match this project. Ultracode brings all of them and lets a turn run as long as it needs.",
                leading: { compact in AnyView(ModeSwitch(current: workspace.mode, modes: app.workspaceModes, compact: compact) { setMode($0) }) },
                placeholder: workspace.mode.placeholder,
                hint: workspace.mode.note,
                disabledReason: workspace.rootExists ? nil : "This folder is no longer on disk, so nothing here can run.",
                isRunning: turn?.isRunning ?? false,
                onSend: { content, skills in send(content, skills: skills) },
                onStop: { if let conversationId { app.turns.stop(conversationId) } }
            )
            .frame(maxWidth: 820)
            .padding(.horizontal, 16)
            .padding(.bottom, 12)
        }
    }

    // MARK: Actions

    private func load() async {
        do {
            let loaded = try await app.client.workspace(workspaceId)
            detail = loaded
            // Pick up the most recent conversation, so reopening resumes it.
            if conversationId == nil { conversationId = loaded.conversations.first?.id }
            if let conversationId { await app.loadMessages(conversationId) }
        } catch {
            if detail == nil { missing = true }
        }
    }

    private func refreshProcesses() async {
        if let list = try? await app.client.processes(workspaceId) { processes = list }
    }

    /// Polls even while the panel is shut, and opens it when something new starts —
    /// but only on that change, so closing it while a server runs keeps it closed.
    private func pollProcesses() async {
        var previous: Int?
        while !Task.isCancelled {
            await refreshProcesses()
            let live = processes.filter(\.running).count
            if let previous, live > previous, !app.prefs.runningOpen { app.prefs.runningOpen = true }
            previous = live
            try? await Task.sleep(for: .seconds(2))
        }
    }

    private func setMode(_ mode: WorkspaceMode) {
        perform($failure) {
            try await app.client.updateWorkspace(workspaceId, ["mode": mode.rawValue])
            app.invalidate(.workspace(workspaceId), .workspaces)
        }
    }

    private func startConversation() {
        perform($failure) {
            let created = try await app.client.newWorkspaceConversation(workspaceId)
            conversationId = created.id
            app.invalidate(.workspace(workspaceId))
        }
    }

    private func send(_ content: String, skills: [String], editing: String? = nil) {
        let prefs = app.prefs
        perform($failure) {
            var target = conversationId
            if target == nil {
                target = try await app.client.newWorkspaceConversation(workspaceId).id
                conversationId = target
            }
            guard let target else { return }
            // A coding turn's tools are the workspace's own; web and memory stay off.
            app.turns.run(
                target,
                content: content,
                request: TurnRequest(model: prefs.codeModel, effort: prefs.codeEffort, tools: [], skills: skills),
                editing: editing,
                workspace: workspaceId
            )
        }
    }
}

/// The mode is the permission, chosen right before sending.
private struct ModeSwitch: View {
    let current: WorkspaceMode
    let modes: [WorkspaceModeInfo]
    let compact: Bool
    let onChange: (WorkspaceMode) -> Void

    var body: some View {
        Menu {
            Picker("Mode", selection: Binding(get: { current }, set: onChange)) {
                ForEach(WorkspaceMode.allCases, id: \.self) { mode in
                    let info = modes.first { $0.id == mode }
                    Label {
                        Text(info?.label ?? mode.rawValue.capitalized)
                        Text(info?.blurb ?? mode.note)
                    } icon: {
                        Image(systemName: mode.systemImage)
                    }
                    .tag(mode)
                }
            }
            .pickerStyle(.inline)
        } label: {
            HStack(spacing: 4) {
                Image(systemName: current.systemImage)
                if !compact { Text(current.rawValue.capitalized) }
                Image(systemName: "chevron.down").font(.caption2)
            }
            .font(.callout)
            .foregroundStyle(current.tint)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(current.tint.opacity(0.13), in: Capsule())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help(modes.first { $0.id == current }.map { "\($0.label) — \($0.blurb)" } ?? current.note)
    }
}
