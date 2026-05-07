# tape-marketplace

A single-plugin Claude Code marketplace that distributes the [`tape`](./plugins/tape) plugin — a cassette tape for agent runs.

## Install

From the root of this repo:

```sh
# Add this directory as a marketplace source
/plugin marketplace add ./marketplace

# Install the tape plugin
/plugin install tape@tape-marketplace
```

The plugin registers a `tape` MCP server automatically and adds three slash commands (`/tape-resume`, `/tape-list`, `/tape-record-help`).

## Uninstall

```sh
/plugin uninstall tape@tape-marketplace
/plugin marketplace remove tape-marketplace
```

## What's inside

```
marketplace/
├── .claude-plugin/
│   └── marketplace.json
└── plugins/
    └── tape/
        ├── .claude-plugin/plugin.json
        ├── .mcp.json                       # registers the deck MCP server
        ├── README.md
        ├── bin/                            # bundled binaries (macOS arm64)
        │   ├── tape
        │   ├── tape-hook
        │   └── tape-mcp-wrap
        ├── commands/
        │   ├── tape-resume.md
        │   ├── tape-list.md
        │   └── tape-record-help.md
        └── skills/
            └── tape-usage/SKILL.md
```

## Platform note

The bundled `bin/` is built for macOS Apple Silicon at v0. For other platforms, build from source (instructions in `plugins/tape/README.md`).

## License

Apache 2.0.
