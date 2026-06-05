# quark-cli

`quark-cli` 是一个面向夸克网盘的命令行工具，安装后使用的命令名是 `quark`。它适合在服务器、NAS、脚本和日常终端里上传、下载、查看、移动、删除夸克网盘文件。

这个 README 面向第一次使用命令行工具的用户，先说明怎么下载安装，再说明怎么登录和完成常见操作。

## 下载和安装

推荐从 GitHub Releases 下载已经编译好的可执行文件，不需要本地安装 Rust 或 Cargo。

常见平台对应文件名：

- Linux x86_64: `quark-linux-x86_64.tar.gz`
- Linux ARM64: `quark-linux-aarch64.tar.gz`
- macOS Apple Silicon: `quark-macos-aarch64.tar.gz`
- Windows x86_64: `quark-windows-x86_64.zip`

Linux x86_64 包在 Ubuntu 22.04 环境构建，适合 Debian 12、Ubuntu 22.04 或更新的 glibc 系统。

Linux 或 macOS 示例：

```bash
tar -xzf quark-linux-x86_64.tar.gz
chmod +x quark
./quark --help
sudo mv quark /usr/local/bin/quark
```

Windows 示例：

```powershell
.\quark.exe --help
```

如果你想从源码安装，仍然可以使用 Cargo：

```bash
cargo install --git https://github.com/i-sync/quark-cli quarkcli
```

Cargo 包名是 `quarkcli`，安装后的命令名仍然是 `quark`。

## 第一次使用：设置 Cookie

`quark` 使用夸克网盘网页端 Cookie 访问你的账号。Cookie 是私人凭据，不要发给别人，不要提交到 Git 仓库，也不要写进公开脚本。

推荐从标准输入保存 Cookie：

```bash
quark auth set-cookie --from-stdin
```

执行后把 Cookie 粘贴进去，然后按 `Ctrl+D` 结束输入。Windows PowerShell 里可按 `Ctrl+Z` 后回车结束。

查看当前 Cookie 来源：

```bash
quark auth show-source
```

也可以临时使用环境变量：

```bash
QUARK_COOKIE='你的 Cookie' quark ls /
```

或者使用文件：

```bash
quark --cookie-file ./cookie.txt ls /
```

如果你以前用过旧的 `quarkpan` 配置，`quark` 会在新的配置不存在时兼容读取旧配置，方便迁移。

## 最常用命令

查看根目录：

```bash
quark ls /
```

查看某个目录：

```bash
quark ls /tvtemp
```

下载文件：

```bash
quark get /远端/文件.mp4 ./文件.mp4
```

上传文件到目录：

```bash
quark put ./backup.tar.gz /backup/
```

创建目录：

```bash
quark mkdir /backup/new
```

重命名同一目录下的文件或文件夹：

```bash
quark mv /backup/old.bin new.bin
```

删除文件或目录：

```bash
quark rm /backup/old.bin --yes
```

查看文件详情：

```bash
quark stat /远端/文件.mp4
quark stat /远端/文件.mp4 --json
```

进入交互式 shell：

```bash
quark shell
```

在 shell 里可以使用 `ls`、`cd`、`get`、`put`、`mkdir`、`mv`、`rm`、`stat` 等命令，适合连续操作同一个目录。

## 下载大文件和断点续传

大文件下载建议开启续传和自动重试：

```bash
quark get /tvtemp/01.mp4 ./01.mp4 --continue --retry auto
```

网络很不稳定时，可以无限重试并设置延迟上限：

```bash
quark get /tvtemp/01.mp4 ./01.mp4 --retry infinite --retry-delay 2 --retry-max-delay 60
```

也可以指定最多重试次数，并使用固定间隔：

```bash
quark get /tvtemp/01.mp4 ./01.mp4 --retry 300 --retry-backoff fixed
```

下载过程会先写入 `.part` 文件，恢复信息保存在 `.quark.task` 文件里。下载完成后，程序会先校验 `.part`，校验通过再重命名为最终文件。

如果服务端提供 md5，默认会校验文件完整性。校验失败会报错，并保留 `.part` 和 `.quark.task`，方便排查或继续处理。`--no-verify` 可以跳过校验，但不推荐日常使用。

进度条里出现 `reconnects:N` 表示下载流断开后已经自动重连了 N 次。需要看每次断线的详细原因时，加 `--debug`：

```bash
quark --debug get /tvtemp/01.mp4 ./01.mp4 --continue --retry auto
```

## 自动化和 JSON 输出

脚本里建议使用 JSON 输出，方便用 `jq`、Python 或其他工具处理。

```bash
quark ls / --json
quark stat /path/file --json
quark get /path/file ./file --quiet --no-progress
```

普通表格和 JSON 会输出到 stdout，进度条和 debug 信息会输出到 stderr，便于脚本分离数据和日志。

## 诊断下载问题

如果某个文件下载慢、频繁断开，或者你想确认下载链接、md5、Range 支持情况，可以使用探测命令：

```bash
quark probe download --fid <file_fid>
quark probe download --fid <file_fid> --json
quark --debug probe download --fid <file_fid>
```

默认输出不会打印完整下载 URL，避免把敏感链接暴露到日志里。只有开启 `--debug` 时才会显示完整 URL，并标记为 sensitive。

## 进阶命令：FID 模式

大多数用户优先使用路径命令，例如 `quark get /目录/文件 ./文件`。如果你已经知道夸克网盘的 FID，也可以使用低层 FID 命令：

```bash
quark download --fid <file_fid> --output ./file.bin
quark download-dir --pdir-fid <folder_fid> --output ./backup
quark upload --file ./file.bin --pdir-fid 0
quark upload-dir --dir ./photos --pdir-fid 0
```

根目录的父目录 FID 通常是 `0`。

## 常见问题

**命令叫 `quark` 还是 `quarkcli`？**

Cargo 包名是 `quarkcli`，二进制命令名是 `quark`。日常使用都输入 `quark`。

**为什么需要 Cookie？**

`quark` 需要代表你的账号访问夸克网盘。Cookie 等同于登录凭据，请按密码级别保管。

**可以在定时任务里使用吗？**

可以。建议使用 `--quiet --no-progress`，需要机器可读结果时加 `--json`。

**下载中断后怎么办？**

保留 `.part` 和 `.quark.task`，重新运行同一个下载命令并加 `--continue`。

**为什么不建议使用 `--no-verify`？**

它会跳过完整性校验。只有在你明确知道服务端没有可靠校验信息，且愿意自行确认文件完整性时才使用。

## 开发者说明

本仓库是 Rust Cargo workspace，包含两个 crate：

- `libquarkpan/`: 夸克网盘异步客户端库
- `quarkcli/`: 基于库实现的命令行工具，安装后提供 `quark`

常用开发命令：

```bash
cargo check --workspace
cargo test --workspace
cargo run -p quarkcli -- --help
```

TLS feature 需要在 `default-tls`、`native-tls`、`native-tls-vendored`、`rustls`、`rustls-no-provider` 中选择一个。

## License And Attribution

本项目采用 `GPL-3.0-only` 协议发布，详见 `LICENSE`。

本项目基于 `niuhuan/quarkpan-rs` 修改分发；上游归属和修改说明见 `NOTICE.md`。
