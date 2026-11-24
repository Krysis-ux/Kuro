import SwiftUI
import KuroKit

/// The specialists a coding turn can delegate to: Kuro's own, and ones the user
/// wrote. Each runs in a fresh context and returns a report; the ones that
/// cannot edit are given no write tools.
struct SubagentsPage: View {
    @Environment(AppModel.self) private var app
    @State private var editing: EditorRequest?
    @State private var deleting: SubagentInfo?
    @State private var failure: String?

    /// What the editor opens with: nothing, an agent to change, or one to copy.
    struct EditorRequest: Identifiable {
        let id = UUID()
        let agent: SubagentInfo?
        let duplicate: Bool
    }

    var body: some View {
        let agents = app.tools?.agents ?? []
        let yours = agents.filter(\.isCustom)
        let builtIn = agents.filter { !$0.isCustom }

        SettingsPage(pane: .subagents) {
            Button { editing = EditorRequest(agent: nil, duplicate: false) } label: {
                Label("New", systemImage: "plus")
            }
            .buttonStyle(PrimaryButtonStyle())
            .disabled(app.tools == nil)
        } content: {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                        Text("Used in Code. Ask for one by name: “have the reviewer check this”.")
                    }
                    .font(.callout)
                    .foregroundStyle(.secondary)

                    if let failure { ErrorText(failure) }

                    heading("Yours", count: yours.count)
                    if yours.isEmpty {
                        emptyYours
                    } else {
                        grid(yours)
                    }

                    heading("Built in", count: builtIn.count)
                    grid(builtIn)
                }
                .padding(20)
                .frame(maxWidth: 760)
                .frame(maxWidth: .infinity)
            }
        }
        #if DEBUG
        .onReceive(NotificationCenter.default.publisher(for: .init("kuro.debug.subagents"))) { note in
            let slug = note.object as? String ?? ""
            editing = EditorRequest(agent: agents.first { $0.slug == slug }, duplicate: false)
        }
        #endif
        .sheet(item: $editing) { request in
            SubagentEditor(original: request.agent, duplicate: request.duplicate) { editing = nil }
        }
        .confirmationDialog("Delete “\(deleting?.name ?? "")”?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Delete", role: .destructive) {
                guard let agent = deleting else { return }
                perform($failure) {
                    try await app.client.deleteAgent(agent.slug)
                    app.invalidate(.tools)
                }
            }
        }
    }

    private func heading(_ title: String, count: Int) -> some View {
        HStack(spacing: 6) {
            Text(title).font(.system(size: 13, weight: .medium))
            Text("\(count)").foregroundStyle(.tertiary)
        }
        .padding(.top, 4)
    }

    private var emptyYours: some View {
        VStack(spacing: 10) {
            Text("Make a specialist for work you repeat.").foregroundStyle(.secondary)
            Button { editing = EditorRequest(agent: nil, duplicate: false) } label: {
                Label("New subagent", systemImage: "plus")
            }
            .buttonStyle(SubtleButtonStyle())
        }
        .font(.callout)
        .frame(maxWidth: .infinity)
        .padding(.vertical, 22)
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(Palette.line, style: StrokeStyle(lineWidth: 1, dash: [5, 4]))
        )
    }

    private func grid(_ agents: [SubagentInfo]) -> some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 300), spacing: 12)], spacing: 12) {
            ForEach(agents) { agent in
                SubagentCard(agent: agent) {
                    if agent.isCustom {
                        Button("Edit…") { editing = EditorRequest(agent: agent, duplicate: false) }
                        Button("Duplicate…") { editing = EditorRequest(agent: agent, duplicate: true) }
                        Divider()
                        Button("Delete…", role: .destructive) { deleting = agent }
                    } else {
                        Button("Duplicate and Edit…") { editing = EditorRequest(agent: agent, duplicate: true) }
                    }
                }
            }
        }
    }
}

private struct SubagentCard<Actions: View>: View {
    let agent: SubagentInfo
    @ViewBuilder var actions: Actions

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: Self.symbol(agent))
                .font(.system(size: 15))
                .frame(width: 34, height: 34)
                .background(Palette.sunken, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous).strokeBorder(Palette.line))

            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Text(agent.name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                    Tag(agent.writes ? "Can edit" : "Read-only", tone: agent.writes ? .neutral : .live)
                    Spacer(minLength: 4)
                    if !agent.when.isEmpty { HelpTip(agent.when) }
                    Menu { actions } label: {
                        Image(systemName: "ellipsis").foregroundStyle(.secondary)
                    }
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .fixedSize()
                    .help("More")
                }
                if !agent.blurb.isEmpty {
                    Text(agent.blurb)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !agent.skills.isEmpty {
                    FlowLayout(spacing: 4) {
                        ForEach(agent.skills, id: \.self) { skill in
                            Text(skill)
                                .font(.caption2)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .foregroundStyle(.secondary)
                                .background(Palette.hover, in: Capsule())
                        }
                    }
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card()
        .contextMenu { actions }
    }

    static func symbol(_ agent: SubagentInfo) -> String {
        switch agent.slug {
        case "explore": "binoculars"
        case "frontend": "paintbrush"
        case "backend": "server.rack"
        case "debug": "ladybug"
        case "test": "checkmark.seal"
        case "review": "eye"
        case "architect": "square.3.layers.3d"
        case "refactor": "arrow.triangle.2.circlepath"
        default: agent.writes ? "person.crop.circle.badge.plus" : "person.crop.circle"
        }
    }
}

// MARK: - Editor

/// Create, change or copy a subagent.
private struct SubagentEditor: View {
    @Environment(AppModel.self) private var app
    let original: SubagentInfo?
    /// Copying an agent (a built-in, usually) into a new one of the user's.
    let duplicate: Bool
    let onClose: () -> Void

    @State private var name = ""
    @State private var blurb = ""
    @State private var when = ""
    @State private var brief = ""
    @State private var writes = false
    @State private var skills: [String] = []
    @State private var skillQuery = ""
    @State private var saving = false
    @State private var failure: String?

    private static let maxSkills = 8

    private var isEdit: Bool { original != nil && !duplicate }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(isEdit ? "Edit Subagent" : "New Subagent").font(.system(size: 16, weight: .semibold))
                Spacer()
            }
            .padding([.horizontal, .top], 20)
            .padding(.bottom, 12)

            Form {
                Section {
                    TextField("Name", text: $name, prompt: Text("Docs writer"))
                    TextField("Description", text: $blurb, prompt: Text("Writes and updates documentation."))
                    TextField(text: $when, prompt: Text("Use for READMEs, guides and changelogs.")) {
                        TitleWithHelp("When to use", help: "The coder reads this to decide when to hand work over.")
                    }
                    Toggle(isOn: $writes) {
                        TitleWithHelp("Can edit files", help: "Off gives it read-only tools. Right for reviewers, explorers and planners.")
                    }
                }

                Section {
                    TextEditor(text: $brief)
                        .font(.body)
                        .frame(minHeight: 120)
                        .scrollContentBackground(.hidden)
                } header: {
                    TitleWithHelp("Instructions", help: "Who it is and how it works. It cannot see the conversation, only the task it is handed.")
                }

                Section {
                    if !skills.isEmpty {
                        FlowLayout(spacing: 4) {
                            ForEach(skills, id: \.self) { slug in
                                Button { skills.removeAll { $0 == slug } } label: {
                                    HStack(spacing: 3) {
                                        Text(skillName(slug))
                                        Image(systemName: "xmark").font(.system(size: 8, weight: .bold))
                                    }
                                    .font(.caption)
                                    .padding(.horizontal, 7)
                                    .padding(.vertical, 3)
                                    .background(Palette.selected, in: Capsule())
                                }
                                .buttonStyle(.plain)
                                .help("Remove")
                            }
                        }
                    }
                    TextField("Find a skill", text: $skillQuery, prompt: Text("Find a skill"))
                        .labelsHidden()
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(matchingSkills, id: \.slug) { option in
                                let on = skills.contains(option.slug)
                                Button { toggle(option.slug) } label: {
                                    HStack(spacing: 8) {
                                        Image(systemName: on ? "checkmark.circle.fill" : "circle")
                                            .foregroundStyle(on ? .primary : .tertiary)
                                        Text(option.name).lineLimit(1)
                                        Spacer()
                                    }
                                    .padding(.vertical, 5)
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .disabled(!on && skills.count >= Self.maxSkills)
                                .help(option.blurb)
                            }
                        }
                    }
                    .frame(height: 140)
                } header: {
                    TitleWithHelp("Skills \(skills.count)/\(Self.maxSkills)", help: "Know-how added to its instructions, on top of the skills the project's language brings in.")
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)

            HStack {
                if let failure { ErrorText(failure).lineLimit(2) }
                Spacer()
                Button("Cancel", action: onClose)
                    .buttonStyle(SubtleButtonStyle())
                    .keyboardShortcut(.cancelAction)
                Button(saving ? "Saving…" : isEdit ? "Save" : "Create", action: save)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canSave || saving)
            }
            .padding(20)
        }
        .frame(width: 540, height: 680)
        .background(Palette.canvas)
        .onAppear(perform: prefill)
    }

    private struct SkillOption {
        let slug: String
        let name: String
        let blurb: String
    }

    /// Essentials first: an agent works in its own context and does not get them by default.
    private var allSkills: [SkillOption] {
        guard let tools = app.tools else { return [] }
        let essentials = tools.skills.essentials.map { SkillOption(slug: $0.slug, name: $0.name, blurb: $0.blurb) }
        let catalogue = tools.skills.catalogue.map { SkillOption(slug: $0.slug, name: $0.name, blurb: $0.blurb) }
        return essentials + catalogue.filter { option in !essentials.contains { $0.slug == option.slug } }
    }

    private var matchingSkills: [SkillOption] {
        let query = skillQuery.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return allSkills }
        return allSkills.filter { $0.name.localizedCaseInsensitiveContains(query) || $0.slug.localizedCaseInsensitiveContains(query) }
    }

    private func skillName(_ slug: String) -> String {
        allSkills.first { $0.slug == slug }?.name ?? slug
    }

    private func toggle(_ slug: String) {
        if skills.contains(slug) {
            skills.removeAll { $0 == slug }
        } else if skills.count < Self.maxSkills {
            skills.append(slug)
        }
    }

    private var canSave: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty && !brief.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func prefill() {
        guard let original else { return }
        name = duplicate ? "\(original.name) copy" : original.name
        blurb = original.blurb
        when = original.when
        brief = original.brief ?? ""
        writes = original.writes
        skills = original.skillSlugs ?? []
    }

    private func save() {
        guard canSave, !saving else { return }
        let body: KuroClient.Body = [
            "name": name.trimmingCharacters(in: .whitespaces),
            "blurb": blurb,
            "when": when,
            "brief": brief,
            "writes": writes,
            "skills": skills,
        ]
        saving = true
        perform($failure) {
            defer { saving = false }
            if isEdit, let original {
                try await app.client.updateAgent(original.slug, body)
            } else {
                try await app.client.createAgent(body)
            }
            app.invalidate(.tools)
            onClose()
        }
    }
}
