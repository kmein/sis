# sis

**systemd has a sister.**

Bro is `systemctl`. Bro knows everything, but you have to ask him in full
sentences, one at a time, and he never tells you when something changes.
sis just tells you.

```
sis            # sis, what's going on?
sis --user     # sis, what's going on with *my* stuff?
sis timers     # sis, when does the backup run?
```

sis is k9s for systemd: one terminal window over `systemctl`, `journalctl`
and the whole `*ctl` family, updated live from D-Bus, driven by single keys.

## sis sees everything

The unit table is what `systemctl list-units` shows, but it moves. A unit
starts, fails, gets reloaded: the row changes within a fraction of a second,
because sis listens to the manager's signals instead of polling.

`Enter` describes a unit the way `systemctl status` would, with every
property, the cgroup's process tree and the unit file behind tabs. `l` tails
its journal. `/` filters, `:` jumps to another view, `Tab` walks through
them: units, timers, sockets, jobs, the journal, coredumps, sessions, users,
machines, network links, boot entries, `systemd-analyze` security, blame and
the boot timeline, the D-Bus name list, the user database, and a page of
`hostnamectl`, `timedatectl`, `resolvectl` and friends. `u` flips between the
system and your user manager without leaving the view.

## sis does things

Start, stop, restart, reload, enable, disable, mask, unmask, reset a
failure, kill, reload the daemon, cancel a job, terminate a session, poweroff
a machine, reconfigure a link, pick the next boot entry. sis watches the job
finish and tells you how it went. Anything destructive asks first.

## sis takes you places

`!` drops you into a shell inside a unit's namespaces and brings you back
when you leave; `ctrl-g` does the same with a debugger. The
`systemd-analyze` verbs work from the command bar: `:calendar *-*-* 04:00`,
`:critical-chain nginx.service`, `:verify foo.service`, `:cat-config
systemd/system.conf`, `:capabilities`, `:syscalls`.

Press `?` anywhere for the keys of the view you are in.

## Permissions

Reading needs no privileges: sis talks to systemd over D-Bus as whoever runs
it. Actions go through polkit, so on a desktop you get the usual prompt and
on a bare TTY the status line tells you to run as root or use `--user`.
Shells and debuggers inside units need root.

## Getting sis

```
nix run github:kmein/sis
cargo build --release
```
