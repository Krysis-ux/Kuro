import SwiftUI
import UniformTypeIdentifiers
import KuroKit

/// Skills are prompt guidance you switch on. Each turn picks the ones the
/// message calls for, up to a budget, so a skill that is on but irrelevant costs nothing.
struct SkillsPane: View {
    @Environment(AppModel.self) private var app
    let overview: ToolsOverview

    @State private var repo = ""
    @State private var importing = false
    @State private var imported: SkillImport?
    @State private var failure: String?
    @State private var expanded: String?

    private static let categoryLabels = [
        "language": "Languages",
        "coding": "Working in a Codebase",
        "practice": "Engineering Practice",
        "design": "Interface and Design",
        "writing": "Writing and Reasoning",
    ]

    var body: some View {
        let skills = overview.skills
        let enabled = Set(skills.enabled)
        let perTurn = skills.budgets?.code.first { $0.effort == "balanced" }?.tokens

        Form {
            Section {
                HStack {
                    Text(enabled.isEmpty ? "None on" : "\(enabled.count) on" + (perTurn.map { " · up to ~\($0) tokens per turn" } ?? ""))
                        .foregroundStyle(.secondary)
                        .help("Switching a skill on makes it available. Each turn picks the ones your message calls for, up to the budget.")
                    Spacer()
                    let all = enabled.count < skills.catalogue.count
                    Button(all ? "Enable All" : "Turn All Off") { setMany(skills.catalogue.map(\.slug), on: all) }
                }
            } footer: {
                Text("Expertise you switch on. Each adds specific instructions to the model's brief — the single biggest help for a small local model. Type `/name` in a message to use one for that turn.")
            }

            addSection

            if !skills.essentials.isEmpty {
                Section {
                    ForEach(skills.essentials) { skill in
                        LabeledContent {
                            Image(systemName: "lock.fill").foregroundStyle(.tertiary)
                        } label: {
                            Text(skill.name)
                            Text(skill.blurb)
                        }
                    }
                } header: {
                    Text("Always On When Coding")
                } footer: {
                    Text("These have no switch: reading a file before editing it, and running the tests rather than describing them, are not preferences.")
                }
            }

            ForEach(categories(skills.catalogue), id: \.0) { category, members in
                Section {
                    ForEach(members) { skill in row(skill, on: enabled.contains(skill.slug)) }
                } header: {
                    HStack {
                        Text(Self.categoryLabels[category] ?? category.capitalized)
                        Spacer()
                        let allOn = members.allSatisfy { enabled.contains($0.slug) }
                        Button(allOn ? "None" : "All") { setMany(members.map(\.slug), on: !allOn) }
                            .buttonStyle(.borderless)
                            .font(.caption)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }

    private var addSection: some View {
        Section {
            HStack {
                TextField("Repository", text: $repo, prompt: Text("github.com/anthropics/skills"))
                    .labelsHidden()
                    .font(.body.monospaced())
                    .onSubmit(importRepo)
                Button(importing ? "Reading…" : "Import", action: importRepo)
                    .disabled(repo.trimmingCharacters(in: .whitespaces).isEmpty || importing)
                    .help("Read every SKILL.md in this repository")
                Button("Upload…", action: upload)
                    .help("Add a SKILL.md from this Mac")
            }
            if let failure { ErrorText(failure) }
            if let imported {
                VStack(alignment: .leading, spacing: 3) {
                    Text(imported.repo).fontWeight(.medium)
                    Text("\(imported.added.count) added" + (imported.skipped.isEmpty ? "" : " · \(imported.skipped.joined(separator: ", ")) already built in"))
                        .font(.caption).foregroundStyle(.secondary)
                    if !imported.needsScripts.isEmpty {
                        Text("These expect scripts that were not imported, so may not work as written: \(imported.needsScripts.joined(separator: ", "))")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            let custom = overview.skills.custom ?? []
            if !custom.isEmpty {
                DisclosureGroup("\(custom.count) skill\(custom.count == 1 ? "" : "s") you added") {
                    ForEach(custom) { skill in
                        HStack {
                            Text(skill.name)
                            Text("/\(skill.slug)").font(.caption.monospaced()).foregroundStyle(.secondary)
                            Spacer()
                            Text(skill.source == "upload" ? "uploaded" : skill.source.replacingOccurrences(of: "https://github.com/", with: ""))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Button(role: .destructive) { remove(skill) } label: { Image(systemName: "trash") }
                                .buttonStyle(.borderless)
                                .help("Remove this skill")
                        }
                    }
                }
            }
        } header: {
            Text("Add Your Own")
        } footer: {
            Text("A repository is read, not cloned: Kuro takes the instructions from each SKILL.md. Scripts a skill ships alongside are not imported.")
        }
    }

    private func row(_ skill: Skill, on: Bool) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Toggle(isOn: Binding(get: { on }, set: { setMany([skill.slug], on: $0) })) {
                Text(skill.name)
                Text(skill.blurb)
            }
            Button(expanded == skill.slug ? "Hide instructions" : "What it tells the model") {
                withAnimation(.snappy) { expanded = expanded == skill.slug ? nil : skill.slug }
            }
            .buttonStyle(.link)
            .font(.caption)
            if expanded == skill.slug {
                CodeSurface(text: skill.instructions, maxHeight: 260)
            }
        }
    }

    private func categories(_ catalogue: [Skill]) -> [(String, [Skill])] {
        var order: [String] = []
        var grouped: [String: [Skill]] = [:]
        for skill in catalogue {
            if grouped[skill.category] == nil { order.append(skill.category) }
            grouped[skill.category, default: []].append(skill)
        }
        return order.map { ($0, grouped[$0]!) }
    }

    /// The whole enabled set is sent, so concurrent toggles cannot disagree.
    private func setMany(_ slugs: [String], on: Bool) {
        var next = overview.skills.enabled
        if on {
            for slug in slugs where !next.contains(slug) { next.append(slug) }
        } else {
            next.removeAll { slugs.contains($0) }
        }
        perform($failure) {
            try await app.client.setSkills(next)
            app.invalidate(.tools)
        }
    }

    private func importRepo() {
        let url = repo.trimmingCharacters(in: .whitespaces)
        guard !url.isEmpty else { return }
        importing = true
        perform($failure) {
            defer { importing = false }
            imported = try await app.client.importSkills(from: url)
            repo = ""
            app.invalidate(.tools)
        }
    }

    private func upload() {
        guard let file = Pick.files(title: "Choose a SKILL.md", types: [UTType(filenameExtension: "md") ?? .plainText, .plainText], multiple: false).first else { return }
        perform($failure) {
            let content = try String(contentsOf: file, encoding: .utf8)
            try await app.client.uploadSkill(filename: file.lastPathComponent, content: content)
            imported = nil
            app.invalidate(.tools)
        }
    }

    private func remove(_ skill: CustomSkill) {
        perform($failure) {
            try await app.client.deleteSkill(skill.slug)
            app.invalidate(.tools)
        }
    }
}
