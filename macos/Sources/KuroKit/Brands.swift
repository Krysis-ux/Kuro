import Foundation

/// Which maker a model or provider belongs to, for the logo beside its name.
///
/// Read off the name, because that is all a GGUF repository reliably says about
/// where its weights came from: `unsloth/DeepSeek-R1-0528-Qwen3-8B-GGUF` is a
/// DeepSeek model that unsloth quantized.
///
/// Mirrors `web/src/lib/brands.ts` and draws on the same SVGs in
/// `web/src/assets/logos`. Keep the two tables in step.
public struct Brand: Hashable, Sendable {
    public let label: String
    /// The logo's file name, without `.svg`.
    public let file: String

    /// Drawn in the text colour rather than the brand's own, so it holds in both themes.
    public var mono: Bool { !file.hasSuffix("-color") }
}

public enum Brands {
    /// First match wins, so fine-tuners and distillations sit above the families
    /// their names also contain: `Hermes-3-Llama` is Nous Research's.
    private static let models: [(NSRegularExpression, Brand)] = [
        (#"deepseek"#, "DeepSeek", "deepseek-color"),
        (#"hermes|nousresearch"#, "Nous Research", "nousresearch"),
        (#"nemotron|minitron|nvidia"#, "NVIDIA", "nvidia-color"),
        (#"openchat"#, "OpenChat", "openchat-color"),
        (#"r1-1776|sonar|perplexity"#, "Perplexity", "perplexity-color"),
        (#"kwaipilot|kwaicoder"#, "Kwaipilot", "kwaipilot-color"),
        (#"arcee|\bafm-"#, "Arcee", "arcee-color"),
        (#"qwen|qwq"#, "Qwen", "qwen-color"),
        (#"gpt|openai|(^|/)o[1-9](-|$)"#, "OpenAI", "openai"),
        (#"claude"#, "Claude", "claude-color"),
        (#"gemini"#, "Gemini", "gemini-color"),
        (#"gemma"#, "Gemma", "gemma-color"),
        (#"grok"#, "Grok", "grok"),
        (#"mistral|mixtral|devstral|magistral|ministral|codestral|pixtral|voxtral"#, "Mistral", "mistral-color"),
        (#"llama|meta-"#, "Meta", "meta-color"),
        (#"\bphi[-_.\d]|microsoft"#, "Microsoft", "microsoft-color"),
        (#"granite|\bibm"#, "IBM", "ibm"),
        (#"\baya"#, "Aya", "aya-color"),
        (#"command-?r|command-?a|c4ai|cohere"#, "Cohere", "cohere-color"),
        (#"codegeex"#, "CodeGeeX", "codegeex-color"),
        (#"chatglm"#, "ChatGLM", "chatglm-color"),
        (#"\bglm|zhipu|zai-org"#, "Z.ai", "zhipu-color"),
        (#"kimi|moonshot"#, "Kimi", "kimi"),
        (#"minimax"#, "MiniMax", "minimax-color"),
        (#"internlm|intern-?s1|internvl"#, "InternLM", "internlm-color"),
        (#"baichuan"#, "Baichuan", "baichuan-color"),
        (#"hunyuan"#, "Hunyuan", "hunyuan-color"),
        (#"ernie"#, "ERNIE", "wenxin-color"),
        (#"doubao"#, "Doubao", "doubao-color"),
        (#"seed-?oss|bytedance|ui-tars"#, "ByteDance", "bytedance-color"),
        (#"stepfun|\bstep-?\d"#, "StepFun", "stepfun-color"),
        (#"falcon"#, "TII", "tii-color"),
        (#"olmo|tulu|molmo|allenai"#, "Ai2", "ai2-color"),
        (#"smollm|smolvlm|huggingface"#, "Hugging Face", "huggingface-color"),
        (#"\blfm|liquid"#, "Liquid AI", "liquid"),
        (#"exaone"#, "LG AI", "lg-color"),
        (#"solar|upstage"#, "Upstage", "upstage-color"),
        (#"jina"#, "Jina AI", "jina"),
        (#"\bbge|baai"#, "BAAI", "baai"),
        (#"mimo|xiaomi"#, "Xiaomi", "xiaomimimo"),
        (#"arctic|snowflake"#, "Snowflake", "snowflake-color"),
        (#"dbrx"#, "Databricks", "dbrx-color"),
        (#"stablelm|stable-?code|stability"#, "Stability AI", "stability-color"),
        (#"openelm|apple"#, "Apple", "apple"),
        (#"\byi[-_.\d]|01-ai"#, "Yi", "yi"),
    ].map { pattern, label, file in
        // The patterns are literals above; one that fails to compile is a typo caught by KuroChecks.
        (try! NSRegularExpression(pattern: pattern), Brand(label: label, file: file))
    }

    /// Keyed by the provider's slug, as the presets and the free pool name them.
    private static let providers: [String: Brand] = [
        "openrouter": ("OpenRouter", "openrouter-color"),
        "anthropic": ("Anthropic", "anthropic"),
        "openai": ("OpenAI", "openai"),
        "groq": ("Groq", "groq"),
        "deepseek": ("DeepSeek", "deepseek-color"),
        "mistral": ("Mistral", "mistral-color"),
        "together": ("Together", "together-color"),
        "lambda": ("Lambda", "lambda"),
        "vllm": ("vLLM", "vllm-color"),
        "ollama": ("Ollama", "ollama"),
        "cerebras": ("Cerebras", "cerebras"),
        "google": ("Google", "google-color"),
        "cohere": ("Cohere", "cohere-color"),
        "cloudflare": ("Cloudflare", "cloudflare-color"),
        "zhipu": ("Z.ai", "zhipu-color"),
        "siliconflow": ("SiliconFlow", "siliconcloud-color"),
        "nvidia": ("NVIDIA", "nvidia-color"),
        "vercel": ("Vercel", "vercel"),
        "huggingface": ("Hugging Face", "huggingface-color"),
        "kilo": ("Kilo", "kilocode"),
        // Web search providers, on the Tools screen.
        "brave": ("Brave", "brave-color"),
        "tavily": ("Tavily", "tavily-color"),
        "searxng": ("SearXNG", "searxng-color"),
    ].mapValues { Brand(label: $0.0, file: $0.1) }

    public static func model(_ name: String?) -> Brand? {
        guard let name, !name.isEmpty else { return nil }
        let lower = name.lowercased()
        let range = NSRange(lower.startIndex..., in: lower)
        return models.first { $0.0.firstMatch(in: lower, range: range) != nil }?.1
    }

    public static func provider(_ slug: String?) -> Brand? {
        slug.flatMap { providers[$0.lowercased()] }
    }

    /// A Hub account's avatar. Organisations and users live at different paths
    /// and the name does not say which, so organisations (most publishers) go first.
    public static func avatarURLs(owner: String) -> [URL] {
        let name = owner.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? owner
        return ["organizations", "users"].compactMap {
            URL(string: "https://huggingface.co/api/\($0)/\(name)/avatar?redirect=true")
        }
    }

    /// Every logo file the tables name, for checking that none is missing.
    public static var allFiles: Set<String> {
        Set(models.map(\.1.file) + providers.values.map(\.file))
    }
}
