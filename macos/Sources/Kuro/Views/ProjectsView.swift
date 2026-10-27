import SwiftUI
import KuroKit

/// Projects: standing instructions plus a place for related chats. The
/// instructions are added to the model's brief for every chat in the project.
struct ProjectsView: View {
    @Environment(AppModel.self) private var app
    @State private var projects: [Project] = []
    @State private var creating = false
    @State private var name = ""
    @State private var failure: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Say it once. What you write in a project is added to the model's brief for every conversation in it.")
                    .foregroundStyle(.secondary)

                if let failure { ErrorText(failure) }

                if projects.isEmpty {
                    EmptyNote(
                        title: "No projects yet",
                        systemImage: "folder",
                        message: "Worth making as soon as you notice yourself retyping the same context at the top of every chat."
                    )
                    .frame(maxWidth: .infinity, minHeight: 260)
                } else {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 14)], spacing: 14) {
                        ForEach(projects) { project in
                            Button { app.go(.project(project.id)) } label: { card(project) }
                                .buttonStyle(.plain)
                        }
                    }
                }
            }
            .padding(28)
            .frame(maxWidth: 980, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .navigationTitle("Projects")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { creating = true } label: { Label("New Project", systemImage: "plus") }
            }
        }
        .task(id: app.stamp(.projects)) {
            projects = (try? await app.client.projects()) ?? projects
        }
        .alert("New Project", isPresented: $creating) {
            TextField("Kuro, thesis, side project…", text: $name)
            Button("Create", action: create)
            Button("Cancel", role: .cancel) { name = "" }
        }
    }

    private func card(_ project: Project) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Image(systemName: "folder.fill").foregroundStyle(.secondary)
                Text(project.name).font(.headline).lineLimit(1)
            }
            if !project.description.isEmpty {
                Text(project.description).font(.callout).foregroundStyle(.secondary).lineLimit(2)
            }
            Spacer(minLength: 0)
            HStack(spacing: 6) {
                Text("\(project.conversationCount) \(project.conversationCount == 1 ? "chat" : "chats")")
                if !project.instructions.isEmpty { Tag("instructions", systemImage: "sparkles") }
                Spacer()
                Text(Naming.relative(project.updatedAt))
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(16)
        .frame(maxWidth: .infinity, minHeight: 120, alignment: .topLeading)
        .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.primary.opacity(0.07)))
        .contentShape(Rectangle())
    }

    private func create() {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        name = ""
        guard !trimmed.isEmpty else { return }
        perform($failure) {
            let project = try await app.client.createProject(name: trimmed)
            app.invalidate(.projects)
            app.go(.project(project.id))
        }
    }
}

/// One project: its instructions, its default model, and its chats.
struct ProjectDetailView: View {
    @Environment(AppModel.self) private var app
    let projectId: String

    @State private var project: Project?
    @State private var conversations: [Conversation] = []
    @State private var instructions = ""
    @State private var missing = false
    @State private var failure: String?
    @State private var confirmDelete = false

    var body: some View {
        Group {
            if let project {
                form(project)
            } else if missing {
                EmptyNote(title: "That project does not exist", systemImage: "folder.badge.questionmark")
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(project?.name ?? "Project")
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button { app.go(.projects) } label: { Label("Projects", systemImage: "chevron.backward") }
            }
            ToolbarItem(placement: .primaryAction) {
                Button(action: startChat) { Label("New Chat Here", systemImage: "square.and.pencil") }
                    .help("A chat started here inherits the instructions")
            }
        }
        .task(id: app.stamp(.projects)) { await load() }
        .confirmationDialog("Delete “\(project?.name ?? "")”?", isPresented: $confirmDelete) {
            Button("Delete Project", role: .destructive, action: delete)
        } message: {
            Text("Its chats are kept and move back to the top level. The project is only a grouping.")
        }
    }

    private func form(_ project: Project) -> some View {
        let dirty = instructions != project.instructions
        let words = instructions.split(whereSeparator: \.isWhitespace).count

        return Form {
            Section {
                TextEditor(text: $instructions)
                    .font(.body)
                    .frame(minHeight: 160)
                    .scrollContentBackground(.hidden)
                HStack {
                    Text(words > 0 ? "\(words) words" : "Empty — the project is just a folder until you write something here.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    if dirty {
                        Button("Revert") { instructions = project.instructions }
                        Button("Save") { save(["instructions": instructions]) }
                            .buttonStyle(.borderedProminent)
                    }
                }
            } header: {
                Text("Instructions")
            } footer: {
                Text("Added to the model's brief for every chat in this project, ahead of Kuro's own guidance. Concrete beats aspirational: “assume PostgreSQL 16” earns its place, “be helpful” does not.")
            }

            Section {
                LabeledContent("Model") {
                    HStack {
                        ModelPicker(selection: Binding(
                            get: { project.modelId },
                            set: { save(["modelId": $0]) }
                        ))
                        if project.modelId != nil {
                            Button("Clear") { save(["modelId": nil]) }.buttonStyle(.borderless)
                        }
                    }
                }
            } header: {
                Text("Defaults for New Chats")
            } footer: {
                Text(project.modelId == nil ? "Inherits whatever the composer is set to." : "New chats here start on this model.")
            }

            Section("Chats") {
                if conversations.isEmpty {
                    Text("No chats yet. One started here inherits the instructions above.").foregroundStyle(.secondary)
                }
                ForEach(conversations) { conversation in
                    HStack {
                        Button { app.go(.chat(conversation.id)) } label: {
                            HStack {
                                Text(conversation.title)
                                Spacer()
                                Text(Naming.relative(conversation.updatedAt)).foregroundStyle(.secondary)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        Button("Remove") { removeChat(conversation.id) }
                            .buttonStyle(.borderless)
                            .help("Move this chat out of the project")
                    }
                }
            }

            if let failure { Section { ErrorText(failure) } }

            Section {
                Button("Delete Project…", role: .destructive) { confirmDelete = true }
            }
        }
        .formStyle(.grouped)
    }

    private func load() async {
        do {
            let (loaded, chats) = try await app.client.project(projectId)
            if project == nil || instructions == project?.instructions { instructions = loaded.instructions }
            project = loaded
            conversations = chats
        } catch {
            if project == nil { missing = true }
        }
    }

    private func save(_ patch: KuroClient.Body) {
        perform($failure) {
            try await app.client.updateProject(projectId, patch)
            app.invalidate(.projects)
        }
    }

    private func startChat() {
        perform($failure) {
            let conversation = try await app.client.createConversation(model: project?.modelId)
            try await app.client.moveConversation(conversation.id, toProject: projectId)
            app.invalidate(.projects, .conversations)
            app.go(.chat(conversation.id))
        }
    }

    private func removeChat(_ id: String) {
        perform($failure) {
            try await app.client.moveConversation(id, toProject: nil)
            app.invalidate(.projects, .conversations)
        }
    }

    private func delete() {
        perform($failure) {
            try await app.client.deleteProject(projectId)
            app.invalidate(.projects, .conversations)
            app.go(.projects)
        }
    }
}
