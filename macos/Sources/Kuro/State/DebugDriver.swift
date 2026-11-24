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
            case "settings": app.go(.settings(SettingsPane(rawValue: target.count > 1 ? target[1] : "") ?? .general))
            default: break
            }
        case "providers":
            // `providers add`, `providers free` or `providers <preset slug>`: what the Providers page shows.
            NotificationCenter.default.post(name: .init("kuro.debug.providers"), object: argument)
        case "subagent":
            // `subagent new` or `subagent <slug>`: open the subagent editor.
            NotificationCenter.default.post(name: .init("kuro.debug.subagents"), object: argument)
        case "draft":
            app.prefs.setDraft("chat:new", argument)
        case "snap":
            // Render the main window to a PNG, even off screen, and any open sheet beside it
            // as `<path>.sheet.png`. Settings are a route now: `route settings providers`.
            let window = NSApp.windows.first { $0.identifier?.rawValue.contains("main") == true }
            let render = { (target: NSWindow?, path: String) in
                guard let view = target?.contentView?.superview ?? target?.contentView,
                      let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
                view.cacheDisplay(in: view.bounds, to: rep)
                try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
            }
            render(window, argument)
            if let sheet = window?.attachedSheet { render(sheet, argument + ".sheet.png") }
        case "click":
            // `click 120 300`: a left click at a point measured from the window's top left,
            // the same frame `snap` draws, so a hit-testing offset shows up as the wrong target.
            let numbers = argument.split(separator: " ").compactMap { Double($0) }
            guard numbers.count == 2,
                  let window = NSApp.windows.first(where: { $0.identifier?.rawValue.contains("main") == true }) else { return }
            let point = NSPoint(x: numbers[0], y: window.frame.height - numbers[1])
            for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
                if let event = NSEvent.mouseEvent(
                    with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1
                ) {
                    window.sendEvent(event)
                }
            }
        case "where":
            // `where <path>`: write the current route to a file, to read back after a `click`.
            try? "\(app.route)".write(toFile: argument, atomically: true, encoding: .utf8)
        case "size":
            // `size 820 560`: resize the main window's content, to check layouts at the minimum.
            let numbers = argument.split(separator: " ").compactMap { Double($0) }
            guard numbers.count == 2 else { return }
            NSApp.windows.first { $0.identifier?.rawValue.contains("main") == true }?
                .setContentSize(NSSize(width: numbers[0], height: numbers[1]))
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
