import SwiftUI
import KuroKit

/// Model Context Protocol servers: the connected ones, and a curated marketplace.
struct McpPane: View {
    @Environment(AppModel.self) private var app

    @State private var servers: [McpServer]?
    @State private var registry: [McpRegistryEntry] = []
    @State private var connecting = false
    @State private var query = ""
    @State private var adding: AddRequest?
    @State private var failure: String?

    struct AddRequest: Identifiable {
        let id = UUID()
        let prefill: McpRegistryEntry?
    }

    var body: some View {
        Form {
            Section {
                if let servers {
                    if servers.isEmpty {
                        Text("None connected. Add one, or pick from the marketplace below.").foregroundStyle(.secondary)
                    }
                    ForEach(servers) { ServerRow(server: $0) }
                } else {
                    ProgressView()
                }
                if let failure { ErrorText(failure) }
            } header: {
                HStack {
                    Text("Connected Servers")
                    Spacer()
                    Button(connecting ? "Connecting…" : "Reconnect All") { Task { await load(connect: true) } }
                        .buttonStyle(.borderless)
                        .disabled(connecting)
                    Button("Add Server…") { adding = AddRequest(prefill: nil) }
                        .buttonStyle(.borderless)
                }
            }

            Section {
                TextField("Search", text: $query, prompt: Text("Search servers…"))
                    .labelsHidden()
                let matches = filtered
                if matches.isEmpty {
                    Text(query.isEmpty ? "Nothing to show." : "Nothing matched “\(query)”. Any MCP server works — use Add Server.")
                        .foregroundStyle(.secondary)
                }
                ForEach(matches) { entry in
                    MarketplaceRow(entry: entry) { adding = AddRequest(prefill: entry) }
                }
            } header: {
                Text("Marketplace")
            } footer: {
                Text("A curated list rather than a directory: each is keyless or run by the people who own what it connects to.")
            }
        }
        .formStyle(.grouped)
        .task(id: app.stamp(.mcp)) { await load(connect: servers == nil) }
        .sheet(item: $adding) { request in
            AddServerSheet(prefill: request.prefill, registry: registry) {
                adding = nil
                app.invalidate(.mcp, .tools)
            }
        }
    }

    /// Name, blurb and detail all count: "browser" should find Playwright.
    private var filtered: [McpRegistryEntry] {
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !needle.isEmpty else { return registry }
        return registry.filter { [$0.name, $0.blurb, $0.detail, $0.slug].contains { $0.lowercased().contains(needle) } }
    }

    private func load(connect: Bool) async {
        connecting = connect
        defer { connecting = false }
        do {
            servers = try await app.client.mcpServers(connect: connect)
            registry = try await app.client.mcpRegistry()
            failure = nil
        } catch {
            failure = error.localizedDescription
        }
    }
}

private struct ServerRow: View {
    @Environment(AppModel.self) private var app
    let server: McpServer

    @State private var showTools = false
    @State private var editingToken = false
    @State private var token = ""
    @State private var busy = false
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                StatusDot(color: StatusDot.color(for: server.status))
                Text(server.name).fontWeight(.medium)
                Tag(server.transport)
                let count = server.toolCount ?? server.tools.count
                if count > 0 {
                    Button("\(count) \(count == 1 ? "tool" : "tools")") { withAnimation(.snappy) { showTools.toggle() } }
                        .buttonStyle(.link)
                        .font(.caption)
                }
                if server.hasAuth { Tag("key", systemImage: "key").help("A token is stored outside the database") }
                Spacer()
                actions
            }
            Text(server.address).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            if server.status == "error", let error = server.lastError { ErrorText(error) }
            if let failure { ErrorText(failure) }

            if showTools {
                ForEach(server.tools) { tool in
                    VStack(alignment: .leading, spacing: 1) {
                        Text(tool.name).font(.callout.monospaced())
                        Text(tool.description).font(.caption).foregroundStyle(.secondary)
                    }
                    .padding(.leading, 13)
                }
            }

            if editingToken {
                HStack {
                    SecureField("Bearer token", text: $token)
                    Button("Save and Reconnect") {
                        act { try await app.client.setMcpToken(server.id, token) }
                        token = ""
                        editingToken = false
                    }
                    .disabled(token.trimmingCharacters(in: .whitespaces).isEmpty)
                    Button("Cancel") { editingToken = false }
                }
            }
        }
        .padding(.vertical, 2)
        .opacity(server.enabled ? 1 : 0.6)
    }

    private var actions: some View {
        HStack(spacing: 6) {
            Button { act { try await app.client.refreshMcpServer(server.id) } } label: {
                if busy { ProgressView().controlSize(.mini) } else { Image(systemName: "arrow.clockwise") }
            }
            .help("Reconnect")
            Button { editingToken.toggle() } label: { Image(systemName: "key") }
                .help(server.hasAuth ? "Replace the token" : "Add a token")
            Toggle("Enabled", isOn: Binding(get: { server.enabled }, set: { on in act { try await app.client.setMcpEnabled(server.id, on) } }))
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.mini)
            Button(role: .destructive) { act { try await app.client.deleteMcpServer(server.id) } } label: { Image(systemName: "trash") }
                .help("Remove")
        }
        .buttonStyle(.borderless)
    }

    private func act(_ work: @escaping () async throws -> Void) {
        busy = true
        perform($failure) {
            defer { busy = false }
            try await work()
            app.invalidate(.mcp, .tools)
        }
    }
}

private struct MarketplaceRow: View {
    let entry: McpRegistryEntry
    let onAdd: () -> Void
    @State private var more = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(entry.name).fontWeight(.medium)
                switch entry.requirement {
                case "none": Tag("works now", tone: .live)
                case "api_key": Tag("needs a key", tone: .warning)
                case "local_runtime": Tag("runs locally")
                default: EmptyView()
                }
                Spacer()
                if let docs = URL(string: entry.homepage) { Link("Docs", destination: docs).font(.caption) }
                Button(entry.installed ? "Installed" : "Add…", action: onAdd).disabled(entry.installed)
            }
            Text(more ? entry.detail : entry.blurb).font(.callout).foregroundStyle(.secondary)
            Button(more ? "Less" : "More") { withAnimation(.snappy) { more.toggle() } }
                .buttonStyle(.link)
                .font(.caption)
        }
        .padding(.vertical, 2)
    }
}

/// Add an MCP server: a recommended one in one click, or configured by hand.
/// It stays open until the connection has actually been tried.
private struct AddServerSheet: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss

    let prefill: McpRegistryEntry?
    let registry: [McpRegistryEntry]
    let onAdded: () -> Void

    @State private var slug: String?
    @State private var transport = "http"
    @State private var name = ""
    @State private var url = ""
    @State private var command = ""
    @State private var args = ""
    @State private var useAuth = false
    @State private var token = ""
    @State private var headers: [Header] = []
    @State private var connecting = false
    @State private var failure: String?

    struct Header: Identifiable {
        let id = UUID()
        var key = ""
        var value = ""
    }

    private var selected: McpRegistryEntry? { registry.first { $0.slug == slug } }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                let recommended = registry.filter { !$0.installed }
                if !recommended.isEmpty {
                    Section {
                        Picker("Server", selection: Binding(get: { slug ?? "" }, set: { choose($0) })) {
                            Text("Configure manually").tag("")
                            ForEach(recommended) { Text($0.name).tag($0.slug) }
                        }
                    } header: {
                        Text("Recommended")
                    } footer: {
                        if let selected { Text(selected.detail) }
                    }
                }

                if let selected {
                    Section {
                        LabeledContent("Endpoint") {
                            Text(selected.url ?? ([selected.command ?? ""] + selected.args).joined(separator: " "))
                                .font(.callout.monospaced())
                                .textSelection(.enabled)
                        }
                        if selected.requirement == "local_runtime" {
                            Text("Runs as a process on this Mac. It can only reach what you pass in the arguments.")
                                .font(.callout).foregroundStyle(.secondary)
                        }
                    }
                } else {
                    Section("Manual") {
                        Picker("Transport", selection: $transport) {
                            Text("Remote (HTTP)").tag("http")
                            Text("Local (stdio)").tag("stdio")
                        }
                        .pickerStyle(.segmented)
                        TextField("Name", text: $name, prompt: Text("My server"))
                        if transport == "http" {
                            TextField("Server URL", text: $url, prompt: Text("https://mcp.example.com/mcp"))
                                .font(.body.monospaced())
                        } else {
                            TextField("Command", text: $command, prompt: Text("npx"))
                                .font(.body.monospaced())
                        }
                    }
                }

                if transport == "stdio" || selected?.transport == "stdio" {
                    Section {
                        TextField("Arguments", text: $args, prompt: Text("-y @modelcontextprotocol/server-filesystem ~/project"))
                            .font(.body.monospaced())
                    } footer: {
                        Text("Space separated; quote paths with spaces. For the filesystem server, the folders it may read.")
                    }
                }

                Section {
                    Toggle("Send an authorization token", isOn: $useAuth)
                    if useAuth {
                        SecureField("Bearer token", text: $token)
                        if let link = selected?.credentialsUrl.flatMap(URL.init(string:)) {
                            Link("Get a token", destination: link)
                        }
                    }
                } footer: {
                    if useAuth { Text("Kept in an owner-only file, never in Kuro's database.") }
                }

                Section {
                    ForEach($headers) { $header in
                        HStack {
                            TextField("Header", text: $header.key, prompt: Text("X-Header")).font(.body.monospaced())
                            TextField("Value", text: $header.value, prompt: Text("value")).font(.body.monospaced())
                            Button { headers.removeAll { $0.id == header.id } } label: { Image(systemName: "minus.circle") }
                                .buttonStyle(.borderless)
                        }
                        .labelsHidden()
                    }
                    Button("Add Header", systemImage: "plus") { headers.append(Header()) }
                        .buttonStyle(.borderless)
                } header: {
                    Text("Custom Headers")
                }

                if let failure {
                    Section {
                        ErrorText("Could not connect. \(failure)")
                        Text("The server was saved. Fix the details and press Add again, or close and reconnect it from the list.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .formStyle(.grouped)

            Divider()
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button(connecting ? "Connecting…" : "Add", action: add)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canSubmit || connecting)
            }
            .padding(14)
        }
        .frame(width: 560, height: 620)
        .onAppear { if let prefill { choose(prefill.slug) } }
    }

    private var canSubmit: Bool {
        if slug != nil { return true }
        let target = transport == "http" ? url : command
        return !name.trimmingCharacters(in: .whitespaces).isEmpty && !target.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private func choose(_ chosen: String) {
        failure = nil
        guard let entry = registry.first(where: { $0.slug == chosen }) else {
            slug = nil
            return
        }
        slug = entry.slug
        transport = entry.transport
        name = entry.name
        url = entry.url ?? ""
        command = entry.command ?? ""
        args = entry.args.joined(separator: " ")
        useAuth = entry.requirement == "api_key"
    }

    private func add() {
        // A recommended entry is added by slug; its arguments are still sent, since those are what a user fills in.
        var body: KuroClient.Body = [
            "args": shellWords(args),
            "headers": Dictionary(headers.compactMap { header -> (String, String)? in
                let key = header.key.trimmingCharacters(in: .whitespaces)
                let value = header.value.trimmingCharacters(in: .whitespaces)
                return key.isEmpty || value.isEmpty ? nil : (key, value)
            }, uniquingKeysWith: { _, last in last }),
        ]
        if let slug {
            body["slug"] = slug
        } else {
            body["name"] = name.trimmingCharacters(in: .whitespaces)
            body["transport"] = transport
            if transport == "http" { body["url"] = url.trimmingCharacters(in: .whitespaces) } else { body["command"] = command.trimmingCharacters(in: .whitespaces) }
        }
        if useAuth, !token.trimmingCharacters(in: .whitespaces).isEmpty { body["authToken"] = token.trimmingCharacters(in: .whitespaces) }

        connecting = true
        perform($failure) {
            defer { connecting = false }
            let result = try await app.client.addMcpServer(body)
            if result.ok {
                onAdded()
            } else {
                failure = result.error ?? "Connected, but the server returned no tools."
            }
        }
    }
}
