# para CLI Reference

## Install

```sh
cargo install --path /path/to/para-notes
```

Or run directly:

```sh
cargo run -- serve
```

## Commands

### serve

Serve the Notes directory as a local web UI with markdown rendering, search, and browsing.

```sh
para serve
para serve --notes-dir /path/to/Notes --port 8989
```

Options:
- `--notes-dir <path>` overrides the Notes root.
- `--port <port>` sets the HTTP port (default: `8989`).
