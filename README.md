# lazypost

`lazypost` is a terminal-first API scanner and debugger for Spring MVC / Spring Boot projects.

It statically scans Java source files, extracts controller routes and request parameters, and lets you inspect and invoke APIs from a TUI without starting the Java app for reflection-based metadata discovery.

## Features

- Scan Spring MVC / Spring Boot controllers from `.java` source files
- Extract HTTP method, path, path/query/header/body bindings, and endpoint descriptions
- Generate request body drafts from DTO definitions
- Browse APIs in a TUI and edit Params / Headers / Body independently
- Send requests directly and inspect responses in folded JSON or raw-response popup mode
- Persist per-project request drafts locally

## Install

### Homebrew

Install directly from this repository's formula:

```bash
brew install https://raw.githubusercontent.com/life2you/lazypost4J/main/Formula/lazypost.rb
```

### Cargo

```bash
cargo install --path .
```

## Usage

Launch the TUI:

```bash
lazypost /path/to/java-project
```

Scan only:

```bash
lazypost scan /path/to/java-project
```

Scan and print JSON:

```bash
lazypost scan /path/to/java-project --json
```

## TUI shortcuts

- `1..7`: switch modules
- `e`: edit current module (`2..6`)
- `s`: send request
- `v`: open raw response popup from response module
- `f`: fuzzy search APIs
- `?`: help
- `q`: quit

## Local config

`lazypost` stores local state in the system config directory:

- macOS: `~/Library/Application Support/lazypost/config.json`
- Linux: `~/.config/lazypost/config.json`

Stored data includes:

- recent projects
- base URLs
- global request headers
- per-project API request drafts

## Development

- Development notes: [DEVELOPMENT.md](./DEVELOPMENT.md)
- Product notes: [api_tui_v_1_prd_and_srd.md](./api_tui_v_1_prd_and_srd.md)

## License

Licensed under either of:

- Apache License, Version 2.0, see [LICENSE-APACHE](./LICENSE-APACHE)
- MIT license, see [LICENSE-MIT](./LICENSE-MIT)

