import AppKit
import SwiftUI
import KuroKit

struct RootView: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        Group {
            if app.server.phase == .ready {
                NavigationSplitView {
                    SidebarView()
                        .navigationSplitViewColumnWidth(min: 210, ideal: 250, max: 340)
                } detail: {
                    detail.id(app.route)
                }
            } else {
                ServerStateView()
            }
        }
        .frame(minWidth: 820, minHeight: 560)
        .preferredColorScheme(app.prefs.theme.scheme)
        .task {
            await app.server.start()
        }
        .task(id: app.server.phase) {
            if app.server.phase == .ready { await app.loadAll() }
        }
    }

    @ViewBuilder private var detail: some View {
        switch app.route {
        case .newChat: ChatView(conversationId: nil)
        case .chat(let id): ChatView(conversationId: id)
        case .code: WorkspacesView()
        case .workspace(let id): WorkspaceView(workspaceId: id)
        case .projects: ProjectsView()
        case .project(let id): ProjectDetailView(projectId: id)
        case .models: ModelsView()
        case .model(let repo): ModelDetailView(repo: repo)
        case .free: FreeView()
        case .tools: ToolsView()
        case .providers: EndpointsView(surface: .provider)
        case .cloud: EndpointsView(surface: .cloud)
        }
    }
}

/// Shown while the server starts, after it fails, or once it was stopped.
struct ServerStateView: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        VStack(spacing: 18) {
            KuroMark(size: 56)

            switch app.server.phase {
            case .starting, .ready:
                Text("Starting Kuro…").font(.title3)
                ProgressView().controlSize(.small)
            case .stopped:
                Text("Kuro is stopped").font(.title3)
                Text("Models are unloaded and the API is closed.").foregroundStyle(.secondary)
                Button("Start Kuro") { Task { await app.server.start() } }
                    .buttonStyle(.borderedProminent)
            case .failed(let message):
                Text("Kuro could not start").font(.title3)
                Text(message)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
                HStack {
                    Button("Show Log") { NSWorkspace.shared.open(ServerController.logURL) }
                    Button("Try Again") { Task { await app.server.start() } }
                        .buttonStyle(.borderedProminent)
                }
            }
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
