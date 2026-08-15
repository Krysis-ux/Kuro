import { EndpointsPage } from './Endpoints'

export function ProvidersPage() {
  return (
    <EndpointsPage
      surface="provider"
      title="Providers"
      intro="Talk to a model you do not run yourself. Your account, your key, your bill — the request goes straight from this machine to the provider, and Kuro is not in the middle of it."
      urlPlaceholder="https://api.example.com/v1"
      notes={[
        'Provider models appear in the same picker as local ones, marked as leaving this machine. Local models stay at the top of the list.',
        'Everything else works identically: conversations, the effort control, web search, MCP tools, file access, the request inspector.',
        'Keys are written to an owner-only file next to the database, never into it, and are never sent back to this page once saved.',
        'Kuro speaks one wire format — the OpenAI API. Anthropic is reached through its compatibility endpoint, which is why it needs no special handling.',
        'To run your own weights on a GPU you rent rather than paying per token, use Cloud instead.',
      ]}
    />
  )
}
