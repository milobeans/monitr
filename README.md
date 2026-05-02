# rmon

`rmon` is a lightweight macOS activity monitor for the terminal. It is built in Rust with Ratatui and focuses on a fast process table, low overhead sampling, and an Activity Monitor-style layout.

## Features

- CPU, Memory, Energy, Disk, and Network views
- Sortable process table with fast keyboard navigation
- Process filter by PID, name, user, command, or status
- Inspector panel for the selected process
- CPU, memory, virtual memory, runtime, status, user, parent PID, command, executable, and current working directory
- Disk read/write rates and totals
- macOS thread count for the selected process
- Open file counts where the OS exposes them
- Confirmed TERM and KILL actions
- Small native release binary

## Install

The crate is prepared for crates.io as `rmon-macos`. Once published:

```bash
cargo install rmon-macos
```

Build from source:

```bash
cargo install --path .
```

Or install into `~/.local/bin` from this checkout:

```bash
make install-local
```

## Usage

```bash
rmon
```

Start with a custom refresh interval:

```bash
rmon --interval 750
```

Start with a filter:

```bash
rmon --filter codex
```

## Controls

| Key | Action |
| --- | --- |
| `1`-`5`, `Tab` | Switch views |
| `j`/`k`, arrows | Move selection |
| `PageUp`/`PageDown`, `Home`/`End` | Jump in the process table |
| `/` | Filter processes |
| `s` | Cycle sort key |
| `S` | Reverse sort direction |
| `c`, `m`, `e`, `d`, `n`, `p`, `u` | Sort by CPU, memory, energy impact, disk, name, PID, user |
| `i`, `Enter` | Toggle inspector |
| `t` | Send TERM after confirmation |
| `f` | Send KILL after confirmation |
| `+`, `-` | Adjust refresh interval |
| `r` | Refresh now |
| `?` | Help |
| `q`, `Esc` | Quit |

## Scope

`rmon` is intended to be a faster, lighter terminal alternative to Activity Monitor for common process and system inspection. It does not use Apple's private Activity Monitor internals, so some values are approximations or interface-level summaries:

- Energy impact is a lightweight estimate based on CPU, memory share, I/O, and run state.
- Network data is interface-level, not per-process.
- Some process details depend on macOS permissions and may show `-` for protected processes.

## Development

```bash
make check
```

This runs formatting, clippy, unit tests, and a release build.
