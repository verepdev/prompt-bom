# prompt-bom

> AI-BOM emitter — sign and verify SPDX-AI provenance for AI-assisted code

`prompt-bom` is a single-binary Rust CLI that walks a repository, attributes each AI-generated code hunk to its source (model, session, prompt hash), and emits a signed SPDX 2.3 + SPDX-AI extension JSON file. Designed for EU AI Act / EU CRA compliance and OSS provenance trust.

## Why

AI-assisted code is now in production codebases everywhere, and there's no portable way to prove which lines came from which model with which prompt. SPDX-AI is the emerging standard. `prompt-bom` aims to be the first Rust CLI to emit and verify it.

## Status

Pre-release scaffold (v0.0.1). No usable features yet — see milestones in repo.

## Install

Once published:

```bash
cargo install prompt-bom
```

## License

[MIT](LICENSE) — by [@verepdev](https://github.com/verepdev).
