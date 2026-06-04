# quark-cli

`quark-cli` 是面向服务器、NAS 和自动化场景的夸克网盘命令行工具，安装后的命令是 `quark`。核心库仍然是 `libquarkpan`，Cargo package 是 `quarkcli`。

## 安装

```bash
cargo install quarkcli
cargo run -p quarkcli --bin quark -- --help
cargo install --path quarkcli --force
```

默认 TLS backend 是 `default-tls`。也可以选择其他 backend：

```bash
cargo install quarkcli --no-default-features --features native-tls
cargo install quarkcli --no-default-features --features rustls
```

## Cookie

```bash
quark auth set-cookie --from-stdin
quark auth show-source
```

也可以使用 `--cookie`、`--cookie-file` 或 `QUARK_COOKIE`。已有 `quarkpan` 配置会被读取并在 `auth show-source` 中标记为 legacy；新的 `auth set-cookie` 写入 `quarkcli` 配置目录。

## 常用命令

路径优先命令是推荐入口：

```bash
quark ls /
quark get /tvtemp/01.mp4 ./01.mp4
quark put ./backup.tar.gz /backup/
quark mkdir /backup/new
quark mv /backup/old.bin new.bin
quark rm /backup/old.bin --yes
quark stat /tvtemp/01.mp4
quark shell
```

FID 低层命令仍保留：

```bash
quark download --fid <file_fid> --output ./file.bin
quark download-dir --pdir-fid <folder_fid> --output ./backup
quark upload --file ./file.bin --pdir-fid 0
quark upload-dir --dir ./photos --pdir-fid 0
```

## 下载可靠性

下载请求使用原始字节流和 `Accept-Encoding: identity`。单文件下载先写入 `file.part`，验证通过后再原子重命名为最终文件；`file.quark.task` 用于恢复元数据。校验失败默认是错误并保留 `.part` 和 `.quark.task`，`--no-verify` 可以跳过校验但不推荐。

大文件可能被限速，HTTP 流也可能周期性断开。`quark` 会使用 Range 请求恢复：

```bash
quark get /tvtemp/01.mp4 ./01.mp4 --continue --retry auto
quark get /tvtemp/01.mp4 ./01.mp4 --retry infinite --retry-delay 2 --retry-max-delay 60
quark get /tvtemp/01.mp4 ./01.mp4 --retry 300 --retry-backoff fixed
```

正常模式会在进度中显示 `reconnects:N`，不会刷屏打印每次断线原因；`--debug` 会把原始 reconnect 错误打印到 stderr。

Shell `get` 支持同样的下载选项：

```text
get /tvtemp/01.mp4 ./01.mp4 -c --retry auto --retry-delay 2 --retry-max-delay 60 --retry-backoff exponential --no-verify
```

## 自动化输出

表格和 JSON 输出写到 stdout，进度和 debug 信息写到 stderr。

```bash
quark ls / --json
quark --format json stat /tvtemp/01.mp4
quark get /path/file ./file --quiet --no-progress
```

下载探测命令用于诊断 URL、md5 和 Range 行为：

```bash
quark probe download --fid <file_fid>
quark --json probe download --fid <file_fid>
quark --debug probe download --fid <file_fid>
```

默认不会打印完整下载 URL；`--debug` 会输出并标记为 sensitive。

## License And Attribution

`GPL-3.0-only`

This package is part of `quark-cli`, a modified distribution based on `niuhuan/quarkpan-rs`. See the root `NOTICE.md` and `LICENSE`.
