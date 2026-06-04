# quarkcli

`quarkcli` is the Cargo package for the `quark` command-line client.

Install it when you want to manage Quark Drive files from a terminal, script, NAS, or server. The installed executable is named `quark`, not `quarkcli`.

## Install

From Cargo:

```bash
cargo install quarkcli
```

From source:

```bash
cargo install --git https://github.com/i-sync/quark-cli quarkcli
```

Prebuilt binaries for Linux, macOS, and Windows are published from the repository releases:

https://github.com/i-sync/quark-cli

## Quick Start

Save your Quark Drive web Cookie:

```bash
quark auth set-cookie --from-stdin
```

List files:

```bash
quark ls /
```

Download a file:

```bash
quark get /remote/file.mp4 ./file.mp4
```

Upload a file:

```bash
quark put ./backup.tar.gz /backup/
```

Resume and retry a large download:

```bash
quark get /tvtemp/01.mp4 ./01.mp4 --continue --retry auto
```

Use JSON in scripts:

```bash
quark ls / --json
quark stat /remote/file.mp4 --json
```

Run the interactive shell:

```bash
quark shell
```

For the full beginner guide, release downloads, large-file notes, and troubleshooting commands, see:

https://github.com/i-sync/quark-cli

## License And Attribution

`GPL-3.0-only`

This package is part of `quark-cli`, a modified distribution based on `niuhuan/quarkpan-rs`. See the repository `NOTICE.md` and `LICENSE`.
