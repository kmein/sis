# sis

k9s for systemd: a terminal UI over `systemctl`, `journalctl` and the rest of
the `*ctl` family. One process, live updates, single-key actions.

```
sis            # the system manager
sis --user     # your user manager
sis timers     # start in another view
```

The main screen is the unit table. It refreshes from D-Bus signals within a
fraction of a second, shows what `systemctl list-units` shows (press `ctrl-a`
for everything), and enriches the rows on screen with PID, memory and age.

## Keys

| Key | Action |
|---|---|
| `:` | command bar; `Tab` completes the common prefix, then cycles through the candidates (`Shift-Tab` backwards) |
| `/` | filter the current view (`Esc` clears) |
| `?` | help overlay with every key of the current view |
| `Esc` | back; `q` quits at the root, `ctrl-c` anywhere |
| `Tab` `Shift-Tab` | next / previous view in the strip under the header (at the root) |
| `j` `k` `ctrl-d` `ctrl-u` `g` `G` | move |
| `Enter` | describe (`systemctl status`-like tabs: status, properties, cgroup, file) |
| `l` | logs: a followed `journalctl` tail with `f` follow, `w` wrap, `p` priority, `/` filter |
| `s` `x` `r` `R` | start, stop, restart, reload |
| `e` `d` `m` `M` | enable, disable, mask, unmask |
| `f` `c` `ctrl-k` `D` | reset-failed, cat unit file, kill (SIGTERM), daemon-reload |
| `!` `ctrl-g` | `systemd-analyze unit-shell` / `unit-gdb`: leave the TUI, get a shell or a debugger inside the unit, come back (needs root) |
| `C` `V` `Y` | `systemd-analyze` critical-chain, verify, dump for the unit |
| `u` | switch between the system and user manager |
| `1`…`5`, `0` | only services, timers, sockets, targets, mounts, or all |
| `N` `A` `T` `L` `M` | sort by name, active state, type, load, memory (again: flip) |
| `S` | show the `systemd-analyze security` exposure column |
| `ctrl-a` | show inactive units too |
| `ctrl-r` | refresh now |

Anything that stops, kills, masks or disables asks first.

## Views

| `:command` | Source | Row actions |
|---|---|---|
| `units` | `ListUnits` + `ListUnitFiles` over D-Bus | all of the above |
| `timers` | `systemctl list-timers` | describe, logs of the activated unit, start/stop/enable |
| `sockets` | `systemctl list-sockets` | same as timers |
| `jobs` | `ListJobs` over D-Bus | describe, logs, `x` cancel |
| `coredumps` | `coredumpctl list` (newest 2000) | `Enter` info, `l` logs of the pid |
| `sessions` | `loginctl list-sessions` | status, logs, `x` terminate, `L` lock |
| `users` | `loginctl list-users` | status, logs, `x` terminate, `L` toggle linger |
| `seats` | `loginctl list-seats` | status |
| `machines` | `machinectl list` | status, logs, start, `x` poweroff, `ctrl-k` terminate, `r` reboot, `!` shell |
| `links` | `networkctl list` | status, networkd logs, `r` reconfigure, `s` up, `x` down |
| `boot` | `bootctl list` | entry, `D` set-default, `O` boot once |
| `security` | `systemd-analyze security` | analysis, `c` describe unit |
| `blame` | `systemd-analyze blame` | describe, logs |
| `plot` | `systemd-analyze plot --json` as a table: when each unit started, how long it took | describe, logs, `C` critical chain |
| `critical-chain`, `unit-files`, `unit-paths`, `exit-status`, `capabilities`, `syscalls`, `filesystems`, `architectures` | the matching `systemd-analyze` verb | text, `ctrl-r` reruns |
| `bus` | `busctl list` | tree, `i` introspect, `s` status, `U` describe unit |
| `userdb`, `groups` | `userdbctl` | record |
| `info` | hostnamectl, timedatectl, localectl, resolvectl, systemd-analyze time, oomctl | `ctrl-r` reruns |

`:user` and `:system` switch managers, `:all` toggles inactive units,
`:help` and `:quit` do what they say. `systemd-analyze` verbs that take an
argument work from the command bar too: `:calendar *-*-* 04:00`,
`:timespan 1h 30min`, `:condition ConditionPathExists=/etc`,
`:cat-config systemd/system.conf`, `:verify foo.service`, `:dump nginx*`,
`:critical-chain nginx.service`, `:security nginx.service`.

## Permissions

sis talks to systemd over D-Bus, so it needs no root to look. Actions go
through polkit: on a desktop you get the usual authentication prompt, on a
bare TTY the status line tells you to run as root or use `--user`.

## Building

```
cargo build --release
nix build            # wraps the binary with systemd's tools on PATH
```

Set `--log-file FILE` together with `RUST_LOG=debug` to see what sis does.

## Design

`src/app.rs` is a synchronous state machine: events in, effects out. The
tokio runtime in `src/runtime.rs` executes the effects (D-Bus calls, journal
subprocesses, external commands) and feeds results back as events. Every
table is a `Resource` (columns, rows from the shared `Store`, key bindings)
rendered by one generic `TableView`; adding a view means one file in
`src/resources/` and one line in the registry.
