# quark-cli Product Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Productize the fork as `quark-cli` for end users by updating GPL-compliant attribution, repository metadata, beginner-focused documentation, and GitHub Release binary automation for common platforms.

**Architecture:** Keep the existing Rust workspace and `GPL-3.0-only` license. Treat `quark` as the user-facing product and keep `libquarkpan` as an internal implementation detail in public docs except where license/developer context requires it. Use GitHub Actions to build release binaries from tagged source so every binary has matching GPL source code via the release tag.

**Tech Stack:** Rust 2024, Cargo workspace, GitHub Actions, `actions-rs`-style Cargo commands via shell, `actions/upload-artifact`, `softprops/action-gh-release`, GPLv3 compliance docs.

---

## Background And Decisions

The current project is a modified fork of a GPLv3 project. The root `LICENSE` is GNU GPL v3 text and `quarkcli/Cargo.toml` declares:

```toml
license = "GPL-3.0-only"
```

Keep GPLv3. Do not replace the license with MIT, Apache, proprietary, or a custom license. The project may be renamed, modified, republished, and distributed as binaries, but distributed binaries must be accompanied by corresponding source code under GPLv3. GitHub Releases satisfy this practically when releases are created from tagged source and include GitHub-generated source archives.

New repository URL:

```text
https://github.com/i-sync/quark-cli.git
```

User-facing product:

```text
Project: quark-cli
Cargo package: quarkcli
Installed command: quark
License: GPL-3.0-only
```

Documentation direction:

- Focus on `quark` CLI usage for beginner users.
- Do not lead with `libquarkpan` in README.
- Mention `libquarkpan` only in a short developer/attribution/license section.
- Keep low-level fid commands documented as advanced fallback commands.
- Put direct executable downloads before Cargo installation.

Release direction:

- Create GitHub Actions workflow triggered by version tags such as `v0.4.0`.
- Build release assets for:
  - Linux x86_64
  - Linux ARM64
  - macOS x86_64
  - macOS Apple Silicon
  - Windows x86_64
- Use clear asset names:
  - `quark-linux-x86_64.tar.gz`
  - `quark-linux-aarch64.tar.gz`
  - `quark-macos-x86_64.tar.gz`
  - `quark-macos-aarch64.tar.gz`
  - `quark-windows-x86_64.zip`

## File Structure

Expected files to modify or create:

- `Cargo.toml`
  - Change `homepage` and `repository` to the new GitHub repo.

- `quarkcli/Cargo.toml`
  - Keep `license = "GPL-3.0-only"`.
  - Update `description` if it still reads like a library wrapper rather than a CLI product.

- `NOTICE.md` or `ACKNOWLEDGEMENTS.md`
  - Create one concise attribution file.
  - State that this project is modified from `niuhuan/quarkpan-rs`.
  - State that the project is distributed under GPLv3.

- `README.md`
  - Rewrite as the main end-user landing document.
  - Lead with direct binary download and common `quark` commands.
  - Include beginner-friendly Cookie setup.
  - Explain large-file download resume and checksum behavior.
  - Include automation JSON examples.
  - Include a short license/attribution section.

- `quarkcli/README.md`
  - Either mirror the user-focused root README or reduce to a short crate/package-specific README.
  - Keep it consistent with the root README.

- `CHANGELOG.md`
  - Add release-productization notes.

- `.github/workflows/release.yml`
  - Create release workflow for tagged builds.

- `.github/workflows/ci.yml`
  - Optional but recommended: add CI check/test workflow for pull requests and pushes.

## Milestone 1: GPL Attribution And Repository Metadata

### Task 1: Update Repository Metadata

**Files:**
- Modify: `Cargo.toml`
- Modify: `quarkcli/Cargo.toml`

- [x] Step 1: Write a metadata expectation test using search.

Run:

```bash
rg -n "niuhuan/quarkpan-rs|github.com/i-sync/quark-cli|GPL-3.0-only" Cargo.toml quarkcli/Cargo.toml README.md quarkcli/README.md CHANGELOG.md
```

Expected before implementation:

- Old `niuhuan/quarkpan-rs` appears in root `Cargo.toml`.
- New `github.com/i-sync/quark-cli` is missing from Cargo metadata.
- `GPL-3.0-only` appears in `quarkcli/Cargo.toml`.

- [x] Step 2: Update root workspace metadata.

Change:

```toml
[workspace.package]
homepage = "https://github.com/i-sync/quark-cli"
repository = "https://github.com/i-sync/quark-cli"
```

Do not include `.git` in Cargo metadata URLs.

- [x] Step 3: Update CLI package description.

In `quarkcli/Cargo.toml`, prefer a user-facing description:

```toml
description = "Practical command-line client for Quark Drive"
```

Keep:

```toml
license = "GPL-3.0-only"
```

- [x] Step 4: Verify metadata.

Run:

```bash
cargo check --workspace
rg -n "github.com/i-sync/quark-cli|GPL-3.0-only" Cargo.toml quarkcli/Cargo.toml
```

Expected:

- `cargo check --workspace` exits 0.
- Root `Cargo.toml` points to `github.com/i-sync/quark-cli`.
- License remains `GPL-3.0-only`.

- [x] Step 5: Commit.

```bash
git add Cargo.toml quarkcli/Cargo.toml
git commit -m "build: update repository metadata"
```

### Task 2: Add Attribution Notice

**Files:**
- Create: `NOTICE.md`
- Modify: `README.md`
- Modify: `quarkcli/README.md`

- [x] Step 1: Create `NOTICE.md`.

Content:

```markdown
# Notice

`quark-cli` is a modified distribution based on the GPL-3.0-only project `quarkpan-rs` by niuhuan:

https://github.com/niuhuan/quarkpan-rs

This project keeps the GPL-3.0-only license. See `LICENSE` for the full license text.

Modifications in this distribution include CLI rebranding, path-first commands, large-file download reliability changes, JSON output, release packaging, and documentation updates.
```

- [x] Step 2: Link notice from root README.

Add a short section near the end:

```markdown
## License And Attribution

This project is distributed under `GPL-3.0-only`. It is based on `niuhuan/quarkpan-rs`; see `NOTICE.md` and `LICENSE`.
```

- [x] Step 3: Link notice from `quarkcli/README.md`.

Use the same short section or a shorter package-specific reference.

- [x] Step 4: Verify.

Run:

```bash
rg -n "NOTICE|niuhuan/quarkpan-rs|GPL-3.0-only" README.md quarkcli/README.md NOTICE.md LICENSE
cargo test --workspace
```

Expected:

- Notice links appear.
- Full workspace tests pass.

- [x] Step 5: Commit.

```bash
git add NOTICE.md README.md quarkcli/README.md
git commit -m "docs: add GPL attribution notice"
```

## Milestone 2: Beginner-Focused CLI Documentation

### Task 3: Rewrite Root README For End Users

**Files:**
- Modify: `README.md`

- [x] Step 1: Replace the root README structure.

Use this section order:

```markdown
# quark-cli

一句话说明：`quark-cli` 是面向服务器、NAS 和自动化场景的夸克网盘命令行工具，安装后的命令是 `quark`。

## 下载和安装
## 第一次使用：设置 Cookie
## 最常用命令
## 下载大文件和断点续传
## 自动化和 JSON 输出
## 诊断下载问题
## 进阶命令：FID 模式
## 常见问题
## 开发者说明
## License And Attribution
```

Do not make `libquarkpan` a top-level product section.

- [x] Step 2: Add direct download instructions.

Use placeholder release URL:

```markdown
从 GitHub Releases 下载适合你系统的文件：

- Linux x86_64: `quark-linux-x86_64.tar.gz`
- Linux ARM64: `quark-linux-aarch64.tar.gz`
- macOS Intel: `quark-macos-x86_64.tar.gz`
- macOS Apple Silicon: `quark-macos-aarch64.tar.gz`
- Windows x86_64: `quark-windows-x86_64.zip`
```

Add Linux/macOS install example:

```bash
tar -xzf quark-linux-x86_64.tar.gz
chmod +x quark
./quark --help
sudo mv quark /usr/local/bin/quark
```

Add Windows guidance:

```markdown
Windows 用户解压 zip 后，在 PowerShell 中运行：

```powershell
.\quark.exe --help
```
```

- [x] Step 3: Add beginner Cookie setup.

Include:

```bash
quark auth set-cookie --from-stdin
quark auth show-source
```

Explain:

- Cookie is private.
- Do not commit Cookie.
- Existing `quarkpan` config can be read as legacy.

- [x] Step 4: Add common commands.

Include:

```bash
quark ls /
quark get /远端/文件.mp4 ./文件.mp4
quark put ./backup.tar.gz /backup/
quark mkdir /backup/new
quark mv /backup/old.bin new.bin
quark rm /backup/old.bin --yes
quark stat /远端/文件.mp4 --json
quark shell
```

- [x] Step 5: Add large-file behavior.

Include:

```bash
quark get /tvtemp/01.mp4 ./01.mp4 --continue --retry auto
quark get /tvtemp/01.mp4 ./01.mp4 --retry infinite --retry-delay 2 --retry-max-delay 60
quark get /tvtemp/01.mp4 ./01.mp4 --retry 300 --retry-backoff fixed
```

Explain:

- `.part` means incomplete download.
- `.quark.task` stores resume metadata.
- Successful downloads are renamed after verification.
- Checksum mismatch fails by default.
- `--no-verify` exists but is not recommended.
- `reconnects:N` is normal for large files.
- `--debug` prints raw reconnect errors.

- [x] Step 6: Add automation section.

Include:

```bash
quark ls / --json
quark stat /path/file --json
quark get /path/file ./file --quiet --no-progress
```

- [x] Step 7: Add probe section.

Include:

```bash
quark probe download --fid <file_fid>
quark probe download --fid <file_fid> --json
quark --debug probe download --fid <file_fid>
```

Explain full URLs are hidden unless `--debug` is used.

- [x] Step 8: Add advanced FID section.

Keep short:

```bash
quark download --fid <file_fid> --output ./file.bin
quark download-dir --pdir-fid <folder_fid> --output ./backup
quark upload --file ./file.bin --pdir-fid 0
quark upload-dir --dir ./photos --pdir-fid 0
```

- [x] Step 9: Verify docs.

Run:

```bash
rg -n "libquarkpan" README.md
rg -n "quark ls /|quark get|--retry auto|--json|probe download|GPL-3.0-only" README.md
```

Expected:

- `libquarkpan` appears only in developer/license context, not as the main product section.
- Main user commands are present.

- [x] Step 10: Commit.

```bash
git add README.md
git commit -m "docs: rewrite README for quark users"
```

### Task 4: Align CLI Crate README

**Files:**
- Modify: `quarkcli/README.md`

- [x] Step 1: Decide README shape.

Keep `quarkcli/README.md` shorter than root README. It should include:

- What package installs.
- Quick install.
- Quick start commands.
- Pointer to root README for full guide.
- License/attribution link.

- [x] Step 2: Write concise package README.

Use:

```markdown
# quarkcli

`quarkcli` installs the `quark` command, a practical CLI for Quark Drive.

## Quick Start
...

For the full user guide, see the repository README:
https://github.com/i-sync/quark-cli
```

- [x] Step 3: Verify.

Run:

```bash
rg -n "quarkcli|quark auth|quark ls|github.com/i-sync/quark-cli|GPL-3.0-only" quarkcli/README.md
```

- [x] Step 4: Commit.

```bash
git add quarkcli/README.md
git commit -m "docs: simplify crate README"
```

## Milestone 3: GitHub Release Binary Automation

### Task 5: Add CI Workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [x] Step 1: Create `.github/workflows/ci.yml`.

Content:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    name: Check and test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Format check
        run: cargo fmt --all -- --check
      - name: Check workspace
        run: cargo check --workspace
      - name: Test workspace
        run: cargo test --workspace
      - name: Check rustls TLS variant
        run: |
          cargo check -p libquarkpan --no-default-features --features rustls
          cargo check -p quarkcli --no-default-features --features rustls
```

- [x] Step 2: Verify YAML exists.

Run:

```bash
test -f .github/workflows/ci.yml
```

- [x] Step 3: Run local equivalents.

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo check -p libquarkpan --no-default-features --features rustls
cargo check -p quarkcli --no-default-features --features rustls
```

- [x] Step 4: Commit.

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add workspace checks"
```

### Task 6: Add Release Workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [x] Step 1: Create release workflow.

Use a matrix with one job per target:

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.asset_name }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            asset_name: quark-linux-x86_64
            archive: tar.gz
          - os: ubuntu-24.04-arm
            target: aarch64-unknown-linux-gnu
            asset_name: quark-linux-aarch64
            archive: tar.gz
          - os: macos-13
            target: x86_64-apple-darwin
            asset_name: quark-macos-x86_64
            archive: tar.gz
          - os: macos-14
            target: aarch64-apple-darwin
            asset_name: quark-macos-aarch64
            archive: tar.gz
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            asset_name: quark-windows-x86_64
            archive: zip
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Build
        run: cargo build --release -p quarkcli --target ${{ matrix.target }}
      - name: Package Unix
        if: matrix.archive == 'tar.gz'
        shell: bash
        run: |
          mkdir dist
          cp target/${{ matrix.target }}/release/quark dist/quark
          cp README.md LICENSE NOTICE.md dist/
          tar -C dist -czf ${{ matrix.asset_name }}.tar.gz .
      - name: Package Windows
        if: matrix.archive == 'zip'
        shell: pwsh
        run: |
          New-Item -ItemType Directory -Path dist
          Copy-Item target/${{ matrix.target }}/release/quark.exe dist/quark.exe
          Copy-Item README.md dist/README.md
          Copy-Item LICENSE dist/LICENSE
          Copy-Item NOTICE.md dist/NOTICE.md
          Compress-Archive -Path dist/* -DestinationPath "${{ matrix.asset_name }}.zip"
      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.asset_name }}
          path: |
            ${{ matrix.asset_name }}.tar.gz
            ${{ matrix.asset_name }}.zip
          if-no-files-found: ignore

  publish:
    name: Publish GitHub Release
    runs-on: ubuntu-latest
    needs: build
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true
      - uses: softprops/action-gh-release@v2
        with:
          files: artifacts/*
          generate_release_notes: true
```

Important note:

- `ubuntu-24.04-arm` availability depends on GitHub-hosted ARM runner availability for the account/repo. If unavailable, use cross-compilation instead or initially omit Linux ARM64.

- [x] Step 2: Check workflow syntax by inspection.

Run:

```bash
rg -n "quark-linux_x86_64|quark-linux-x86_64|softprops/action-gh-release|NOTICE.md|x86_64-pc-windows-msvc" .github/workflows/release.yml
```

Expected:

- Correct asset names use hyphens, for example `quark-linux-x86_64`.
- Workflow packages `README.md`, `LICENSE`, and `NOTICE.md`.

- [x] Step 3: Run local release build smoke for host.

Run:

```bash
cargo build --release -p quarkcli
./target/release/quark --help
```

Expected:

- Build exits 0.
- Help shows command name `quark`.

- [x] Step 4: Commit.

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release binary workflow"
```

## Milestone 4: Final Verification And Release Notes

### Task 7: Final Documentation And Metadata Audit

**Files:**
- Modify: `CHANGELOG.md`
- Inspect: `Cargo.toml`, `README.md`, `quarkcli/README.md`, `NOTICE.md`, `.github/workflows/*.yml`

- [x] Step 1: Add changelog entry.

Add under the current release section:

```markdown
- Updated repository metadata to `github.com/i-sync/quark-cli`.
- Added GPL attribution notice for the upstream project.
- Reworked documentation around the `quark` command and binary downloads.
- Added CI and tagged release workflows for multi-platform binaries.
```

- [x] Step 2: Run stale metadata scan.

Run:

```bash
rg -n "niuhuan/quarkpan-rs|quarkpan-rs|libquarkpan" README.md quarkcli/README.md Cargo.toml CHANGELOG.md NOTICE.md .github/workflows
```

Expected:

- `niuhuan/quarkpan-rs` appears only in `NOTICE.md` or attribution/license sections.
- `libquarkpan` appears only in developer/license/package-context sections, not as the main README product.
- `Cargo.toml` no longer points to `niuhuan/quarkpan-rs`.

- [x] Step 3: Run full automated verification.

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo check -p libquarkpan --no-default-features --features rustls
cargo check -p quarkcli --no-default-features --features rustls
cargo build --release -p quarkcli
./target/release/quark --help
```

- [x] Step 4: Commit.

```bash
git add CHANGELOG.md
git commit -m "docs: document release packaging"
```

### Task 8: Tag And Release Procedure

**Files:**
- No source changes required.

- [x] Step 1: Confirm clean status.

Run:

```bash
git status --short
```

Expected: no unstaged or staged changes except intentionally untracked local notes.

- [ ] Step 2: Create version tag.

Use the current workspace version. For `0.4.0`:

```bash
git tag v0.4.0
git push origin main
git push origin v0.4.0
```

- [ ] Step 3: Confirm GitHub Release.

After the workflow completes on GitHub, verify the release contains:

```text
quark-linux-x86_64.tar.gz
quark-linux-aarch64.tar.gz
quark-macos-x86_64.tar.gz
quark-macos-aarch64.tar.gz
quark-windows-x86_64.zip
Source code (zip)
Source code (tar.gz)
```

- [ ] Step 4: Smoke test downloaded binary manually.

On at least one platform:

```bash
quark --help
quark auth show-source
quark ls /
```

Do not upload cookies or local config files to the repository.

## Acceptance Criteria

The work is acceptable when:

- `Cargo.toml` points to `https://github.com/i-sync/quark-cli`.
- `LICENSE` remains GPLv3 and `quarkcli/Cargo.toml` remains `GPL-3.0-only`.
- `NOTICE.md` attributes the upstream `niuhuan/quarkpan-rs` project.
- Root README is beginner-focused and leads with direct binary download/install.
- Root README focuses on `quark` CLI usage, not `libquarkpan`.
- `quarkcli/README.md` is concise and consistent with root README.
- GitHub Actions CI checks format, workspace check, workspace tests, and rustls feature checks.
- GitHub Actions release workflow builds and uploads common platform binaries on `v*` tags.
- Release archives include the executable, README, LICENSE, and NOTICE.
- Local verification commands pass:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo check -p libquarkpan --no-default-features --features rustls
cargo check -p quarkcli --no-default-features --features rustls
cargo build --release -p quarkcli
./target/release/quark --help
```
