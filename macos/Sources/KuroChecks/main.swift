import Foundation
import KuroKit

// Assertion checks for KuroKit's pure logic. Run with `swift run KuroChecks`.
// (Command Line Tools ship neither XCTest nor Swift Testing, so this is a plain executable.)

var failures = 0

func check(_ condition: @autoclosure () -> Bool, _ message: String, line: Int = #line) {
    if !condition() {
        failures += 1
        print("FAIL line \(line): \(message)")
    }
}

// Diff
do {
    let rows = LineDiff.rows(before: "a\nb\nc", after: "a\nB\nc\nd")
    check(rows.map(\.kind) == [.same, .removed, .added, .same, .added], "diff kinds: \(rows.map(\.kind))")
    check(rows[2].after == 2 && rows[1].before == 2, "diff line numbers")

    let long = (1...20).map { "line \($0)" }.joined(separator: "\n")
    let edited = long.replacingOccurrences(of: "line 10\n", with: "line ten\n")
    let items = LineDiff.collapse(LineDiff.rows(before: long, after: edited))
    check(items.first == .gap(6), "leading gap: \(String(describing: items.first))")
    check(items.last == .gap(7), "trailing gap: \(String(describing: items.last))")
    check(LineDiff.rows(before: "x", after: "x").allSatisfy { $0.kind == .same }, "identical text has no changes")
}

// Slash commands
do {
    let text = "please use /ru here"
    let token = Slash.token(in: text, caret: 14)
    check(token?.query == "ru" && token?.start == 11 && token?.end == 14, "token mid-sentence: \(String(describing: token))")
    check(Slash.token(in: "see https://x.com/a", caret: 19) == nil, "URLs are not commands")
    check(Slash.token(in: "/usr/local", caret: 4) == nil, "paths are not commands")
    check(Slash.token(in: "and/or", caret: 6) == nil, "mid-word slash is not a command")

    let replaced = Slash.replace(token!, in: text, with: "/rust ")
    check(replaced.text == "please use /rust  here" && replaced.caret == 17, "replace: \(replaced)")
    let removed = Slash.replace(token!, in: text, with: "")
    check(removed.text == "please use here", "remove: '\(removed.text)'")

    let known: Set<String> = ["rust", "security"]
    check(Slash.skillsNamed(in: "/rust and (/security) /nope /rust", known: known) == ["rust", "security"], "skills named")
    check(Slash.removing("rust", from: "use /rust for this") == "use for this", "remove skill word")

    let commands = [
        SlashCommand(name: "web", hint: "", kind: .toggle),
        SlashCommand(name: "rust", hint: "", aliases: ["rust"], kind: .skill),
        SlashCommand(name: "trust", hint: "", kind: .skill),
    ]
    check(Slash.match(commands, query: "ru").map(\.name) == ["rust", "trust"], "prefix before substring")
}

// Rich text in user messages
do {
    let segments = RichSegment.split("see https://x.com/rust. Use /rust, not /tmp/x")
    check(segments.contains(.link("https://x.com/rust")), "link trimmed: \(segments)")
    check(segments.contains(.command("/rust")), "command found")
    check(!segments.contains(.command("/tmp")), "paths are not commands")
}

// Naming
do {
    check(Naming.friendly("qwen3-4b:q4_k_m") == "qwen3-4b", "strip quant")
    check(Naming.quant(of: "qwen3-4b:q4_k_m") == "Q4_K_M", "quant")
    check(Naming.friendly("cloud:abc/meta/llama-3") == "meta/llama-3", "remote name keeps slashes")
    check(Naming.isRemote("cloud:abc/x") && !Naming.isRemote("cloud:abc"), "remote detection")
    check(Naming.friendly("free:coding") == "Kuro Free · coding" && Naming.isFree("free:auto"), "free names")
    check(Naming.brand(of: "qwen2.5-0.5b", family: nil) == "qwen", "brand strips version")
    check(Naming.bytes(1536) == "1.5 KB" && Naming.bytes(nil) == "—", "bytes")
    check(Naming.count(1_500_000) == "1.5M" && Naming.count(1499) == "1k", "count")
}

// File tree
do {
    let tree = FileNode.tree(from: ["src/main.rs", "README.md", "src/", "src/db/mod.rs"])
    check(tree.map(\.name) == ["src", "README.md"], "folders first: \(tree.map(\.name))")
    check(tree[0].children.map(\.name) == ["db", "main.rs"], "nested: \(tree[0].children.map(\.name))")
    check(tree[0].children[1].path == "src/main.rs", "file path kept")
}

// Shell words
check(shellWords(#"-y server "/Users/me/My Project" 'a b'"#) == ["-y", "server", "/Users/me/My Project", "a b"], "quoted args")

// Tool descriptions
do {
    let read = ToolAction.describe("read_file", ["path": .string("src/a.rs"), "start_line": .number(4), "end_line": .number(9)])
    check(read.target == "src/a.rs:4-9" && read.path == "src/a.rs", "read range: \(read.target)")
    check(ToolAction.describe("mcp_thing", ["q": .string("x")]).verb == "mcp thing", "unknown tool")
}

// Server-sent events
do {
    var parser = SSEParser()
    var events: [(String?, String)] = []
    for byte in Array("event: token\ndata: {\"content\":\"hé\"}\n\nevent: done\r\ndata: {}\r\n\r\n".utf8) {
        if let event = parser.feed(byte) { events.append(event) }
    }
    check(events.count == 2 && events[0].0 == "token" && events[1].0 == "done", "sse events: \(events)")
    if case .token(let text)? = ChatEvent.decode(event: events[0].0, data: events[0].1) {
        check(text == "hé", "multi-byte token")
    } else {
        check(false, "token decodes")
    }
}

// Markdown blocks
do {
    let blocks = Markdown.blocks("# Title\n\nSome **bold** text\nand more.\n\n- one\n- two\n  1. nested\n\n```swift\nlet x = 1\n```\n\n> quoted\n\n| a | b |\n|---|---|\n| 1 | 2 |\n")
    check(blocks.first == .heading(AttributedString("Title"), level: 1) || { if case .heading(_, 1) = blocks[0] { return true }; return false }(), "heading: \(blocks.first.map { "\($0)" } ?? "")")
    check(blocks.contains { if case .code("let x = 1", "swift") = $0 { return true }; return false }, "code block")
    check(blocks.contains { if case .paragraph(_, 1, "•", false) = $0 { return true }; return false }, "bullet item")
    check(blocks.contains { if case .paragraph(_, 2, "1.", false) = $0 { return true }; return false }, "nested ordered item")
    check(blocks.contains { if case .paragraph(_, _, _, true) = $0 { return true }; return false }, "quote")
    check(blocks.contains { if case .table(let rows) = $0 { return rows.count == 2 && rows[1].count == 2 }; return false }, "table")
}

// Brands
do {
    check(Brands.model("unsloth/DeepSeek-R1-0528-Qwen3-8B-GGUF")?.label == "DeepSeek", "distill belongs to its maker")
    check(Brands.model("NousResearch/Hermes-3-Llama-3.1-8B")?.label == "Nous Research", "fine-tune beats base family")
    check(Brands.model("microsoft/phi-4")?.label == "Microsoft", "phi")
    check(Brands.model("sophia-7b") == nil, "phi only as a word")
    check(Brands.model("o3-mini")?.label == "OpenAI", "o-series")
    check(Brands.provider("OpenRouter")?.file == "openrouter-color", "provider by slug")
    check(Brands.model("gemma-3")?.mono == false && Brands.model("gpt-oss")?.mono == true, "mono marks")
    let logos = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().appendingPathComponent("web/src/assets/logos")
    let missing = Brands.allFiles.filter { !FileManager.default.fileExists(atPath: logos.appendingPathComponent("\($0).svg").path) }
    check(missing.isEmpty, "every named logo exists: missing \(missing.sorted())")
}

// Model cards
do {
    let readme = """
    <div align="center">
      <img src="figures/logo.svg" width="60%" />
    </div>
    <p>Read <a href="https://docs.example.com">the guide</a> and <strong>run it</strong>.</p>
    <a href="https://x.com"><img src="https://img.shields.io/a.svg"></a> <img src="https://img.shields.io/b.svg">

    See [the licence](LICENSE) and `<tool_call>` tokens.

    ```xml
    <tool_call>{"name": "x"}</tool_call>
    ```

    ![bench](figures/bench.png)
    <video controls src="https://cdn.example.com/demo.mp4"></video>
    """
    let blocks = ModelCardText.blocks(readme, repo: "acme/Model")
    let logo = URL(string: "https://huggingface.co/acme/Model/resolve/main/figures/logo.svg")!

    check(blocks.first == .images([CardImage(url: logo, width: nil)]), "logo first, percent width dropped: \(blocks.first.map { "\($0)" } ?? "")")
    check(blocks.contains { if case .images(let row) = $0 { return row.count == 2 }; return false }, "badges share a row")
    check(blocks.contains(.video(URL(string: "https://cdn.example.com/demo.mp4")!)), "video lifted out")
    check(blocks.contains(.images([CardImage(url: URL(string: "https://huggingface.co/acme/Model/resolve/main/figures/bench.png")!, width: nil)])), "markdown image resolved to the raw file")

    let text = blocks.compactMap { if case .markdown(let text) = $0 { return text }; return nil }.joined(separator: "\n")
    check(text.contains("[the guide](https://docs.example.com)") && text.contains("**run it**"), "html links and bold: \(text)")
    check(text.contains("(https://huggingface.co/acme/Model/blob/main/LICENSE)"), "relative link points at the repo")
    check(text.contains("`<tool_call>`") && text.contains("<tool_call>{\"name\""), "code left as written")
    check(!text.contains("<div") && !text.contains("<p>") && !text.contains("[]("), "tags and empty links removed: \(text)")
    check(ModelCardText.resolve("javascript:alert(1)", repo: "a/b", link: true) == nil, "script URLs refused")
}

print(failures == 0 ? "All checks passed." : "\(failures) check(s) failed.")
exit(failures == 0 ? 0 : 1)
