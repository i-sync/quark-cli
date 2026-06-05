# Changelog

All notable changes to this project will be documented in this file.

The format loosely follows Keep a Changelog, and this workspace currently uses semantic versioning.

## [0.4.2] - 2026-06-05

### Fixed

- Treat automatic download MD5 mismatches as warnings instead of hard failures, because Quark download metadata can disagree with the returned file bytes.

## [0.4.1] - 2026-06-05

### Fixed

- Restart a partial file from byte 0 when a resume request is rejected because the server ignores `Range`.
- Preserve `get --no-verify` when downloading directories, including shell `get` directory downloads.

## [0.4.0] - 2026-03-18

### Added

- Renamed the CLI package to `quarkcli` and the installed command to `quark`.
- Added path-first commands: `get`, `put`, `ls`, `rm`, `mkdir`, `mv`, and `stat`.
- Added `--format json` / `--json` output mode for automation.
- Added `probe download --fid <fid>` for download diagnostics.
- Added legacy `quarkpan` config lookup while writing new config to `quarkcli`.
- Added `.part` download files, checksum-failing completion, retry classification, reconnect counts, and configurable retry backoff.
- Added GPL attribution notice for the upstream project.
- Added CI and tagged release workflows for multi-platform binaries.

### Changed

- Bumped the workspace, `libquarkpan`, and `quarkcli` versions from `0.3.0` to `0.4.0`.
- Upgraded `libquarkpan` from `reqwest 0.12.x` to `reqwest 0.13.2`.
- Renamed the public TLS feature set in `libquarkpan` and `quarkcli` to align with `reqwest 0.13`, using `default-tls`, `native-tls`, `native-tls-vendored`, `rustls`, and `rustls-no-provider`.
- Updated repository metadata to `github.com/i-sync/quark-cli`.
- Reworked documentation around the `quark` command and binary downloads.

## [0.3.0] - 2026-03-18

### Changed

- Changed `libquarkpan::QuarkPan::delete` from single-`fid` deletion to batch deletion with multiple `fid` values. This is a breaking API change.
- Added `quark delete` and batch deletion via repeated `--fid` arguments.
- Bumped the workspace, `libquarkpan`, and `quarkcli` versions from `0.2.0` to `0.3.0`.
- Added selectable TLS backend features for `libquarkpan` and `quarkcli`, with `rustls-tls` as the default and explicit forwarding for `native-tls` and other `reqwest` TLS variants.

## [0.1.0] - 2026-03-15

### Added

- Added `libquarkpan` as the async core library for Quark Drive.
- Added `QuarkPan::builder()` as the main client entry point.
- Added folder creation and folder listing support.
- Added file download by file ID with stream-based consumption.
- Added upload preparation, rapid-upload detection, and chunked upload support.
- Added `quarkcli` CLI with `list`, `folder create`, `download`, and `upload`.

## [0.2.0] - 2026-03-17

### Changed

- Changed public library builders and CLI parameters to align with Quark API naming, using `fid`, `pdir_fid`, and `file_name` instead of mixed semantic aliases such as `file_id`, `folder_id`, `parent_folder`, and `name`.
- Changed upload and download result payloads in `libquarkpan` and `quarkcli` output structures to use `fid`.
- Changed examples and READMEs to use the new naming consistently.

## [0.1.1] - 2026-03-15

### Added

- Added resumable CLI transfer flows backed by `.quark.task` files.
- Added transfer progress reporting and Ctrl+C cancellation support.
- Added persistent cookie support for the CLI using the platform config directory.
- Added `auth` CLI commands for saving, importing, clearing, and inspecting cookie source.
- Added `download-dir` and `upload-dir` commands with directory task files.
- Added explicit retry logic for interrupted downloads and multipart uploads.
- Added file deletion support in `libquarkpan` for overwrite workflows.
- Added file and folder rename support in `libquarkpan` and the `quark rename` command.
- Expanded the `quark` CLI with `auth`, `download-dir`, and `upload-dir`.

### Changed

- Removed `reqwest-retry` and `reqwest-middleware` in favor of explicit retry control.
- Changed upload prepare flow to a single `prepare().await` call.
- Standardized directory task files to use a sibling `目录名.quark.task` naming scheme.
- Ctrl+C now stops active transfer attempts immediately and preserves task files for resume.
- Removed the CLI `--json` mode and standardized on human-readable output.
- `auth set-cookie` now uses explicit input sources: `--cookie`, `--from-stdin`, `--from-nano`, and `--from-vi`.
- Progress bars are now shown only on interactive TTY sessions and include the current file name.
- Upload completion now flushes the progress bar to a final 100% state before finishing.
- Download md5 verification now supports Quark's base64-encoded remote md5 values.
- `auth set-cookie --from-stdin` now prints a prompt before reading a single-line cookie.
