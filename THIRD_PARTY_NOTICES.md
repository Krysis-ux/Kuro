# Third-party notices

Kuro is a model server, not an inference implementation. Two categories of
third-party material reach a running install, and neither is redistributed by
this repository — both are fetched at runtime, on the user's machine, from the
publisher's own servers.

## Inference engine

Kuro runs GGUF models through a prebuilt engine executable that it downloads on
first use into `~/Library/Application Support/Kuro/engine/`. That executable is
a build of **llama.cpp** (<https://github.com/ggml-org/llama.cpp>), published by
the ggml-org project and its contributors under the **MIT Licence**. It remains
the property of its authors.

Kuro renames the downloaded executable to `kuro-engine` on install. This is a
filename change only, made so that the process Kuro supervises is identifiable
as belonging to Kuro in the process list and in logs. The binary itself is
unmodified, and the rename does not change its authorship, its licence, or the
terms under which it is provided.

The release tag Kuro pins is recorded in `DEFAULT_ENGINE_TAG` in
`crates/kuro-core/src/engine/bootstrap.rs`, and the releases it fetches from are
recorded alongside it in the same file.

## Model weights

Model weights are downloaded from Hugging Face, or from wherever the user points
Kuro. Each model carries its own licence, granted by its publisher and accepted
by the user with that publisher. Kuro neither relicenses nor redistributes them,
and does not vet them on the user's behalf.

## Remote providers and endpoints

Kuro can connect to third-party API providers and to servers the user runs
themselves. Those services are governed by their own terms, agreed between the
user and the operator. Kuro passes requests to the endpoint the user configured
and holds no relationship with the operator.
