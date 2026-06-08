# monitr

`monitr` is a lightweight macOS activity monitor for the terminal. It is built in Rust with Ratatui and focuses on a fast process table, low overhead sampling, and an Activity Monitor-style layout.

## Features

- CPU, Memory, Energy, Disk, and Network views
- Movers view for CPU, memory, and disk-rate changes since the previous sample, each row annotated with the dominant change
- Inline CPU and memory sparklines in the overview, plus a per-process CPU trend in the inspector
- Sortable process table with fast keyboard navigation
- Process filter by PID, name, user, command, or status, with `cpu>50`, `mem<100mb`, and `field:value` predicates
- Inspector panel for the selected process
- Open files and sockets overlay for the selected process, on demand
- Disk and Network tabs include system-level volume/interface totals in the inspector
- CPU, memory, virtual memory, runtime, status, user, parent PID, command, executable, and current working directory
- Disk read/write rates and totals
- macOS thread count for the selected process
- Open file counts where the OS exposes them
- Confirmed TERM and KILL actions
- Confirmed suspend, resume, and priority-adjust actions
- One-shot process snapshots with text or JSON output
- Port ownership lookup for TCP listeners or all TCP/UDP sockets
- One-shot process inspection with open files and sockets
- Small native release binary

## Install

Install from crates.io:

```bash
cargo install --locked monitr
```

Build from source:

```bash
cargo install --locked --path .
```

Or install into `~/.local/bin` from this checkout:

```bash
make install-local
```

## Usage

```bash
monitr
```

Start with a custom refresh interval:

```bash
monitr --interval 750
```

Refresh intervals must be between `250` and `10000` milliseconds.

Start with a filter:

```bash
monitr --filter codex
```

Filters also accept predicates, ANDed together:

```bash
monitr --filter 'cpu>50 user:milo'
```

Supported predicates are `cpu`, `mem`, and `pid` with `>`, `<`, `>=`, `<=`
(memory accepts size suffixes like `100mb`), plus `user:`, `name:`, `status:`,
`cmd:`, and `pid:` substring fields. Anything else is a plain substring.

Print a one-shot process snapshot:

```bash
monitr snapshot --limit 20
```

Print machine-readable JSON for scripts:

```bash
monitr --json --filter node --limit 10
```

Find which process owns a listening port:

```bash
monitr ports 3000
```

Inspect all TCP/UDP sockets, including established connections:

```bash
monitr ports --all --json
```

Inspect one process, including open files and sockets:

```bash
monitr inspect 1234 --limit 20
```

## Controls

| Key | Action |
| --- | --- |
| `1`-`6`, `Tab` | Switch views |
| `j`/`k`, arrows | Move selection |
| `PageUp`/`PageDown`, `Home`/`End` | Jump in the process table |
| Click column header | Sort by that column: descending, then ascending, then cycle to the next key |
| `/` | Filter processes |
| `Ctrl-U` | Clear the active filter from anywhere |
| `s` | Cycle sort key |
| `S` | Reverse sort direction |
| `c`, `m`, `e`, `d`, `D`, `n`, `p`, `T`, `u` | Sort by CPU, memory, energy impact, disk write, disk read, name, PID, runtime, user |
| `i`, `Enter` | Toggle inspector |
| `o` | Show open files and sockets for the selected process |
| `t` | Send TERM after confirmation |
| `f` | Send KILL after confirmation |
| `z` | Suspend with STOP after confirmation |
| `g` | Resume with CONT after confirmation |
| `[` / `]` | Lower / raise process priority by 5 after confirmation |
| `+` | Slow the refresh interval |
| `-` | Speed up the refresh interval |
| `r` | Refresh now |
| `?` | Help |
| `q`, `Esc`, `Ctrl-C` | Quit |

## Scope

`monitr` is intended to be a faster, lighter terminal alternative to Activity Monitor for common process and system inspection. It does not use Apple's private Activity Monitor internals, so some values are approximations or interface-level summaries:

- Energy impact is a lightweight estimate based on CPU, memory share, I/O, and run state.
- Disk and Network tabs keep a process table for context, while the inspector shows system-level volumes or interfaces.
- Network throughput is interface-level, not per-process. The `ports` command identifies socket owners but does not attribute byte counts to each process.
- Some process details depend on macOS permissions and may show `-` for protected processes.

## Roadmap

The strongest opportunities for `monitr` are features that lean into terminal-native workflows or expose macOS process details that Activity Monitor does not make easy to inspect.

- Per-process network attribution: show bytes in/out and active connections by PID, not only interface-level totals. macOS exposes no cheap syscall for this, so it is deferred to an on-demand path rather than the refresh loop; the `o` overlay and `ports` command already identify socket owners.
- Better energy and wakeup data: replace the current lightweight energy estimate with richer macOS-specific signals (idle/interrupt wakeups) where public APIs expose them.
- Longer timeline windows: the history buffer that drives sparklines is in place; extend the Movers view from previous-sample deltas to 10 second, 1 minute, and 5 minute windows on top of it.
- Wider sparklines: extend the existing CPU/memory sparklines to disk and network, and into the process table when the terminal has room.
- Record and replay: export sampled sessions as JSON or CSV, then replay or diff them later for performance investigations.
- Watch rules and alerts: notify when a process exceeds CPU, memory, disk, or network thresholds, or when a matching process spawns, exits, or changes state, including a scriptable `monitr watch` for pipelines.
- Diagnostic capture: build on `monitr inspect` to collect a targeted report for a process, including process tree, sample/spindump output, and recent metric history.
- Process tree and rollups: group processes by parent tree, app bundle, launchd service, terminal session, cwd, or git repository.
- Smarter filtering: build on the new `field:value` and comparison predicates with fuzzy matching, regex, and saved filters.
- Richer anomaly explanations: the Movers view already names the dominant change per row; extend this to swap churn, dominant disk writers, network spikes, FD exhaustion risk, and short-lived process churn.

## Development

`monitr` uses Rust 2024 and requires Rust 1.88 or newer.

```bash
make check
```

This runs formatting, clippy, unit tests, and a release build.

## Releases

Pushes to `main` run CI. The release job syncs the current `package.version` to crates.io and GitHub whenever that version is not fully published yet.

In practice that means a fresh version bump publishes a new release, and a follow-up fix commit can still publish the same version if the earlier attempt never made it to crates.io. Once a version is already published from an earlier commit, bump `Cargo.toml` before the next releasable push.

Set the repository secret `CARGO_REGISTRY_TOKEN` in GitHub before relying on the publish job. The workflow also accepts `CRATES_IO_TOKEN` if you already use that name elsewhere.
