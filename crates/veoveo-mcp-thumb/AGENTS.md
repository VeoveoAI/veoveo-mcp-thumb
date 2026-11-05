# Thumb MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. [`DESIGN.md`](DESIGN.md) is the design and
operational contract.

## Purpose

Makes thumbnails. One tool, `thumbnail`: a file in, a small picture out. It knows nothing of what a caller
does with a thumbnail, and it holds no App and no listing.

## Invariants

- Owns the `thumb://` URI scheme. It has no tables. Identity: slug `thumb`, MCP `/thumb/mcp`, port 8807.
- Every read of a file goes through the Hub's routes or Artifact's client, as the caller, so the file's
  owner decides it. Never read Lore or object storage another way.
- Every `thumbnail` call asks the owner first, even when a picture is kept. What is kept is keyed by
  content and answers nobody by itself.
- A file is only ever parsed by a program started through the `confine` subcommand, in its own process
  group, with a time limit, no inherited environment and a folder of its own. Never run one another way,
  and never pass it a token, a person's file name or the server's paths beyond its job.
- A job's input is named by the server, never by the person.
- Programs are used as their distributions publish them. A recipe in `render.rs` is a command line.
- A tool argument never names a tenant or a person: those come from the forwarded identity.
- Ready `Routes` bind typed tool requests and readers to their output, permission and cache policy.
  Discovery, registration, annotations and dispatch derive from those routes.
- Addresses come from `ThumbResource` through the shared route declaration. Do not build or split a URI by hand.
- `skills/make-thumbnails/` is the server's skill. The setup's test holds that the instructions name every
  skill, and `tests/mcp.rs` that the manifest names exactly the skill's files.

## Build And Test

- `cargo test -p veoveo-mcp-thumb`, as the `dev` image provides it.
- `cargo test -p veoveo-mcp-thumb --no-default-features --features contract`
  checks the contract alone.
- `tests/mcp.rs` calls the server through the SDK's test Hub. `tests/confine.rs` checks the limits on a
  kernel with Landlock, and says so when it is skipped.
- `docker build --target check .` runs the gate.
