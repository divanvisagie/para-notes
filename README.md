# para-notes

`para` is a local web server for browsing a PARA Notes directory with markdown
rendering, search, and directory navigation. It embeds the CSS, JS, and fonts
needed for offline use.

## Install

```sh
cargo install --path /path/to/para-notes
```

Or run directly:

```sh
cargo run -- serve
```

## CLI

See [`docs/cli.md`](docs/cli.md) for command usage.

## macOS daemon

Use the launchd helper to install and load a user-level service:

```sh
resources/launchd/install.sh
```

To stop or restart the service:

```sh
launchctl unload -w ~/Library/LaunchAgents/com.divanv.para.serve.plist
launchctl load -w ~/Library/LaunchAgents/com.divanv.para.serve.plist
```
