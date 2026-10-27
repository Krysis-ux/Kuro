import AppKit
import SwiftUI

@main
struct KuroApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var app = AppModel()

    var body: some Scene {
        Window("Kuro", id: "main") {
            RootView()
                .environment(app)
                .onAppear { delegate.server = app.server }
        }
        .defaultSize(width: 1180, height: 780)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Chat") { app.go(.newChat) }
                    .keyboardShortcut("n")
            }
            CommandGroup(after: .sidebar) {
                Button("Code") { app.go(.code) }.keyboardShortcut("1", modifiers: [.command, .option])
                Button("Projects") { app.go(.projects) }.keyboardShortcut("2", modifiers: [.command, .option])
                Button("Models") { app.go(.models) }.keyboardShortcut("3", modifiers: [.command, .option])
                Button("Tools") { app.go(.tools) }.keyboardShortcut("4", modifiers: [.command, .option])
                Divider()
            }
        }

        Settings {
            SettingsView()
                .environment(app)
                .preferredColorScheme(app.prefs.theme.scheme)
        }
    }
}

/// Owns quitting: the server this app started is stopped before the app exits.
final class AppDelegate: NSObject, NSApplicationDelegate {
    var server: ServerController?
    private var terminationSignal: DispatchSourceSignal?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Needed when run as a bare executable (`swift run`); a bundle gets this from Info.plist.
        NSApp.setActivationPolicy(.regular)
        #if DEBUG
        // Lets scripted debug runs stay in the background.
        if ProcessInfo.processInfo.environment["KURO_BACKGROUND"] == nil { NSApp.activate(ignoringOtherApps: true) }
        #else
        NSApp.activate(ignoringOtherApps: true)
        #endif
        ServerController.linkCommandLineTool()

        // `kill` and logout send SIGTERM, which would otherwise end the app without
        // stopping the server it started. Route it through the normal quit.
        signal(SIGTERM, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
        source.setEventHandler { NSApp.terminate(nil) }
        source.resume()
        terminationSignal = source
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    /// Blocks for at most a few seconds. `.terminateLater` with an async task does not
    /// work here: AppKit does not drain the main queue while it waits for the reply.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        MainActor.assumeIsolated { server?.stopForQuit() }
        return .terminateNow
    }
}
