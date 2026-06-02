# quarkpan

`quarkpan` 是基于 `libquarkpan` 的夸克网盘命令行工具。它提供两种使用方式：

- `quarkpan shell`：推荐给日常用户，进入交互式 shell 后用 `ls`、`cd`、`get`、`put` 操作网盘。
- 普通 CLI 命令：推荐给脚本或高级用户，例如 `download --fid ...`、`upload --pdir-fid ...`。

## 安装

### 使用 Cargo 安装

如果已经安装 Rust/Cargo，可以直接安装：

```bash
cargo install quarkpan
```

如果是在本仓库源码目录里测试本地版本：

```bash
cargo run --bin quarkpan -- shell
```

或者把本地源码版本安装成系统命令：

```bash
cargo install --path quarkpan --force
```

### 预编译可执行文件

理论上可以打包成单个可执行文件，用户下载后即可运行，不必先安装 Cargo。发布时可以为 Linux/macOS/Windows 分别提供 release asset，例如：

```text
quarkpan-linux-x86_64
quarkpan-macos-aarch64
quarkpan-windows-x86_64.exe
```

如果项目 release 页面还没有这些文件，就需要先用 Cargo 安装，或从源码自行构建：

```bash
cargo build --release -p quarkpan
./target/release/quarkpan --help
```

## 第一次使用：设置 Cookie

`quarkpan` 需要夸克网盘登录后的 Cookie 才能访问你的网盘。Cookie 是浏览器或官方客户端登录后生成的身份凭证，应保持私密，不要提交到 Git。

推荐把 Cookie 持久化到系统配置目录：

```bash
quarkpan auth set-cookie --from-stdin
```

运行后粘贴完整 Cookie，再按回车。Cookie 格式类似：

```text
k1=v1; k2=v2; k3=v3
```

查看当前 Cookie 来源：

```bash
quarkpan auth show-source
```

也可以使用其他方式提供 Cookie：

```bash
quarkpan --cookie 'k1=v1; k2=v2' list
quarkpan --cookie-file ./cookie.txt list
QUARK_COOKIE='k1=v1; k2=v2' quarkpan list
```

## 推荐用法：交互式 Shell

进入 shell：

```bash
quarkpan shell
```

常用命令：

```text
quarkpan:/> ls
quarkpan:/> cd "来自：分享"
quarkpan:/来自：分享> pwd
quarkpan:/来自：分享> dir
quarkpan:/来自：分享> get "0531小龙女卷1" ./0531-1
quarkpan:/来自：分享> put ./local.mp4
quarkpan:/来自：分享> exit
```

`ls` 会显示文件名和 FID。路径和 32 位 FID 都可以直接使用：

```text
quarkpan:/> ls 9142a9e0d2ba435d99a98b7acc773e7a
quarkpan:/> cd 9142a9e0d2ba435d99a98b7acc773e7a
quarkpan:/> get e74acfa557aa461d9356ba8e38facdf6 ./72.mp4
```

Shell 命令速查：

- `ls` / `dir [path-or-fid]`：列出目录。
- `cd <path-or-fid>`：切换远端目录。
- `pwd`：显示当前远端目录。
- `get <path-or-fid> [local] [-c] [-o]`：下载文件或目录。
- `put <local> [remote-dir-or-fid] [-c] [-o]`：上传文件或目录。
- `mkdir <path>`：创建目录。
- `rm <path-or-fid>`：删除文件或目录，会先确认。
- `mv <path-or-fid> <new-name>`：同目录重命名。
- `help`：显示帮助。
- `exit` / `quit`：退出。

`-c` 表示继续中断任务，`-o` 表示覆盖或合并。默认遇到同名本地或远端目标时不会自动覆盖。

## 普通 CLI 命令

普通命令适合脚本自动化，参数更接近底层 API。

列出目录：

```bash
quarkpan list
quarkpan list --pdir-fid <folder_fid>
```

下载文件：

```bash
quarkpan download --fid <file_fid> --output ./file.bin
quarkpan download --fid <file_fid> --output ./file.bin -c
```

下载目录：

```bash
quarkpan download-dir --pdir-fid <folder_fid> --output ./backup
quarkpan download-dir --pdir-fid <folder_fid> --output ./backup -c -o
```

上传文件或目录：

```bash
quarkpan upload --file ./file.bin --pdir-fid 0
quarkpan upload-dir --dir ./photos --pdir-fid 0
```

创建、重命名、删除：

```bash
quarkpan folder create --pdir-fid 0 --file-name 我的文档
quarkpan rename --fid <fid> --file-name 新名字
quarkpan delete --fid <fid1> --fid <fid2>
```

## 传输恢复与校验

- 下载和上传中断后会保留 `.quark.task` 任务文件。
- 后续使用 `-c` 可以继续中断任务。
- 目录任务使用与目录同级同名的任务文件，例如 `photos.quark.task`。
- 成功完成后任务文件会自动删除。
- `Ctrl+C` 会取消当前传输，但不会删除任务文件。
- 下载完成后如果服务端返回的 md5 与本地不一致，会提示 warning 并保留文件。
- 进度条只在交互式终端显示，管道、重定向、定时任务中默认不显示。

## TLS Features

默认使用 `default-tls`。也可以选择其他 TLS backend：

```bash
cargo install quarkpan --no-default-features --features native-tls
cargo install quarkpan --no-default-features --features rustls
```

同一时间只能启用一个 TLS backend feature。

## License

`GPL-3.0-only`
