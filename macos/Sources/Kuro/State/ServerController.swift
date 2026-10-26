import AppKit
import KuroKit

/// Starts Kuro's server, watches it, and stops it on quit.
///
/// If a server is already answering on the port (say, `kuro serve` in a
/// terminal), the app uses it and leaves it running when it quits.
@MainActor @Observable
final class ServerController {
    enum Phase: Equatable {
        case starting
        case ready
        case stopped
        case failed(String)
    }

    private(set) var phase: Phase = .starting
    let client: KuroClient
    let port: Int
    /// Whether this app started the server, and so should stop it.
    private(set) var owned = false

    @ObservationIgnored private var process: Process?
    @ObservationIgnored private var watchdog: Task<Void, Never>?

    static let logURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Logs/Kuro/kuro-server.log")

    init() {
        port = ProcessInfo.processInfo.environment["KURO_PORT"].flatMap(Int.init) ?? 8420
        client = KuroClient(base: URL(string: "http://127.0.0.1:\(port)")!)
    }

    func start() async {
        phase = .starting
        if await client.isHealthy() {
            phase = .ready
            watch()
            return
        }

        guard let binary = Self.serverBinary() else {
            phase = .failed("Kuro's server was not found. Build it with `cargo build --release`, or reinstall Kuro.")
            return
        }

        do {
            process = try launch(binary)
            owned = true
        } catch {
            phase = .failed("Kuro's server could not be started: \(error.localizedDescription)")
            return
        }

        // Up to 90 seconds: a first launch may be downloading the engine.
        for _ in 0..<180 {
            if await client.isHealthy() {
                phase = .ready
                watch()
                return
            }
            if process?.isRunning == false { break }
            try? await Task.sleep(for: .milliseconds(500))
        }
        phase = .failed("Kuro's server did not start. Port \(port) may be in use by another program.")
    }

    /// Stop the server from Settings. The window offers to start it again.
    func stop() async {
        watchdog?.cancel()
        try? await client.shutdown()
        phase = .stopped
    }

    /// Ask the server to relaunch itself and wait for the successor.
    func restart() async throws {
        watchdog?.cancel()
        phase = .starting
        // The request may not complete: the server is going down.
        try? await client.restart()
        for _ in 0..<120 {
            try? await Task.sleep(for: .milliseconds(500))
            if await client.isHealthy() {
                phase = .ready
                watch()
                return
            }
        }
        phase = .failed("Kuro did not come back after restarting. See the log for why.")
    }

    /// Called on quit. Stops the server only if this app started it. A restart's
    /// successor is not our child process, so the API is asked rather than the process.
    func stopForQuit() {
        guard owned else { return }
        watchdog?.cancel()

        // Synchronous on purpose; see `AppDelegate.applicationShouldTerminate`.
        var request = URLRequest(url: client.base.appendingPathComponent("api/shutdown"))
        request.httpMethod = "POST"
        request.timeoutInterval = 2
        let answered = DispatchSemaphore(value: 0)
        URLSession.shared.dataTask(with: request) { _, _, _ in answered.signal() }.resume()
        _ = answered.wait(timeout: .now() + 2)

        // The server unloads its engines before exiting; give it a moment, then insist.
        guard let process else { return }
        let deadline = Date().addingTimeInterval(5)
        while process.isRunning, Date() < deadline { usleep(100_000) }
        if process.isRunning { process.terminate() }
    }

    // MARK: Internals

    /// Notice if the server goes away while the app is open.
    private func watch() {
        watchdog?.cancel()
        watchdog = Task { [weak self] in
            var misses = 0
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(5))
                guard let self, !Task.isCancelled else { return }
                misses = await self.client.isHealthy() ? 0 : misses + 1
                if misses >= 2 {
                    self.phase = .failed("Kuro's server stopped unexpectedly.")
                    return
                }
            }
        }
    }

    private func launch(_ binary: URL) throws -> Process {
        let process = Process()
        process.executableURL = binary

        var environment = ProcessInfo.processInfo.environment
        if let web = Bundle.main.resourceURL?.appendingPathComponent("web"),
           FileManager.default.fileExists(atPath: web.appendingPathComponent("index.html").path) {
            environment["KURO_WEB_DIR"] = web.path
        }
        // A Finder launch gets a minimal PATH, so MCP servers (`npx`) and workspace
        // commands (`npm`, `cargo`) would not be found.
        // ponytail: fixed list of common tool dirs; read the login shell's PATH if users hit gaps.
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let extra = ["/opt/homebrew/bin", "/usr/local/bin", "\(home)/.local/bin", "\(home)/.cargo/bin"]
        environment["PATH"] = (extra + [environment["PATH"] ?? "/usr/bin:/bin:/usr/sbin:/sbin"]).joined(separator: ":")
        process.environment = environment

        try FileManager.default.createDirectory(at: Self.logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: Self.logURL.path, contents: nil)
        let log = try FileHandle(forWritingTo: Self.logURL)
        process.standardOutput = log
        process.standardError = log

        try process.run()
        return process
    }

    /// `KURO_SERVER_BIN`, then the copy inside the app bundle, then a development
    /// build in the repository's `target/` directory.
    static func serverBinary() -> URL? {
        let fileManager = FileManager.default
        if let configured = ProcessInfo.processInfo.environment["KURO_SERVER_BIN"], fileManager.isExecutableFile(atPath: configured) {
            return URL(fileURLWithPath: configured)
        }
        if let bundled = Bundle.main.resourceURL?.appendingPathComponent("bin/kuro-server"), fileManager.isExecutableFile(atPath: bundled.path) {
            return bundled
        }
        var directory = Bundle.main.executableURL?.deletingLastPathComponent()
        while let current = directory, current.path != "/" {
            for build in ["target/release/kuro-server", "target/debug/kuro-server"] {
                let candidate = current.appendingPathComponent(build)
                if fileManager.isExecutableFile(atPath: candidate.path) { return candidate }
            }
            directory = current.deletingLastPathComponent()
        }
        return nil
    }

    /// Put the bundled `kuro` command on the PATH the first time the app runs.
    /// `~/.local/bin` rather than `/usr/local/bin`, which would need a password.
    static func linkCommandLineTool() {
        guard let bundled = Bundle.main.resourceURL?.appendingPathComponent("bin/kuro"),
              FileManager.default.isExecutableFile(atPath: bundled.path) else { return }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let existing = ["\(home)/.local/bin/kuro", "/usr/local/bin/kuro", "/opt/homebrew/bin/kuro"]
        guard !existing.contains(where: { FileManager.default.fileExists(atPath: $0) }) else { return }
        try? FileManager.default.createDirectory(atPath: "\(home)/.local/bin", withIntermediateDirectories: true)
        try? FileManager.default.createSymbolicLink(atPath: "\(home)/.local/bin/kuro", withDestinationPath: bundled.path)
    }
}
