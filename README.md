# prompt-bom

> AI-BOM emitter — sign and verify SPDX-AI provenance for AI-assisted code

`prompt-bom` is a single-binary Rust CLI that walks a repository, attributes each AI-generated code hunk to its source (model, session, prompt hash), and emits a signed SPDX 2.3 + SPDX-AI extension JSON file. Designed for EU AI Act / EU CRA compliance and OSS provenance trust.

## Why

AI-assisted code is now in production codebases everywhere, and there's no portable way to prove which lines came from which model with which prompt. SPDX-AI is the emerging standard. `prompt-bom` aims to be the first Rust CLI to emit and verify it.

## Status

Pre-release scaffold (v0.0.1). Emit pipeline functional; signing and verification land in later milestones.

## Install

```bash
cargo build --release
# binary at target/release/prompt-bom
```

Once published:

```bash
cargo install prompt-bom
```

## Usage

```bash
prompt-bom emit \
    --transcript path/to/claude-session.jsonl \
    --repo path/to/repo \
    --out spdx.json \
    --name my-project \
    --created 2026-04-28T00:00:00Z
```

The pipeline parses the Claude Code session JSONL, joins each AI tool-use record with the current file content via substring match and git blame, then writes a deterministic SPDX 2.3 JSON document to `--out`. Pin `--created` for byte-stable output across runs (e.g., from CI).

AI provenance is carried as structured annotations on SPDX `Snippet` entries: `annotationType: OTHER`, `annotator: "Tool: prompt-bom-<version>"`, `comment: <JSON-encoded AiProvenance>` containing `model`, `sessionId`, `attributionUuid`, `timestamp`, and optional `blame` summary. The document remains valid SPDX 2.3; downstream tools that don't understand the extension simply ignore the annotation comment.

## License

[MIT](LICENSE) — by [@verepdev](https://github.com/verepdev).
