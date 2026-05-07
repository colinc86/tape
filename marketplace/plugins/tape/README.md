# `tape` — Claude Code plugin

> A cassette tape for agent runs. Record once, replay anywhere, share as a file.

This plugin exposes the `tape` MCP server (the **deck**) inside Claude Code, so any session can load, search, fork, and annotate `.tape` files captured from prior agent runs.

## What you get on install

- **MCP server `tape`** — registers automatically; provides 11 tools (`tape.load`, `tape.summary`, `tape.tracks`, `tape.play`, `tape.seek`, `tape.tools`, `tape.diff`, `tape.fork`, `tape.record`, `tape.annotate`, `tape.eject`).
- **Slash commands**:
  - `/tape-resume <path>` — load a tape and pick up where the prior agent left off.
  - `/tape-list` — list `.tape` files in the project.
  - `/tape-record-help` — show how to start a recording (recording happens outside Claude Code).
- **Skill `tape-usage`** — context-loaded reference for the deck's call patterns and the handle-not-contents rule.
- **Bundled binaries** in `bin/`: `tape`, `tape-mcp-wrap`, `tape-hook` (Apple Silicon macOS build).

## What recording is (and isn't)

The deck (this plugin) is for **reading** existing `.tape` files inside Claude Code. **Recording** is a separate flow that runs from a shell:

```sh
tape record --task "investigate the bug" --out bug.tape -- claude <args>
```

That spawns a child `claude` process, transparently proxies its API calls, captures shell/file/MCP traffic via Claude Code's hook system, and writes a `.tape` file when the child exits. The plugin makes the `tape` binary available on your `PATH`, so the command above works in any shell after install.

## Platform support

The bundled binaries are built for **macOS (Apple Silicon)** at v0. For other platforms, build from source:

```sh
git clone https://github.com/anthropics/tape
cd tape
cargo build --release
cp target/release/{tape,tape-hook,tape-mcp-wrap} <plugin-install>/bin/
```

Cross-platform binaries ship in v0.1.

## Quick smoke test after install

After `/plugin install tape@tape-marketplace`:

1. **Verify the deck is registered**: ask Claude something like "what MCP servers are available?" — it should mention `tape` with 11 tools.
2. **Load a fixture**: download (or build yourself) one of the test fixtures — `tests/fixtures/killer-scenario-a.tape` from the source repo — and run `/tape-resume <path>`. Claude should report the task, the smoking-gun annotation, and the suggested next step.

## License

Apache 2.0.
