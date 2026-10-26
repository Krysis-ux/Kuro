/**
 * Which maker a model or provider belongs to, for the logo beside its name.
 *
 * Read off the name, because that is all a GGUF repository reliably says about
 * where its weights came from: `unsloth/DeepSeek-R1-0528-Qwen3-8B-GGUF` is a
 * DeepSeek model that unsloth quantized, and the logo should say DeepSeek, not
 * unsloth. The quantizer gets its own, smaller mark from its Hub avatar.
 *
 * The SVGs are LobeHub's icon set (MIT), vendored so no logo needs the network.
 */

const FILES = import.meta.glob('../assets/logos/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>

export interface Brand {
  label: string
  src: string
  /** Drawn in the text colour rather than the brand's own, so it holds in both themes. */
  mono: boolean
}

function brand(label: string, file: string): Brand | null {
  const src = FILES[`../assets/logos/${file}.svg`]
  return src ? { label, src, mono: !file.endsWith('-color') } : null
}

/**
 * First match wins, so fine-tuners and distillations sit above the families
 * their names also contain: `DeepSeek-R1-Distill-Llama` is DeepSeek's,
 * `Hermes-3-Llama` is Nous Research's, `Llama-3.1-Nemotron` is NVIDIA's.
 */
const MODEL_BRANDS: [RegExp, string, string][] = [
  [/deepseek/, 'DeepSeek', 'deepseek-color'],
  [/hermes|nousresearch/, 'Nous Research', 'nousresearch'],
  [/nemotron|minitron|nvidia/, 'NVIDIA', 'nvidia-color'],
  [/openchat/, 'OpenChat', 'openchat-color'],
  [/r1-1776|sonar|perplexity/, 'Perplexity', 'perplexity-color'],
  [/kwaipilot|kwaicoder/, 'Kwaipilot', 'kwaipilot-color'],
  [/arcee|\bafm-/, 'Arcee', 'arcee-color'],
  [/qwen|qwq/, 'Qwen', 'qwen-color'],
  [/gpt|openai|(^|\/)o[1-9](-|$)/, 'OpenAI', 'openai'],
  [/claude/, 'Claude', 'claude-color'],
  [/gemini/, 'Gemini', 'gemini-color'],
  [/gemma/, 'Gemma', 'gemma-color'],
  [/grok/, 'Grok', 'grok'],
  [/mistral|mixtral|devstral|magistral|ministral|codestral|pixtral|voxtral/, 'Mistral', 'mistral-color'],
  [/llama|meta-/, 'Meta', 'meta-color'],
  [/\bphi[-_.\d]|microsoft/, 'Microsoft', 'microsoft-color'],
  [/granite|\bibm/, 'IBM', 'ibm'],
  [/\baya/, 'Aya', 'aya-color'],
  [/command-?r|command-?a|c4ai|cohere/, 'Cohere', 'cohere-color'],
  [/codegeex/, 'CodeGeeX', 'codegeex-color'],
  [/chatglm/, 'ChatGLM', 'chatglm-color'],
  [/\bglm|zhipu|zai-org/, 'Z.ai', 'zhipu-color'],
  [/kimi|moonshot/, 'Kimi', 'kimi'],
  [/minimax/, 'MiniMax', 'minimax-color'],
  [/internlm|intern-?s1|internvl/, 'InternLM', 'internlm-color'],
  [/baichuan/, 'Baichuan', 'baichuan-color'],
  [/hunyuan/, 'Hunyuan', 'hunyuan-color'],
  [/ernie/, 'ERNIE', 'wenxin-color'],
  [/doubao/, 'Doubao', 'doubao-color'],
  [/seed-?oss|bytedance|ui-tars/, 'ByteDance', 'bytedance-color'],
  [/stepfun|\bstep-?\d/, 'StepFun', 'stepfun-color'],
  [/falcon/, 'TII', 'tii-color'],
  [/olmo|tulu|molmo|allenai/, 'Ai2', 'ai2-color'],
  [/smollm|smolvlm|huggingface/, 'Hugging Face', 'huggingface-color'],
  [/\blfm|liquid/, 'Liquid AI', 'liquid'],
  [/exaone/, 'LG AI', 'lg-color'],
  [/solar|upstage/, 'Upstage', 'upstage-color'],
  [/jina/, 'Jina AI', 'jina'],
  [/\bbge|baai/, 'BAAI', 'baai'],
  [/mimo|xiaomi/, 'Xiaomi', 'xiaomimimo'],
  [/arctic|snowflake/, 'Snowflake', 'snowflake-color'],
  [/dbrx/, 'Databricks', 'dbrx-color'],
  [/stablelm|stable-?code|stability/, 'Stability AI', 'stability-color'],
  [/openelm|apple/, 'Apple', 'apple'],
  [/\byi[-_.\d]|01-ai/, 'Yi', 'yi'],
]

/** Keyed by the provider's slug, as the presets and the free pool name them. */
const PROVIDER_FILES: Record<string, [string, string]> = {
  openrouter: ['OpenRouter', 'openrouter-color'],
  anthropic: ['Anthropic', 'anthropic'],
  openai: ['OpenAI', 'openai'],
  groq: ['Groq', 'groq'],
  deepseek: ['DeepSeek', 'deepseek-color'],
  mistral: ['Mistral', 'mistral-color'],
  together: ['Together', 'together-color'],
  lambda: ['Lambda', 'lambda'],
  vllm: ['vLLM', 'vllm-color'],
  ollama: ['Ollama', 'ollama'],
  cerebras: ['Cerebras', 'cerebras'],
  google: ['Google', 'google-color'],
  cohere: ['Cohere', 'cohere-color'],
  cloudflare: ['Cloudflare', 'cloudflare-color'],
  zhipu: ['Z.ai', 'zhipu-color'],
  siliconflow: ['SiliconFlow', 'siliconcloud-color'],
  nvidia: ['NVIDIA', 'nvidia-color'],
  vercel: ['Vercel', 'vercel'],
  huggingface: ['Hugging Face', 'huggingface-color'],
  kilo: ['Kilo', 'kilocode'],
  // Web search providers, on the Tools page.
  brave: ['Brave', 'brave-color'],
  tavily: ['Tavily', 'tavily-color'],
  searxng: ['SearXNG', 'searxng-color'],
}

export function modelBrand(name: string | null | undefined): Brand | null {
  if (!name) return null
  const lower = name.toLowerCase()
  const match = MODEL_BRANDS.find(([pattern]) => pattern.test(lower))
  return match ? brand(match[1], match[2]) : null
}

export function providerBrand(slug: string | null | undefined): Brand | null {
  const entry = slug ? PROVIDER_FILES[slug.toLowerCase()] : undefined
  return entry ? brand(entry[0], entry[1]) : null
}

/**
 * A Hub user's or organisation's avatar.
 *
 * The Hub serves the two from different paths and the owner's name does not
 * say which it is, so the organisation path is tried first (most model
 * publishers are organisations) and the image falls back to the user path.
 */
export function hubAvatarUrls(owner: string): string[] {
  const name = encodeURIComponent(owner)
  return [
    `https://huggingface.co/api/organizations/${name}/avatar?redirect=true`,
    `https://huggingface.co/api/users/${name}/avatar?redirect=true`,
  ]
}
