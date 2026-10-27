import AppKit
import SwiftUI
import WebKit
import KuroKit

/// What a workspace is running, its output, and the page it serves.
///
/// The address comes from the process's own output, so a server that has not
/// printed one yet shows its log instead of an empty frame.
struct RunningPanel: View {
    @Environment(AppModel.self) private var app
    let workspace: Workspace
    let processes: [RunningProcess]
    @Binding var view: Preferences.RunningView
    let reload: () -> Void

    @State private var selected: String?
    @State private var command = ""
    @State private var failure: String?
    @State private var reloadToken = 0

    private var active: RunningProcess? {
        processes.first { $0.id == selected } ?? processes.first(where: \.running) ?? processes.first
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Picker("View", selection: $view) {
                    Label("Terminal", systemImage: "terminal").tag(Preferences.RunningView.terminal)
                    Label("Browser", systemImage: "globe").tag(Preferences.RunningView.browser)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                Button { app.prefs.runningOpen = false } label: { Image(systemName: "xmark") }
                    .buttonStyle(.borderless)
                    .help("Hide")
            }

            if !workspace.mode.canRun {
                Text("\(workspace.mode.rawValue.capitalized) mode cannot run commands. Switch to Agent to build the project, run its tests, or start a dev server.")
                    .foregroundStyle(.secondary)
                Spacer()
            } else {
                if view == .terminal { startField }
                if let failure { ErrorText(failure) }
                processList

                switch view {
                case .terminal:
                    if let active { ProcessLog(workspaceId: workspace.id, process: active) } else { Spacer() }
                case .browser:
                    if let active, active.running, let address = active.url, let url = URL(string: address) {
                        browser(url)
                    } else {
                        Text("Nothing is serving a page yet. Start a dev server in the terminal; its address appears here as soon as it prints one.")
                            .foregroundStyle(.secondary)
                        Spacer()
                    }
                }
            }
        }
        .padding(12)
    }

    private var startField: some View {
        HStack {
            TextField("npm run dev", text: $command)
                .textFieldStyle(.roundedBorder)
                .font(.callout.monospaced())
                .onSubmit(start)
            Button("Start", systemImage: "play.fill", action: start)
                .disabled(command.trimmingCharacters(in: .whitespaces).isEmpty)
                .help("Start this in the background")
        }
    }

    @ViewBuilder private var processList: some View {
        let finished = processes.filter { !$0.running }.count
        if processes.isEmpty {
            Text("Nothing running. Start a dev server above, or ask the model to — the page appears here when it does.")
                .font(.callout)
                .foregroundStyle(.secondary)
        } else {
            VStack(spacing: 2) {
                ForEach(processes) { process in
                    HStack(spacing: 8) {
                        StatusDot(color: process.running ? .green : .secondary.opacity(0.4))
                        VStack(alignment: .leading, spacing: 1) {
                            Text(process.command).font(.callout.monospaced()).lineLimit(1)
                            Text(describe(process)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                        Spacer()
                        if process.running {
                            Button { act { try await app.client.stopProcess(workspace.id, process.id) } } label: { Image(systemName: "stop.fill") }
                                .help("Stop")
                        } else {
                            Button { act { try await app.client.forgetProcess(workspace.id, process.id) } } label: { Image(systemName: "xmark") }
                                .help("Clear this from the list")
                        }
                    }
                    .buttonStyle(.borderless)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                    .background(process.id == active?.id ? Color.accentColor.opacity(0.12) : .clear, in: RoundedRectangle(cornerRadius: 8))
                    .contentShape(Rectangle())
                    .onTapGesture { selected = process.id }
                }
            }
            if finished > 1 {
                Button("Clear \(finished) Finished", systemImage: "trash") {
                    act { try await app.client.clearProcesses(workspace.id) }
                }
                .controlSize(.small)
            }
        }
    }

    private func browser(_ url: URL) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "eye").foregroundStyle(.secondary)
                Text(url.absoluteString).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
                Spacer()
                Button { reloadToken += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .help("Reload")
                Button { NSWorkspace.shared.open(url) } label: { Image(systemName: "arrow.up.right.square") }
                    .help("Open in your browser")
            }
            .buttonStyle(.borderless)
            WebView(url: url, reloadToken: reloadToken)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.primary.opacity(0.1)))
            Text("A frame around your dev server. If it stays blank the page may refuse to be framed — open it in your browser instead.")
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }

    private func describe(_ process: RunningProcess) -> String {
        if !process.running {
            return process.exitCode == 0 ? "finished" : "exited with \(process.exitCode.map(String.init) ?? "an unknown code")"
        }
        let started = Naming.relative(process.startedAt)
        return process.url.map { "serving \($0) · started \(started)" } ?? "running · started \(started)"
    }

    private func start() {
        let line = command.trimmingCharacters(in: .whitespaces)
        guard !line.isEmpty else { return }
        act {
            let started = try await app.client.startProcess(workspace.id, command: line)
            command = ""
            selected = started.id
        }
    }

    private func act(_ work: @escaping () async throws -> Void) {
        perform($failure) {
            try await work()
            reload()
        }
    }
}

/// A process's recent output, polled while it runs. Where a failed start explains itself.
private struct ProcessLog: View {
    @Environment(AppModel.self) private var app
    let workspaceId: String
    let process: RunningProcess
    @State private var lines: [String] = []

    var body: some View {
        Group {
            if lines.isEmpty {
                Text(process.running ? "Nothing printed yet." : "It printed nothing before exiting.")
                    .foregroundStyle(.secondary)
                Spacer()
            } else {
                CodeSurface(text: lines.joined(separator: "\n"), language: "output", maxHeight: .infinity)
                    .defaultScrollAnchor(.bottom)
            }
        }
        .task(id: "\(process.id)-\(process.running)") {
            repeat {
                if let fetched = try? await app.client.processLog(workspaceId, process.id) { lines = fetched }
                try? await Task.sleep(for: .seconds(2))
            } while process.running && !Task.isCancelled
        }
    }
}

struct WebView: NSViewRepresentable {
    let url: URL
    let reloadToken: Int

    final class Coordinator {
        var url: URL?
        var token = 0
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let view = WKWebView()
        view.load(URLRequest(url: url))
        context.coordinator.url = url
        context.coordinator.token = reloadToken
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        if context.coordinator.url != url {
            context.coordinator.url = url
            view.load(URLRequest(url: url))
        } else if context.coordinator.token != reloadToken {
            context.coordinator.token = reloadToken
            view.reload()
        }
    }
}
