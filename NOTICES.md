# Notices

Third-party work used by Kuro.

## llama.cpp (inference engine)

- Source: https://github.com/ggml-org/llama.cpp
- Licence: MIT

Kuro downloads a prebuilt llama.cpp engine on first use into
`~/Library/Application Support/Kuro/engine/` and renames it `kuro-engine`. The binary
is not changed. The pinned version is `DEFAULT_ENGINE_TAG` in
`crates/kuro-core/src/engine/bootstrap.rs`.

## Model weights

Downloaded from Hugging Face or wherever you point Kuro. Each model has its own
licence from its publisher. Kuro does not relicense or redistribute them.

## Providers

Remote providers and your own servers have their own terms. Kuro only sends your
requests to the endpoint you set up.

## LobeHub Icons (logos)

- Source: https://github.com/lobehub/lobe-icons (`@lobehub/icons-static-svg` 1.101.0)
- Licence: MIT

The logos in `apps/web/src/assets/logos/` (also bundled in the Mac app). Some SVG
arcs were rewritten so macOS can draw them; the drawings are unchanged. The marks
belong to their owners and only show whose model or service is listed.

## impeccable (design skills)

- Source: https://github.com/pbakaus/impeccable
- Copyright: Paul Bakaus and contributors
- Licence: Apache License 2.0 (https://www.apache.org/licenses/LICENSE-2.0)

Adapted into the `design-intent` and `design-refusals` skills. The text was
rewritten and shortened for Kuro; no upstream file is included.
