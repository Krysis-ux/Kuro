import SwiftUI
import KuroKit

/// Interface choices that persist between launches.
///
/// Chat and Code keep their own model and effort, so tuning one never
/// reconfigures the other.
@MainActor @Observable
final class Preferences {
    enum Theme: String, CaseIterable {
        case system, light, dark

        var scheme: ColorScheme? {
            switch self {
            case .system: nil
            case .light: .light
            case .dark: .dark
            }
        }
    }

    enum RunningView: String { case terminal, browser }
    enum CodePanel: String { case files, changes }

    @ObservationIgnored private let defaults = UserDefaults.standard

    var theme: Theme { didSet { defaults.set(theme.rawValue, forKey: "theme") } }

    /// `nil` lets the server choose.
    var chatModel: String? { didSet { defaults.set(chatModel, forKey: "chatModel") } }
    var codeModel: String? { didSet { defaults.set(codeModel, forKey: "codeModel") } }

    var chatEffort: Effort {
        didSet { defaults.set(chatEffort.rawValue, forKey: "chatEffort"); defaults.set(true, forKey: "chatEffortChosen") }
    }
    var codeEffort: Effort {
        didSet { defaults.set(codeEffort.rawValue, forKey: "codeEffort"); defaults.set(true, forKey: "codeEffortChosen") }
    }

    /// Search before answering. Off by default: switching it on sends the question off the machine.
    var webSearch: Bool { didSet { defaults.set(webSearch, forKey: "webSearch") } }
    /// Let chat read (never write) the folders opened on the Code page.
    var readFolders: Bool { didSet { defaults.set(readFolders, forKey: "readFolders") } }

    /// Half-written messages, by conversation or workspace.
    var drafts: [String: String] { didSet { defaults.set(drafts, forKey: "drafts") } }

    var filesOpen: Bool { didSet { defaults.set(filesOpen, forKey: "filesOpen") } }
    var codePanel: CodePanel { didSet { defaults.set(codePanel.rawValue, forKey: "codePanel") } }
    var runningOpen: Bool { didSet { defaults.set(runningOpen, forKey: "runningOpen") } }
    var runningView: RunningView { didSet { defaults.set(runningView.rawValue, forKey: "runningView") } }
    /// Panel widths in the Code page, as last dragged.
    var filesWidth: CGFloat { didSet { defaults.set(Double(filesWidth), forKey: "filesWidth") } }
    var viewerWidth: CGFloat { didSet { defaults.set(Double(viewerWidth), forKey: "viewerWidth") } }
    var runningWidth: CGFloat { didSet { defaults.set(Double(runningWidth), forKey: "runningWidth") } }

    init() {
        let read = { (key: String) in UserDefaults.standard.string(forKey: key) }
        let flag = { (key: String, fallback: Bool) in UserDefaults.standard.object(forKey: key) as? Bool ?? fallback }
        theme = Theme(rawValue: read("theme") ?? "") ?? .system
        chatModel = read("chatModel")
        codeModel = read("codeModel")
        chatEffort = Effort(rawValue: read("chatEffort") ?? "") ?? .balanced
        codeEffort = Effort(rawValue: read("codeEffort") ?? "") ?? .high
        webSearch = flag("webSearch", false)
        readFolders = flag("readFolders", true)
        drafts = UserDefaults.standard.dictionary(forKey: "drafts") as? [String: String] ?? [:]
        filesOpen = flag("filesOpen", true)
        codePanel = CodePanel(rawValue: read("codePanel") ?? "") ?? .files
        runningOpen = flag("runningOpen", false)
        runningView = RunningView(rawValue: read("runningView") ?? "") ?? .terminal
        let width = { (key: String, fallback: Double) in CGFloat(UserDefaults.standard.object(forKey: key) as? Double ?? fallback) }
        filesWidth = width("filesWidth", 260)
        viewerWidth = width("viewerWidth", 440)
        runningWidth = width("runningWidth", 420)
    }

    func draft(_ key: String) -> String { drafts[key] ?? "" }

    func setDraft(_ key: String, _ text: String) {
        // Empty drafts are dropped so this does not grow a key per conversation.
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            if drafts[key] != nil { drafts[key] = nil }
        } else if drafts[key] != text {
            drafts[key] = text
        }
    }

    /// Follow the starting efforts in Settings until the user picks one themselves.
    func seedEfforts(chat: Effort, code: Effort) {
        if !defaults.bool(forKey: "chatEffortChosen"), chatEffort != chat {
            chatEffort = chat
            defaults.set(false, forKey: "chatEffortChosen")
        }
        if !defaults.bool(forKey: "codeEffortChosen"), codeEffort != code {
            codeEffort = code
            defaults.set(false, forKey: "codeEffortChosen")
        }
    }

    /// The tool groups a chat turn gets. There is no file-writing group in chat by design.
    var chatToolGroups: [String] {
        (webSearch ? ["web"] : []) + ["memory"] + (readFolders ? ["projects"] : [])
    }
}
