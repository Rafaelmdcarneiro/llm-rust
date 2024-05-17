# `llm` - Large Language Models for Everyone, in Rust

`llm` is an ecosystem of Rust libraries for working with large language models -
it's built on top of the fast, efficient [GGML](./crates/ggml) library for
machine learning.

![A llama riding a crab, AI-generated](./doc/img/llm-crab-llama.png)

> _Image by [@darthdeus](https://github.com/darthdeus/), using Stable Diffusion_

[![Latest version](https://img.shields.io/crates/v/llm.svg)](https://crates.io/crates/llm)
![MIT/Apache2](https://shields.io/badge/license-MIT%2FApache--2.0-blue)
[![Discord](https://img.shields.io/discord/1085885067601137734)](https://discord.gg/YB9WaXYAWU)

## Current State

There are currently four available versions of `llm` (the crate and the CLI):

- The released version `0.1.1` on `crates.io`. This version is several months out of date and does not include support for the most recent models.
- The `main` branch of this repository. This version can reliably infer GGMLv3 models, but does not support GGUF, and uses an old version of GGML.
- The `gguf` branch of this repository; this is a version of `main` that supports inferencing with GGUF, but does not support any models other than Llama, requires the use of a Hugging Face tokenizer, and does not support quantization. It also uses an old version of GGML.
- The `develop` branch of this repository. This is a from-scratch re-port of `llama.cpp` to synchronize with the latest version of GGML, and to support all models and GGUF. It is currently a work in progress, and is not yet ready for use.

The plan is to finish up the work on `develop` (see [the PR](https://github.com/rustformers/llm/pull/442)), and then merge it into `main` and release a new version of `llm` to `crates.io`, so that up-to-date support for the latest models and GGUF will be available. It is not yet known when this will happen.

## Overview

The primary entrypoint [for developers](#using-llm-in-a-rust-project) is
[the `llm` crate](./crates/llm), which wraps [`llm-base`](./crates/llm-base) and
the [supported model](./crates/models) crates.
[Documentation](https://docs.rs/llm) for released version is available on
Docs.rs.

For end-users, there is [a CLI application](#using-the-llm-cli),
[`llm-cli`](./binaries/llm-cli), which provides a convenient interface for
interacting with supported models. [Text generation](#running) can be done as a
one-off based on a prompt, or interactively, through
[REPL or chat](#does-the-llm-cli-support-chat-mode) modes. The CLI can also be
used to serialize (print) decoded models,
[quantize](./crates/ggml/README.md#quantization) GGML files, or compute the
[perplexity](https://huggingface.co/docs/transformers/perplexity) of a model. It
can be downloaded from
[the latest GitHub release](https://github.com/rustformers/llm/releases) or by
installing it from `crates.io`.

`llm` is powered by the [`ggml`](https://github.com/ggerganov/ggml) tensor
library, and aims to bring the robustness and ease of use of Rust to the world
of large language models. At present, inference is only on the CPU, but we hope
to support GPU inference in the future through alternate backends.

Currently, the following models are supported:

- [BLOOM](https://huggingface.co/docs/transformers/model_doc/bloom)
- [GPT-2](https://huggingface.co/docs/transformers/model_doc/gpt2)
- [GPT-J](https://huggingface.co/docs/transformers/model_doc/gptj)
- [GPT-NeoX](https://huggingface.co/docs/transformers/model_doc/gpt_neox)
  (includes [StableLM](https://github.com/Stability-AI/StableLM),
  [RedPajama](https://www.together.xyz/blog/redpajama), and
  [Dolly 2.0](https://www.databricks.com/blog/2023/04/12/dolly-first-open-commercially-viable-instruction-tuned-llm))
- [LLaMA](https://huggingface.co/docs/transformers/model_doc/llama) (includes
  [Alpaca](https://crfm.stanford.edu/2023/03/13/alpaca.html),
  [Vicuna](https://lmsys.org/blog/2023-03-30-vicuna/),
  [Koala](https://bair.berkeley.edu/blog/2023/04/03/koala/),
  [GPT4All](https://gpt4all.io/index.html), and
  [Wizard](https://github.com/nlpxucan/WizardLM))
- [MPT](https://www.mosaicml.com/blog/mpt-7b)

See [getting models](#getting-models) for more information on how to download supported models.

## Using `llm` in a Rust Project

This project depends on Rust v1.65.0 or above and a modern C toolchain.

The `llm` crate exports `llm-base` and the model crates (e.g. `bloom`, `gpt2`
`llama`).

Add `llm` to your project by listing it as a dependency in `Cargo.toml`. To use
the version of `llm` you see in the `main` branch of this repository, add it
from GitHub (although keep in mind this is pre-release software):

```toml
[dependencies]
llm = { git = "https://github.com/rustformers/llm" , branch = "main" }
```

To use a [released](https://github.com/rustformers/llm/releases) version, add it
from [crates.io](https://crates.io/crates/llm) by specifying the desired
version:

```toml
[dependencies]
llm = "0.1"
```

By default, `llm` builds with support for remotely fetching the tokenizer from Hugging Face's model hub.
To disable this, disable the default features for the crate, and turn on the `models` feature to get `llm`
without the tokenizer:

```toml
[dependencies]
llm = { version = "0.1", default-features = false, features = ["models"] }
```

**NOTE**: To improve debug performance, exclude the transitive `ggml-sys`
dependency from being built in debug mode:

```toml
[profile.dev.package.ggml-sys]
opt-level = 3
```

## Leverage Accelerators with `llm`

The `llm` library is engineered to take advantage of hardware accelerators such as `cuda` and `metal` for optimized performance.

To enable `llm` to harness these accelerators, some preliminary configuration steps are necessary, which vary based on your operating system. For comprehensive guidance, please refer to [Acceleration Support](doc/acceleration-support.md) in our documentation.

## Using `llm` from Other Languages

Bindings for this library are available in the following languages:

- Python: [LLukas22/llm-rs-python](https://github.com/LLukas22/llm-rs-python)
- Node: [Atome-FE/llama-node](https://github.com/Atome-FE/llama-node)

## Using the `llm` CLI

The easiest way to get started with `llm-cli` is to download a pre-built
executable from a [released](https://github.com/rustformers/llm/releases)
version of `llm`, but the releases are currently out of date and we recommend
you [install from source](#installing-from-source) instead.

### Installing from Source

To install the `main` branch of `llm` with the most recent features to your Cargo `bin`
directory, which `rustup` is likely to have added to your `PATH`, run:

```shell
cargo install --git https://github.com/rustformers/llm llm-cli
```

The CLI application can then be run through `llm`. See also [features](#features) and
[acceleration support](doc/acceleration-support.md) to turn features on as required.
Note that GPU support (CUDA, OpenCL, Metal) will not work unless you build with the relevant feature.

### Installing with `cargo`

Note that the currently published version is out of date and does not include
support for the most recent models. We currently recommend that you
[install from source](#installing-from-source).

To install the most recently released version of `llm` to your Cargo `bin`
directory, which `rustup` is likely to have added to your `PATH`, run:

```shell
cargo install llm-cli
```

The CLI application can then be run through `llm`. See also [features](#features)
to turn features on as required.

### Features

By default, `llm` builds with support for remotely fetching the tokenizer from Hugging Face's model hub.
This adds a dependency on your system's native SSL stack, which may not be available on all systems.

To disable this, disable the default features for the build:

```shell
cargo build --release --no-default-features
```

To enable hardware acceleration, see [Acceleration Support for Building section](doc/acceleration-support.md), which is also applicable to the CLI.

## Getting Models

GGML models are easy to acquire. They are primarily located on Hugging Face
(see [From Hugging Face](#from-hugging-face)), but can be obtained from elsewhere.

Models are distributed as single files, and do not need any additional files to
be downloaded. However, they are quantized with different levels of precision,
so you will need to choose a quantization level that is appropriate for your
application.

Additionally, we support Hugging Face tokenizers to improve the quality of
tokenization. These are separate files (`tokenizer.json`) that can be used
with the CLI using the `-v` or `-r` flags, or with the `llm` crate by
using the appropriate `TokenizerSource` enum variant.

For a list of models that have been tested, see the
[known-good models](./doc/known-good-models.md).

Certain older GGML formats are not supported by this project, but the goal is to
maintain feature parity with the upstream GGML project. For problems relating to
loading models, or requesting support for
[supported GGML model types](https://github.com/ggerganov/ggml#roadmap), please
[open an Issue](https://github.com/rustformers/llm/issues/new).

### From Hugging Face

Hugging Face 🤗 is a leader in open-source machine learning and hosts hundreds
of GGML models.
[Search for GGML models on Hugging Face 🤗](https://huggingface.co/models?search=ggml).

### r/LocalLLaMA

This Reddit community maintains a wiki related to GGML
models, including well organized lists of links for acquiring
[GGML models](https://www.reddit.com/r/LocalLLaMA/wiki/models/) (mostly from
Hugging Face 🤗).

## Usage

Once the `llm` executable has been built or is in a `$PATH` directory, try
running it. Here's an example that uses the open-source
[RedPajama](https://huggingface.co/rustformers/redpajama-ggml/blob/main/RedPajama-INCITE-Base-3B-v1-q4_0.bin)
language model:

```shell
llm infer -a gptneox -m RedPajama-INCITE-Base-3B-v1-q4_0.bin -p "Rust is a cool programming language because" -r togethercomputer/RedPajama-INCITE-Base-3B-v1
```

In the example above, the first two arguments specify the model architecture and
command, respectively. The required `-m` argument specifies the local path to
the model, and the required `-p` argument specifies the evaluation prompt. The
optional `-r` argument is used to load the model's tokenizer from a remote
Hugging Face 🤗 repository, which will typically improve results when compared
to loading the tokenizer from the model file itself; there is also an optional
`-v` argument that can be used to specify the path to a local tokenizer file.
For more information about the `llm` CLI, use the `--help` parameter.

There is also a [simple inference example](./crates/llm/examples/inference.rs)
that is helpful for [debugging](./.vscode/launch.json):

```shell
cargo run --release --example inference gptneox RedPajama-INCITE-Base-3B-v1-q4_0.bin -r $OPTIONAL_VOCAB_REPO -p $OPTIONAL_PROMPT
```

## Q&A

### Does the `llm` CLI support chat mode?

Yes, but certain fine-tuned models (e.g.
[Alpaca](https://crfm.stanford.edu/2023/03/13/alpaca.html),
[Vicuna](https://lmsys.org/blog/2023-03-30-vicuna/),
[Pygmalion](https://docs.alpindale.dev/)) are more suited to chat use-cases than
so-called "base models". Here's an example of using the `llm` CLI in REPL
(Read-Evaluate-Print Loop) mode with an Alpaca model - note that the
[provided prompt format](./utils/prompts/alpaca.txt) is tailored to the model
that is being used:

```shell
llm repl -a llama -m ggml-alpaca-7b-q4.bin -f utils/prompts/alpaca.txt
```

There is also a [Vicuna chat example](./crates/llm/examples/vicuna-chat.rs) that
demonstrates how to create a custom chatbot:

```shell
cargo run --release --example vicuna-chat llama ggml-vicuna-7b-q4.bin
```

### Can `llm` sessions be persisted for later use?

Sessions can be loaded (`--load-session`) or saved (`--save-session`) to file.
To automatically load and save the same session, use `--persist-session`. This
can be used to cache prompts to reduce load time, too.

### How do I use `llm` to quantize a model?

`llm` can produce a `q4_0`- or
`q4_1`-[quantized](./crates/ggml/README.md#quantization) model from an
`f16`-quantized GGML model

```shell
cargo run --release quantize -a $MODEL_ARCHITECTURE $MODEL_IN $MODEL_OUT {q4_0,q4_1}
```

### Do you provide support for Docker and NixOS?

The `llm` [Dockerfile](./utils/Dockerfile) is in the `utils` directory; the
[NixOS flake](./flake.nix) manifest and lockfile are in the project root.

### What's the best way to get in touch with the `llm` community?

GitHub [Issues](https://github.com/rustformers/llm/issues/new) and
[Discussions](https://github.com/rustformers/llm/discussions/new) are welcome,
or come chat on [Discord](https://discord.gg/YB9WaXYAWU)!

### Do you accept contributions?

Absolutely! Please see the [contributing guide](./doc/CONTRIBUTING.md).

### What applications and libraries use `llm`?

#### Applications

- [llmcord](https://github.com/rustformers/llmcord): Discord bot for generating
  messages using `llm`.
- [local.ai](https://github.com/louisgv/local.ai): Desktop app for hosting an
  inference API on your local machine using `llm`.
- [secondbrain](https://github.com/juliooa/secondbrain): Desktop app to download and run LLMs locally in your computer using `llm`.
- [floneum](https://floneum.com/): A graph editor for local AI workflows.
- [poly](https://github.com/pixelspark/poly): A versatile LLM serving back-end with tasks, streaming completion, memory retrieval, and more.

#### Libraries

- [llm-chain](https://github.com/sobelio/llm-chain): Build chains in large
  language models for text summarization and completion of more complex tasks
