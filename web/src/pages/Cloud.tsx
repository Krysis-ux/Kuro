import { EndpointsPage } from './Endpoints'

export function CloudPage() {
  return (
    <EndpointsPage
      surface="cloud"
      title="Cloud"
      intro="Run your own models on hardware you rent or own. A GPU box, a server on your network, anything speaking the OpenAI API — Kuro drives it exactly as it drives a local engine. Your instance, your endpoint, your bill."
      urlPlaceholder="https://your-pod-8000.proxy.runpod.net/v1"
      notes={[
        'Kuro does not run the machine for you. Start the server on your instance first, whatever it runs, then give Kuro the address it serves on.',
        'The base URL is the part before /chat/completions, usually ending in /v1. Kuro asks the endpoint for its model list on connect, so a wrong URL fails while you are still looking at the form rather than mid-conversation.',
        'Some engines need no key. Put any value in the key field for those; the field is required because most endpoints do want one.',
        'Cloud models appear in the same picker as local ones and are marked as leaving this machine. Tools, projects, skills and the request inspector all work the same way.',
        'An endpoint on a private network is reachable if this machine can reach it. Kuro connects from here, not from anywhere else.',
      ]}
    />
  )
}
