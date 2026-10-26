import SwiftUI
import KuroKit

/// Choosing or opening a workspace: a folder a model can work in.
struct WorkspacesView: View {
    @Environment(AppModel.self) private var app
    @State private var workspaces: [Workspace] = []
    @State private var root = ""
    @State private var name = ""
    @State private var opening = false
    @State private var failure: String?
    @State private var removing: Workspace?

    var body: some View {
        Form {
            Section {
                LabeledContent("Folder") {
                    HStack {
                        Text(root.isEmpty ? "No folder chosen" : root)
                            .font(.callout.monospaced())
                            .foregroundStyle(root.isEmpty ? .tertiary : .primary)
                            .lineLimit(1)
                            .truncationMode(.head)
                        Button("Choose…") {
                            if let path = Pick.folder(title: "Choose a project folder") { choose(path) }
                        }
                    }
                }
                TextField("Name", text: $name, prompt: Text("Named after the folder"))
                    .onSubmit(open)
                if let failure { ErrorText(failure) }
                HStack {
                    Spacer()
                    Button(opening ? "Opening…" : "Open Workspace", action: open)
                        .buttonStyle(.borderedProminent)
                        .disabled(root.isEmpty || opening)
                }
            } header: {
                Text("New Workspace")
            } footer: {
                Text("A model can read this folder, change it, and run its build and tests, depending on the mode you pick. Everything outside it is refused, and so are credentials inside it. Chat can read workspaces but never change them.")
            }

            if !workspaces.isEmpty {
                Section("Workspaces") {
                    ForEach(workspaces) { workspace in
                        Button { app.go(.workspace(workspace.id)) } label: { row(workspace) }
                            .buttonStyle(.plain)
                            .contextMenu {
                                Button("Remove from Kuro…", role: .destructive) { removing = workspace }
                            }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Code")
        .toolbar { BackToolbarItem() }
        .task(id: app.stamp(.workspaces)) {
            workspaces = (try? await app.client.workspaces().workspaces) ?? []
        }
        .confirmationDialog("Remove “\(removing?.name ?? "")” from Kuro?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } })) {
            Button("Remove", role: .destructive) {
                guard let removing else { return }
                Task {
                    try? await app.client.deleteWorkspace(removing.id)
                    app.invalidate(.workspaces)
                }
            }
        } message: {
            Text("The folder and its files are not touched.")
        }
    }

    private func row(_ workspace: Workspace) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "folder.fill").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(workspace.name).fontWeight(.medium)
                Text(workspace.rootPath)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.head)
            }
            Spacer()
            if !workspace.rootExists { Tag("folder missing", tone: .warning) }
            ModeTag(mode: workspace.mode)
            Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
        }
        .contentShape(Rectangle())
    }

    /// Name the workspace after its folder unless a name was typed.
    private func choose(_ path: String) {
        root = path
        if name.trimmingCharacters(in: .whitespaces).isEmpty { name = Naming.leaf(of: path) }
    }

    private func open() {
        guard !root.isEmpty else { return }
        opening = true
        perform($failure) {
            defer { opening = false }
            let created = try await app.client.createWorkspace(name: name.trimmingCharacters(in: .whitespaces), root: root)
            app.invalidate(.workspaces)
            app.go(.workspace(created.id))
        }
    }
}

struct ModeTag: View {
    let mode: WorkspaceMode

    var body: some View {
        Tag(mode.rawValue.capitalized, tone: mode == .bypass ? .warning : mode == .agent ? .live : .neutral)
    }
}

extension WorkspaceMode {
    /// Grey for read-only modes, green for Agent, orange for Bypass.
    var tint: Color {
        switch self {
        case .ask, .plan: .secondary
        case .agent: .green
        case .bypass: .orange
        }
    }

    var systemImage: String {
        switch self {
        case .ask: "bubble.left"
        case .plan: "list.bullet"
        case .agent: "bolt"
        case .bypass: "exclamationmark.shield"
        }
    }

    var emptyLine: String {
        switch self {
        case .ask: "Ask mode. The model cannot see this project — paste in what you want to discuss."
        case .plan: "Plan mode. Ask about this project and the model will read it before answering."
        case .agent: "Agent mode. Describe a change and the model will make it, then build it and run the tests. Every edit can be undone."
        case .bypass: "Bypass mode. Same as Agent, with no limit on which commands it may run."
        }
    }

    var placeholder: String {
        switch self {
        case .ask: "Ask about code…"
        case .plan: "Ask about this project…"
        case .agent, .bypass: "Describe a change…"
        }
    }

    var note: String {
        switch self {
        case .ask: "Ask mode: no access to this folder."
        case .plan: "Plan mode: can read this folder, and cannot change it or run anything."
        case .agent: "Agent mode: can change files here and run build and test commands. Every edit can be undone."
        case .bypass: "Bypass mode: can change files here and run any command, with no allowlist."
        }
    }
}

/// "Back" to the screen this one was opened from, when it was opened from the composer.
struct BackToolbarItem: ToolbarContent {
    @Environment(AppModel.self) private var app

    var body: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            if let back = app.returnRoute {
                Button {
                    app.go(back)
                } label: {
                    Label("Back", systemImage: "chevron.backward")
                }
                .help("Back to where you were writing")
            }
        }
    }
}
