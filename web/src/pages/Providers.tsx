import { EndpointsPage } from './Endpoints'

/**
 * Providers: someone else's model, billed per token.
 *
 * Kuro's argument is that models should run on your machine. This page is the
 * honest exception: sometimes the machine cannot, and the alternative to
 * supporting that is a user keeping a second application open.
 *
 * The framing throughout is "your account, your key, your bill". There is nothing
 * hosted by Kuro here, and the page says so rather than leaving room for the
 * assumption. Renting a GPU to run your own weights is a different decision and
 * lives on its own page — see `Cloud.tsx`.
 */
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
