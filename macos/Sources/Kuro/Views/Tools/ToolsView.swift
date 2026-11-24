import SwiftUI
import KuroKit

// The panes Settings shows under Agent: web search, memory and the built-in tools.
// Skills and MCP have their own files.

// MARK: - Web search

struct SearchPane: View {
    @Environment(AppModel.self) private var app
    let overview: ToolsOverview

    @State private var key = ""
    @State private var baseUrl = ""
    @State private var testing = false
    @State private var result: SearchTest?
    @State private var failure: String?

    var body: some View {
        let search = overview.search
        let selected = search.providers.first { $0.id == search.provider }

        Form {
            Section {
                Picker("Provider", selection: Binding(get: { search.provider }, set: { configure(["provider": $0]) })) {
                    ForEach(search.providers) { provider in
                        VStack(alignment: .leading, spacing: 2) {
                            HStack {
                                Text(provider.name)
                                if !provider.needsApiKey && !provider.needsBaseUrl { Tag("no key") }
                            }
                            Text(provider.note).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                        .tag(provider.id)
                    }
                }
                .pickerStyle(.radioGroup)
                .labelsHidden()
            } header: {
                TitleWithHelp("Search Provider", help: "Turn search on per message with the Web switch in the message box. Kuro searches before the model answers, so it works even on small models.")
            }

            if search.needsApiKey || search.needsBaseUrl {
                Section {
                    if search.needsApiKey {
                        HStack {
                            SecureField("API key", text: $key, prompt: Text(search.hasApiKey ? "A key is stored — paste a new one to replace it" : "Paste the key"))
                            Button("Save") { configure(["apiKey": key]); key = "" }
                                .disabled(key.trimmingCharacters(in: .whitespaces).isEmpty)
                            if search.hasApiKey { Button("Remove") { configure(["apiKey": ""]) } }
                        }
                    }
                    if search.needsBaseUrl {
                        HStack {
                            TextField("Instance URL", text: $baseUrl, prompt: Text(search.baseUrl ?? "http://localhost:8888"))
                            Button("Save") { configure(["baseUrl": baseUrl]); baseUrl = "" }
                                .disabled(baseUrl.trimmingCharacters(in: .whitespaces).isEmpty)
                        }
                    }
                    if let link = selected?.credentialsUrl.flatMap(URL.init(string:)) {
                        Link(search.needsApiKey ? "Get a key" : "How to run one", destination: link)
                    }
                } header: {
                    Text(selected?.name ?? "Settings")
                }
            }

            Section {
                HStack {
                    Button(testing ? "Searching…" : "Test Search", action: test).disabled(testing)
                    HelpTip("Runs a real query against the provider.")
                }
                if let failure { ErrorText(failure) }
                if let result {
                    if result.ok {
                        Label("Working.", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                        ForEach(result.results ?? [], id: \.url) { hit in
                            VStack(alignment: .leading, spacing: 1) {
                                Text(hit.title)
                                Text(hit.url).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                    } else {
                        ErrorText("Not working. \(result.error ?? "")")
                    }
                }
            }
        }
        .settingsForm()
    }

    private func configure(_ patch: KuroClient.Body) {
        perform($failure) {
            try await app.client.configureSearch(patch)
            result = nil
            app.invalidate(.tools)
        }
    }

    private func test() {
        testing = true
        perform($failure) {
            defer { testing = false }
            result = try await app.client.testSearch()
        }
    }
}

// MARK: - Memory

struct MemoryPane: View {
    @Environment(AppModel.self) private var app
    let overview: ToolsOverview

    @State private var memories: [MemoryRecord]?
    @State private var failure: String?

    var body: some View {
        Form {
            Section {
                Toggle(isOn: Binding(get: { overview.memory.preload }, set: { on in
                    perform($failure) {
                        try await app.client.setMemoryPreload(on)
                        app.invalidate(.tools)
                    }
                })) {
                    TitleWithHelp("Preload memories", help: "Puts saved memories in front of the model. Without it, memory only works when the model thinks to look, which small models often do not.")
                }
                LabeledContent("Stored", value: "\(overview.memory.count) \(overview.memory.count == 1 ? "memory" : "memories")")
            } header: {
                TitleWithHelp("Memory", help: "Facts a model was asked to keep. Stored on this Mac and available in every conversation.")
            }

            Section("Remembered") {
                if let failure { ErrorText(failure) }
                if let memories {
                    if memories.isEmpty { Text("Nothing saved yet.").foregroundStyle(.secondary) }
                    ForEach(memories) { memory in
                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(memory.content).textSelection(.enabled)
                                if !memory.tags.isEmpty {
                                    HStack(spacing: 4) { ForEach(memory.tags, id: \.self) { Tag($0) } }
                                }
                            }
                            Spacer()
                            Button(role: .destructive) { forget(memory) } label: { Image(systemName: "trash") }
                                .buttonStyle(.borderless)
                                .help("Forget this")
                        }
                    }
                } else {
                    ProgressView()
                }
            }
        }
        .settingsForm()
        .task(id: app.stamp(.tools)) { memories = (try? await app.client.memories()) ?? memories ?? [] }
    }

    private func forget(_ memory: MemoryRecord) {
        perform($failure) {
            try await app.client.deleteMemory(memory.id)
            app.invalidate(.tools)
        }
    }
}

// MARK: - Built in

struct BuiltinPane: View {
    let overview: ToolsOverview

    var body: some View {
        Form {
            Section {
                ForEach(overview.builtins) { tool in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(tool.name).font(.body.monospaced())
                            Tag(tool.group)
                        }
                        Text(tool.description).font(.callout).foregroundStyle(.secondary).lineLimit(2)
                    }
                }
            } header: {
                TitleWithHelp("What the Model Sees", help: "Offered when their switch is on for a turn. Workspaces add file and command tools, depending on the mode.")
            }
        }
        .settingsForm()
    }
}
