# quark-cli

`quark-cli` 是一个围绕夸克网盘接口的 Rust workspace，当前包含一个核心库和一个命令行工具。安装后的 CLI 命令是 `quark`：

- `libquarkpan`
  Rust 异步库，负责夸克网盘的目录列表、目录创建、下载、上传、分片上传和恢复相关能力。
- `quarkcli`
  基于 `libquarkpan` 的命令行程序，提供交互式 shell 和可脚本化的上传、下载、列目录、建目录等命令。

## 当前状态

当前 workspace 重点覆盖以下能力：

- 使用 Quark Cookie 构造客户端
- 平台标准配置目录中的 Cookie 持久化
- 列出目录内容
- 创建目录
- 删除一个或多个文件或目录项
- 重命名文件或目录项
- 交互式 shell：`ls`、`dir`、`cd`、`pwd`、`get`、`put`、`mkdir`、`rm`、`mv`
- 按文件 ID 下载
- 按目录 ID 批量下载目录
- 路径优先命令：`ls`、`get`、`put`、`rm`、`mkdir`、`mv`、`stat`
- 在 shell 中按路径或 FID 浏览、下载、上传、删除、重命名
- JSON 输出和 `probe download` 诊断命令
- 上传预检和快传判断
- 非快传场景下的分片上传
- 批量上传本地目录
- 传输进度监听
- 仅在交互式终端中显示彩色进度条
- Ctrl+C 取消传输
- 基于 `${filename}.part` 和 `${filename}.quark.task` 的下载恢复机制
- 基于 `目录名.quark.task` 的目录任务恢复机制

## TLS Features

`libquarkpan` 和 `quarkcli` 都支持通过 Cargo feature 选择 TLS backend，命名与 `reqwest 0.13` 对齐，默认使用 `default-tls`。

可选 feature：

- `default-tls`
- `native-tls`
- `native-tls-vendored`
- `rustls`
- `rustls-no-provider`

约束：

- 必须且只能启用一个 TLS backend feature

示例：

```bash
cargo check -p libquarkpan
cargo check -p libquarkpan --no-default-features --features native-tls
```

底层库接口仍然以文件 ID 和目录 ID 为主；CLI 已提供路径优先命令和 `quark shell`。

## Workspace 结构

### `libquarkpan`

适合以下场景：

- 你需要在自己的程序里直接接入夸克网盘
- 你希望自行管理上传流、下载流和恢复策略
- 你希望把目录同步、备份或其他业务逻辑放在自己的应用层

### `quarkcli`

适合以下场景：

- 你只需要一个可执行文件
- 你希望直接在终端通过 `quark get`、`quark put` 或 `quark shell` 完成上传、下载和目录操作
- 你希望中断后依靠 `.quark.task` 文件恢复传输

## Cookie 说明

当前客户端使用浏览器或官方客户端中登录后的 Cookie 发起请求。

常见使用方式：

- 直接通过 `--cookie 'k1=v1; k2=v2'` 传入
- 或写入文件后通过 `--cookie-file ./cookie.txt` 读取
- 或在 CLI 中使用环境变量 `QUARK_COOKIE`
- 或通过 `quark auth set-cookie` 持久化到系统配置目录

Cookie 需要是完整的 `key=value; key2=value2` 形式。
`quark auth set-cookie` 需要显式指定输入来源，例如 `--from-stdin`、`--from-nano` 或 `--from-vi`。
使用 `--from-stdin` 时，CLI 会先提示粘贴 Cookie 再回车。

## 典型操作步骤

首次使用：

```bash
quark auth set-cookie --from-stdin
quark auth show-source
```

已有 `quarkpan` 配置会被读取并标记为 legacy；新写入的 Cookie 使用 `quarkcli` 配置目录。

路径优先命令：

```bash
quark ls /
quark get /tvtemp/01.mp4 ./01.mp4
quark put ./backup.tar.gz /backup/
quark rm /tvtemp/old.mp4 --yes
quark stat /tvtemp/01.mp4 --json
```

进入交互式 shell：

```bash
quark shell
```

常用交互命令：

```text
quark:/> ls
quark:/> cd "来自：分享"
quark:/来自：分享> get "目录或文件名" ./output
quark:/来自：分享> put ./local-file
quark:/来自：分享> exit
```

单文件下载并支持恢复：

```bash
quark get /path/file.bin ./file.bin --continue --retry auto
quark download --fid <fid> --output ./file.bin --continue
```

单文件上传并支持恢复：

```bash
quark upload --file ./file.bin --pdir-fid 0
quark upload --file ./file.bin --pdir-fid 0 -c
```

目录下载并支持恢复：

```bash
quark download-dir --pdir-fid <pdir_fid> --output ./backup
quark download-dir --pdir-fid <pdir_fid> --output ./backup -c
quark download-dir --pdir-fid <pdir_fid> --output ./backup -c -o
```

目录上传并支持恢复：

```bash
quark upload-dir --dir ./photos --pdir-fid 0
quark upload-dir --dir ./photos --pdir-fid 0 -c
quark upload-dir --dir ./photos --pdir-fid 0 -c -o
```

重命名文件或目录项：

```bash
quark rename --fid <fid> --file-name 新名字
```

删除一个或多个文件或目录项：

```bash
quark delete --fid <fid1> --fid <fid2>
```

`Ctrl+C` 行为：

- 会立即取消当前传输
- 不会删除已生成的 `.part` 和 `.quark.task`
- 之后可用 `-c` 继续
- 服务端 md5 存在时，校验不匹配默认失败；`--no-verify` 可跳过但不推荐
- 完成下载会先验证 `.part`，再原子重命名为最终文件

进度条行为：

- 只在交互式 TTY 中显示
- 定时任务、管道、重定向默认不显示
- 上传和下载都会显示当前文件名
- 大文件重连会显示 `reconnects:N`；`--debug` 会显示原始重连错误

## 文档

- 根变更记录见 `CHANGELOG.md`
- 核心库说明见 `libquarkpan/README.md`
- CLI 说明见 `quarkcli/README.md`

## License And Attribution

本仓库采用 `GPL-3.0-only` 协议发布，详见根目录 `LICENSE`。

本项目基于 `niuhuan/quarkpan-rs` 修改分发；上游归属和修改说明见 `NOTICE.md`。
