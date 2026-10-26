import SwiftUI
import KuroKit

/// Files and Changes, beside the conversation.
struct WorkspaceSidebar: View {
    @Environment(AppModel.self) private var app
    let workspace: Workspace
    let onOpen: (String) -> Void

    var body: some View {
        @Bindable var prefs = app.prefs
        VStack(alignment: .leading, spacing: 0) {
            Picker("Panel", selection: $prefs.codePanel) {
                Text("Files").tag(Preferences.CodePanel.files)
                Text("Changes").tag(Preferences.CodePanel.changes)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(10)

            if !workspace.rootExists {
                ErrorText("This folder is no longer on disk.").padding(.horizontal, 10)
            }

            switch prefs.codePanel {
            case .files: FileTree(workspaceId: workspace.id, onOpen: onOpen)
            case .changes: ChangeList(workspaceId: workspace.id)
            }
        }
        .background(.background.secondary)
    }
}

/// The project's files. Folders start closed; filtering lists matching paths.
private struct FileTree: View {
    @Environment(AppModel.self) private var app
    let workspaceId: String
    let onOpen: (String) -> Void

    @State private var entries: [String]?
    @State private var failure: String?
    @State private var filter = ""

    var body: some View {
        VStack(spacing: 0) {
            TextField("Filter files", text: $filter)
                .textFieldStyle(.roundedBorder)
                .padding(.horizontal, 10)
                .padding(.bottom, 6)

            if let failure {
                ErrorText(failure).padding(10)
                Spacer()
            } else if let entries {
                let needle = filter.trimmingCharacters(in: .whitespaces).lowercased()
                if entries.isEmpty {
                    note("This folder is empty.")
                } else if needle.isEmpty {
                    List(FileNode.tree(from: entries), children: \.outlineChildren) { node in
                        fileRow(node)
                    }
                    .listStyle(.sidebar)
                } else {
                    let hits = entries.filter { !$0.hasSuffix("/") && $0.lowercased().contains(needle) }
                    if hits.isEmpty {
                        note("Nothing matched “\(filter)”.")
                    } else {
                        List(hits, id: \.self) { path in
                            Button { onOpen(path) } label: {
                                Label(path, systemImage: "doc").font(.callout.monospaced()).lineLimit(1).truncationMode(.head)
                            }
                            .buttonStyle(.plain)
                        }
                        .listStyle(.sidebar)
                    }
                }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: app.stamp(.workspace(workspaceId))) {
            do {
                entries = try await app.client.tree(workspaceId)
                failure = nil
            } catch {
                failure = error.localizedDescription
            }
        }
    }

    @ViewBuilder private func fileRow(_ node: FileNode) -> some View {
        if node.isDirectory {
            Label(node.name, systemImage: "folder").font(.callout)
        } else {
            Button { onOpen(node.path) } label: {
                Label(node.name, systemImage: "doc").font(.callout)
            }
            .buttonStyle(.plain)
            .help(node.path)
        }
    }

    private func note(_ text: String) -> some View {
        Text(text).foregroundStyle(.secondary).padding(12).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

extension FileNode {
    var outlineChildren: [FileNode]? { isDirectory ? children : nil }
}

/// Every file the model changed, newest first, each with a way to put it back.
private struct ChangeList: View {
    @Environment(AppModel.self) private var app
    let workspaceId: String

    @State private var changes: [WorkspaceChange] = []
    @State private var failure: String?
    @State private var showing: WorkspaceChange?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let failure { ErrorText(failure).padding(10) }
            if changes.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Nothing has been changed yet.")
                    Text("Commands are not listed here. A file edit can be put back; a command that has already run cannot.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(12)
                Spacer()
            } else {
                List(changes) { change in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Button(change.path) { showing = change }
                                .buttonStyle(.link)
                                .font(.callout.monospaced())
                                .lineLimit(1)
                                .truncationMode(.head)
                                .help("Show what changed")
                            Spacer()
                            if change.undone {
                                Text("undone").font(.caption).foregroundStyle(.tertiary)
                            } else if change.undoable {
                                Button(change.created ? "Remove" : "Undo") { undo(change) }
                                    .controlSize(.small)
                                    .help(change.created ? "Remove this file, which the model created" : "Put the previous contents back")
                            }
                        }
                        Text(describe(change)).font(.caption).foregroundStyle(.secondary)
                    }
                    .opacity(change.undone ? 0.5 : 1)
                }
                .listStyle(.sidebar)
            }
        }
        .task(id: app.stamp(.workspace(workspaceId))) {
            changes = (try? await app.client.changes(workspaceId)) ?? changes
        }
        .sheet(item: $showing) { change in
            DiffSheet(workspaceId: workspaceId, change: change)
        }
    }

    private func describe(_ change: WorkspaceChange) -> String {
        if change.created { return "created · \(change.afterLines ?? 0) lines" }
        let before = change.beforeLines ?? 0
        let after = change.afterLines ?? 0
        let delta = after - before
        return "\(change.kind == "edit" ? "edited" : "replaced") · \(after) lines (\(delta > 0 ? "+" : "")\(delta))"
    }

    private func undo(_ change: WorkspaceChange) {
        perform($failure) {
            try await app.client.undo(workspaceId, change.id)
            app.invalidate(.workspace(workspaceId))
        }
    }
}

/// What one change actually changed.
private struct DiffSheet: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    let workspaceId: String
    let change: WorkspaceChange

    @State private var items: [LineDiff.Item]?
    @State private var noSnapshot = false
    @State private var failure: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Text(change.path).font(.headline.monospaced()).lineLimit(1).truncationMode(.head)
                if let items {
                    let rows = items.compactMap { if case .row(let row) = $0 { row } else { nil } }
                    Text("+\(rows.filter { $0.kind == .added }.count)").foregroundStyle(.green).monospacedDigit()
                    Text("−\(rows.filter { $0.kind == .removed }.count)").foregroundStyle(.red).monospacedDigit()
                }
                Spacer()
                if let items { CopyButton(text: LineDiff.text(items), label: "Copy Diff") }
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
            .padding(14)
            Divider()

            if let failure {
                ErrorText(failure).padding()
            } else if noSnapshot {
                Text("This change was too large to keep a copy of, so there is nothing to compare. Undo is unavailable for the same reason.")
                    .foregroundStyle(.secondary)
                    .padding()
            } else if let items {
                ScrollView([.vertical, .horizontal]) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                            DiffLine(item: item)
                        }
                    }
                    .padding(.vertical, 8)
                }
            } else {
                ProgressView().padding()
            }
            Spacer(minLength: 0)
        }
        .frame(minWidth: 760, minHeight: 520)
        .task {
            do {
                let detail = try await app.client.change(workspaceId, change.id)
                // A missing snapshot is not an empty file.
                guard let after = detail.after else {
                    noSnapshot = true
                    return
                }
                items = LineDiff.collapse(LineDiff.rows(before: detail.before ?? "", after: after))
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}

private struct DiffLine: View {
    let item: LineDiff.Item

    var body: some View {
        switch item {
        case .gap(let count):
            Text("⋯ \(count) unchanged \(count == 1 ? "line" : "lines")")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(.horizontal, 14)
                .padding(.vertical, 4)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.primary.opacity(0.03))
        case .row(let row):
            HStack(spacing: 0) {
                Text(row.before.map(String.init) ?? "").frame(width: 44, alignment: .trailing).foregroundStyle(.tertiary)
                Text(row.after.map(String.init) ?? "").frame(width: 44, alignment: .trailing).foregroundStyle(.tertiary)
                Text(row.kind == .added ? "+" : row.kind == .removed ? "−" : " ")
                    .frame(width: 22)
                    .foregroundStyle(row.kind == .added ? .green : row.kind == .removed ? .red : .secondary)
                Text(row.text.isEmpty ? " " : row.text).fixedSize()
                Spacer(minLength: 0)
            }
            .font(.system(.callout, design: .monospaced))
            .padding(.vertical, 1)
            .background(row.kind == .added ? Color.green.opacity(0.1) : row.kind == .removed ? Color.red.opacity(0.1) : .clear)
        }
    }
}

/// One file, read-only. Editing here would sidestep the mode and the undo log.
struct FileViewer: View {
    @Environment(AppModel.self) private var app
    let workspaceId: String
    let path: String
    let onClose: () -> Void

    @State private var content: String?
    @State private var failure: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "doc.text").foregroundStyle(.secondary)
                Text(path).font(.callout.monospaced()).lineLimit(1).truncationMode(.head)
                Spacer()
                Button(action: onClose) { Image(systemName: "xmark") }
                    .buttonStyle(.borderless)
                    .help("Close this file")
            }
            .padding(10)
            Divider()

            if let failure {
                ErrorText(failure).padding()
                Spacer()
            } else if let content {
                CodeSurface(text: content.isEmpty ? "(this file is empty)" : content, maxHeight: .infinity)
                    .frame(maxHeight: .infinity, alignment: .top)
                    .padding(10)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: path) {
            content = nil
            failure = nil
            do { content = try await app.client.file(workspaceId, path: path) } catch { failure = error.localizedDescription }
        }
    }
}
