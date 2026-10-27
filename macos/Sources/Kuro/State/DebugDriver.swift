#if DEBUG
import AppKit
import SwiftUI
import KuroKit

/// Debug builds only: drive the window from outside, for screenshots and smoke tests.
///
///     swift macos/Scripts/poke.swift "route models"
///     swift macos/Scripts/poke.swift "send Hello there"
@MainActor
enum DebugDriver {
    static func install(_ app: AppModel) {
        DistributedNotificationCenter.default().addObserver(forName: .init("kuro.debug"), object: nil, queue: .main) { note in
            guard let command = note.object as? String else { return }
            MainActor.assumeIsolated { run(command, app) }
        }
    }

    private static func run(_ command: String, _ app: AppModel) {
        let parts = command.split(separator: " ", maxSplits: 1).map(String.init)
        let argument = parts.count > 1 ? parts[1] : ""
        switch parts.first {
        case "route":
            let target = argument.split(separator: " ").map(String.init)
            switch target.first {
            case "new": app.go(.newChat)
            case "chat": app.go(target.count > 1 ? .chat(target[1]) : .chat(app.conversations.first?.id ?? ""))
            case "code": app.go(.code)
            case "workspace": if target.count > 1 { app.go(.workspace(target[1])) }
            case "projects": app.go(.projects)
            case "project": if target.count > 1 { app.go(.project(target[1])) }
            case "models": app.go(.models)
            case "model": if target.count > 1 { app.go(.model(target[1]), returning: true) }
            case "free": app.go(.free)
            case "tools": app.go(.tools)
            case "providers": app.go(.providers)
            case "cloud": app.go(.cloud)
            default: break
            }
        case "draft":
            app.prefs.setDraft("chat:new", argument)
        case "snap":
            // Render the window (or Settings, with `snap settings <path>`) to a PNG, even off screen.
            let words = argument.split(separator: " ", maxSplits: 1).map(String.init)
            let settings = words.first == "settings"
            let path = settings ? (words.count > 1 ? words[1] : "") : argument
            let window = NSApp.windows.first { candidate in
                let id = candidate.identifier?.rawValue ?? ""
                return settings ? candidate.isVisible && !id.contains("main") && candidate.frame.width > 400 : id.contains("main")
            }
            guard let view = window?.contentView?.superview ?? window?.contentView,
                  let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
            view.cacheDisplay(in: view.bounds, to: rep)
            try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
        case "settings":
            // `settings <tab> <path>`: host one Settings tab in a scratch window and render it.
            let words = argument.split(separator: " ", maxSplits: 1).map(String.init)
            guard words.count == 2, let tab = SettingsView.Tab(rawValue: words[0]) else { return }
            let window = NSWindow(contentViewController: NSHostingController(rootView: SettingsView(tab: tab).environment(app)))
            window.setFrameOrigin(NSPoint(x: -4000, y: -4000))
            window.orderBack(nil)
            Task {
                try? await Task.sleep(for: .seconds(2))
                if let view = window.contentView, let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) {
                    view.cacheDisplay(in: view.bounds, to: rep)
                    try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: words[1]))
                }
                window.close()
            }
        case "files":
            app.prefs.filesOpen = argument == "on"
        case "running":
            app.prefs.runningOpen = argument == "on"
        case "folders":
            app.prefs.readFolders = argument == "on"
        case "theme":
            app.prefs.theme = Preferences.Theme(rawValue: argument) ?? .system
        case "send":
            Task {
                guard let conversation = try? await app.client.createConversation(model: app.prefs.chatModel) else { return }
                app.turns.run(conversation.id, content: argument, request: TurnRequest(
                    model: app.prefs.chatModel, effort: app.prefs.chatEffort, tools: app.prefs.chatToolGroups
                ))
                app.invalidate(.conversations)
                app.go(.chat(conversation.id))
            }
        default:
            break
        }
    }
}
#endif
