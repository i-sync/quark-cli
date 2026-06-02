use std::io::Write;
use std::path::PathBuf;

use libquarkpan::{ListPage, QuarkEntry, QuarkPan, QuarkPanError};

use crate::{
    DeleteArgs, DownloadArgs, DownloadDirArgs, FolderArgs, FolderCommand, FolderCreateArgs,
    OutputFlags, RenameArgs, UploadArgs, UploadDirArgs, find_entry_by_name, handle_delete,
    handle_download, handle_download_dir, handle_folder, handle_rename, handle_upload,
    handle_upload_dir, list_all_entries, print_list_output,
};

const DEFAULT_PAGE_SIZE: u32 = 100;

struct ShellState {
    current_fid: String,
    current_path: String,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            current_fid: "0".to_string(),
            current_path: "/".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
    Help,
    Exit,
    Pwd,
    Ls {
        remote_path: Option<String>,
    },
    Cd {
        remote_path: String,
    },
    Get {
        remote_path: String,
        local_path: Option<PathBuf>,
        continue_transfer: bool,
        overwrite: bool,
    },
    Put {
        local_path: PathBuf,
        remote_dir: Option<String>,
        continue_transfer: bool,
        overwrite: bool,
    },
    Mkdir {
        remote_path: String,
    },
    Rm {
        remote_path: String,
    },
    Mv {
        remote_path: String,
        new_name: String,
    },
}

pub fn parse_shell_command(line: &str) -> Result<ShellCommand, QuarkPanError> {
    let words = shlex::split(line).ok_or_else(|| {
        QuarkPanError::invalid_argument("cannot parse command: unmatched quote or escape")
    })?;
    let Some((command, args)) = words.split_first() else {
        return Err(QuarkPanError::invalid_argument("empty command"));
    };
    match command.as_str() {
        "help" | "?" => parse_no_args("help", args, ShellCommand::Help),
        "exit" | "quit" => parse_no_args("exit", args, ShellCommand::Exit),
        "pwd" => parse_no_args("pwd", args, ShellCommand::Pwd),
        "ls" | "dir" => {
            parse_optional_path("ls", args).map(|remote_path| ShellCommand::Ls { remote_path })
        }
        "cd" => parse_one_path("cd", args).map(|remote_path| ShellCommand::Cd { remote_path }),
        "get" => parse_get(args),
        "put" => parse_put(args),
        "mkdir" => {
            parse_one_path("mkdir", args).map(|remote_path| ShellCommand::Mkdir { remote_path })
        }
        "rm" => parse_one_path("rm", args).map(|remote_path| ShellCommand::Rm { remote_path }),
        "mv" => parse_mv(args),
        _ => Err(QuarkPanError::invalid_argument(format!(
            "unknown shell command: {command}"
        ))),
    }
}

pub async fn run_shell(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = ShellState::default();
    print_shell_help();
    let stdin = std::io::stdin();
    loop {
        print!("quarkpan:{}> ", state.current_path);
        std::io::stdout().flush()?;
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!();
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_shell_command(line) {
            Ok(ShellCommand::Exit) => break,
            Ok(command) => {
                if let Err(err) = execute_shell_command(flags, quark_pan, &mut state, command).await
                {
                    eprintln!("error: {err}");
                }
            }
            Err(err) => eprintln!("error: {err}"),
        }
    }
    Ok(())
}

async fn execute_shell_command(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    state: &mut ShellState,
    command: ShellCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ShellCommand::Help => print_shell_help(),
        ShellCommand::Exit => {}
        ShellCommand::Pwd => println!("{}", state.current_path),
        ShellCommand::Ls { remote_path } => {
            let (fid, _) = resolve_dir_path(quark_pan, state, remote_path.as_deref()).await?;
            let entries = list_all_entries(quark_pan, &fid, DEFAULT_PAGE_SIZE).await?;
            let page = ListPage {
                entries,
                page: 1,
                size: DEFAULT_PAGE_SIZE,
                total: 0,
            };
            print_list_output(flags, &page, false, false)?;
        }
        ShellCommand::Cd { remote_path } => {
            let (fid, path) = resolve_dir_path(quark_pan, state, Some(&remote_path)).await?;
            state.current_fid = fid;
            state.current_path = path;
        }
        ShellCommand::Get {
            remote_path,
            local_path,
            continue_transfer,
            overwrite,
        } => {
            let (entry, path) = resolve_entry_path(quark_pan, state, &remote_path).await?;
            let output = local_path.unwrap_or_else(|| PathBuf::from(&entry.file_name));
            if entry.dir {
                handle_download_dir(
                    flags,
                    quark_pan,
                    DownloadDirArgs {
                        pdir_fid: entry.fid,
                        output,
                        continue_download: continue_transfer,
                        overwrite,
                        retry: 5,
                        retry_delay: 2,
                    },
                )
                .await?;
            } else {
                let output = if output.is_dir() {
                    output.join(&entry.file_name)
                } else {
                    output
                };
                handle_download(
                    flags,
                    quark_pan,
                    DownloadArgs {
                        fid: entry.fid,
                        output: Some(output),
                        stdout: false,
                        overwrite,
                        continue_download: continue_transfer,
                        retry: 5,
                        retry_delay: 2,
                    },
                )
                .await?;
            }
            if !flags.quiet {
                eprintln!("get completed: {path}");
            }
        }
        ShellCommand::Put {
            local_path,
            remote_dir,
            continue_transfer,
            overwrite,
        } => {
            let (pdir_fid, _) = resolve_dir_path(quark_pan, state, remote_dir.as_deref()).await?;
            if local_path.is_dir() {
                handle_upload_dir(
                    flags,
                    quark_pan,
                    UploadDirArgs {
                        pdir_fid,
                        dir: local_path,
                        file_name: None,
                        r#continue: continue_transfer,
                        overwrite,
                    },
                )
                .await?;
            } else {
                handle_upload(
                    flags,
                    quark_pan,
                    UploadArgs {
                        pdir_fid,
                        file: local_path,
                        file_name: None,
                        r#continue: continue_transfer,
                        overwrite,
                    },
                )
                .await?;
            }
        }
        ShellCommand::Mkdir { remote_path } => {
            let (parent_fid, file_name) =
                resolve_parent_and_name(quark_pan, state, &remote_path).await?;
            handle_folder(
                flags,
                quark_pan,
                FolderArgs {
                    command: FolderCommand::Create(FolderCreateArgs {
                        pdir_fid: parent_fid,
                        file_name,
                    }),
                },
            )
            .await?;
        }
        ShellCommand::Rm { remote_path } => {
            let (entry, path) = resolve_entry_path(quark_pan, state, &remote_path).await?;
            if confirm(&format!("delete {path}? [y/N] "))? {
                handle_delete(
                    flags,
                    quark_pan,
                    DeleteArgs {
                        fid: vec![entry.fid],
                    },
                )
                .await?;
            }
        }
        ShellCommand::Mv {
            remote_path,
            new_name,
        } => {
            if new_name.contains('/') {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "mv only supports renaming within the same remote directory",
                )));
            }
            let (entry, _) = resolve_entry_path(quark_pan, state, &remote_path).await?;
            handle_rename(
                flags,
                quark_pan,
                RenameArgs {
                    fid: entry.fid,
                    file_name: new_name,
                },
            )
            .await?;
        }
    }
    Ok(())
}

async fn resolve_dir_path(
    quark_pan: &QuarkPan,
    state: &ShellState,
    path: Option<&str>,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let Some(path) = path.filter(|value| !value.trim().is_empty()) else {
        return Ok((state.current_fid.clone(), state.current_path.clone()));
    };
    if path == "." {
        return Ok((state.current_fid.clone(), state.current_path.clone()));
    }
    if path == "/" {
        return Ok(("0".to_string(), "/".to_string()));
    }
    if is_quark_fid(path) {
        return Ok((path.to_string(), fid_display_path(path)));
    }
    let absolute = absolute_remote_path(&state.current_path, path);
    let mut current_fid = "0".to_string();
    if absolute == "/" {
        return Ok((current_fid, absolute));
    }
    for component in absolute.trim_start_matches('/').split('/') {
        let entry = find_entry_by_name(quark_pan, &current_fid, component)
            .await?
            .ok_or_else(|| {
                QuarkPanError::invalid_argument(format!("remote path not found: {absolute}"))
            })?;
        if !entry.dir {
            return Err(Box::new(QuarkPanError::invalid_argument(format!(
                "remote path is not a directory: {absolute}"
            ))));
        }
        current_fid = entry.fid;
    }
    Ok((current_fid, absolute))
}

async fn resolve_entry_path(
    quark_pan: &QuarkPan,
    state: &ShellState,
    path: &str,
) -> Result<(QuarkEntry, String), Box<dyn std::error::Error>> {
    let absolute = absolute_remote_path(&state.current_path, path);
    if is_quark_fid(path) {
        return resolve_fid_entry(quark_pan, path).await;
    }
    if absolute == "/" {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "remote root is not a file entry",
        )));
    }
    let (parent_path, name) = split_remote_parent_name(&absolute)?;
    let (parent_fid, _) = resolve_dir_path(quark_pan, state, Some(&parent_path)).await?;
    let entry = find_entry_by_name(quark_pan, &parent_fid, &name)
        .await?
        .ok_or_else(|| {
            QuarkPanError::invalid_argument(format!("remote path not found: {absolute}"))
        })?;
    Ok((entry, absolute))
}

async fn resolve_parent_and_name(
    quark_pan: &QuarkPan,
    state: &ShellState,
    path: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let absolute = absolute_remote_path(&state.current_path, path);
    let (parent_path, name) = split_remote_parent_name(&absolute)?;
    let (parent_fid, _) = resolve_dir_path(quark_pan, state, Some(&parent_path)).await?;
    Ok((parent_fid, name))
}

fn split_remote_parent_name(path: &str) -> Result<(String, String), QuarkPanError> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return Err(QuarkPanError::invalid_argument(
            "remote path must include a name",
        ));
    }
    let Some((parent, name)) = trimmed.rsplit_once('/') else {
        return Ok(("/".to_string(), trimmed.to_string()));
    };
    if name.is_empty() {
        return Err(QuarkPanError::invalid_argument(
            "remote path must include a name",
        ));
    }
    let parent = if parent.is_empty() { "/" } else { parent };
    Ok((parent.to_string(), name.to_string()))
}

fn absolute_remote_path(current_path: &str, path: &str) -> String {
    if path.starts_with('/') {
        normalize_remote_path(path)
    } else {
        let base = if current_path == "/" {
            format!("/{path}")
        } else {
            format!("{current_path}/{path}")
        };
        normalize_remote_path(&base)
    }
}

async fn resolve_fid_entry(
    quark_pan: &QuarkPan,
    fid: &str,
) -> Result<(QuarkEntry, String), Box<dyn std::error::Error>> {
    if quark_pan
        .list()
        .pdir_fid(fid.to_string())
        .page(1)
        .size(1)
        .prepare()?
        .request()
        .await
        .is_ok()
    {
        return Ok((synthetic_fid_entry(fid, true), fid_display_path(fid)));
    }
    quark_pan
        .download()
        .fid(fid.to_string())
        .prepare()?
        .info()
        .await?;
    Ok((synthetic_fid_entry(fid, false), fid_display_path(fid)))
}

fn synthetic_fid_entry(fid: &str, dir: bool) -> QuarkEntry {
    QuarkEntry {
        fid: fid.to_string(),
        file_name: fid_file_name(fid),
        pdir_fid: String::new(),
        size: 0,
        format_type: String::new(),
        status: 0,
        created_at: 0,
        updated_at: 0,
        dir,
        file: !dir,
    }
}

fn fid_file_name(fid: &str) -> String {
    format!("@fid-{}", &fid[..8])
}

fn fid_display_path(fid: &str) -> String {
    format!("/@fid:{}", &fid[..8])
}

fn is_quark_fid(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn confirm(prompt: &str) -> Result<bool, Box<dyn std::error::Error>> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("y") || answer.trim().eq_ignore_ascii_case("yes"))
}

fn print_shell_help() {
    println!(
        "Interactive commands: ls|dir [path-or-fid], cd <path-or-fid>, pwd, get <path-or-fid> [local] [-c] [-o], put <local> [remote_dir-or-fid] [-c] [-o], mkdir <path>, rm <path-or-fid>, mv <path-or-fid> <new_name>, help, exit"
    );
}

fn parse_no_args(
    name: &str,
    args: &[String],
    command: ShellCommand,
) -> Result<ShellCommand, QuarkPanError> {
    if args.is_empty() {
        Ok(command)
    } else {
        Err(QuarkPanError::invalid_argument(format!("usage: {name}")))
    }
}

fn parse_optional_path(name: &str, args: &[String]) -> Result<Option<String>, QuarkPanError> {
    if args.len() <= 1 {
        Ok(args.first().cloned())
    } else {
        Err(QuarkPanError::invalid_argument(format!(
            "usage: {name} [remote_path]"
        )))
    }
}

fn parse_one_path(name: &str, args: &[String]) -> Result<String, QuarkPanError> {
    if args.len() == 1 {
        Ok(args[0].clone())
    } else {
        Err(QuarkPanError::invalid_argument(format!(
            "usage: {name} <remote_path>"
        )))
    }
}

fn parse_get(args: &[String]) -> Result<ShellCommand, QuarkPanError> {
    let mut positional = Vec::new();
    let mut continue_transfer = false;
    let mut overwrite = false;
    for arg in args {
        match arg.as_str() {
            "-c" | "--continue" => continue_transfer = true,
            "-o" | "--overwrite" => overwrite = true,
            _ if arg.starts_with('-') => {
                return Err(QuarkPanError::invalid_argument(format!(
                    "unknown get option: {arg}"
                )));
            }
            _ => positional.push(arg.clone()),
        }
    }
    if positional.is_empty() || positional.len() > 2 {
        return Err(QuarkPanError::invalid_argument(
            "usage: get <remote_path> [local_path] [-c] [-o]",
        ));
    }
    Ok(ShellCommand::Get {
        remote_path: positional[0].clone(),
        local_path: positional.get(1).map(PathBuf::from),
        continue_transfer,
        overwrite,
    })
}

fn parse_put(args: &[String]) -> Result<ShellCommand, QuarkPanError> {
    let mut positional = Vec::new();
    let mut continue_transfer = false;
    let mut overwrite = false;
    for arg in args {
        match arg.as_str() {
            "-c" | "--continue" => continue_transfer = true,
            "-o" | "--overwrite" => overwrite = true,
            _ if arg.starts_with('-') => {
                return Err(QuarkPanError::invalid_argument(format!(
                    "unknown put option: {arg}"
                )));
            }
            _ => positional.push(arg.clone()),
        }
    }
    if positional.is_empty() || positional.len() > 2 {
        return Err(QuarkPanError::invalid_argument(
            "usage: put <local_path> [remote_dir] [-c] [-o]",
        ));
    }
    Ok(ShellCommand::Put {
        local_path: PathBuf::from(&positional[0]),
        remote_dir: positional.get(1).cloned(),
        continue_transfer,
        overwrite,
    })
}

fn parse_mv(args: &[String]) -> Result<ShellCommand, QuarkPanError> {
    if args.len() != 2 {
        return Err(QuarkPanError::invalid_argument(
            "usage: mv <remote_path> <new_name>",
        ));
    }
    Ok(ShellCommand::Mv {
        remote_path: args[0].clone(),
        new_name: args[1].clone(),
    })
}

pub fn normalize_remote_path(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_get_with_quoted_remote_path_and_continue_flag() {
        let command = parse_shell_command(r#"get "来自：分享/0531 小龙女" ./0531 -c"#).unwrap();

        assert_eq!(
            command,
            ShellCommand::Get {
                remote_path: "来自：分享/0531 小龙女".to_string(),
                local_path: Some("./0531".into()),
                continue_transfer: true,
                overwrite: false,
            }
        );
    }

    #[test]
    fn normalizes_remote_path_with_parent_components() {
        assert_eq!(
            normalize_remote_path("/来自：分享/0531/../tvtemp"),
            "/来自：分享/tvtemp"
        );
        assert_eq!(normalize_remote_path("../tvtemp"), "/tvtemp");
        assert_eq!(normalize_remote_path("./"), "/");
    }

    #[test]
    fn parses_basic_navigation_and_mutation_commands() {
        assert_eq!(
            parse_shell_command(r#"ls "来自：分享""#).unwrap(),
            ShellCommand::Ls {
                remote_path: Some("来自：分享".to_string())
            }
        );
        assert_eq!(
            parse_shell_command("cd ..").unwrap(),
            ShellCommand::Cd {
                remote_path: "..".to_string()
            }
        );
        assert_eq!(parse_shell_command("pwd").unwrap(), ShellCommand::Pwd);
        assert_eq!(
            parse_shell_command("put ./local.mp4 /tvtemp -o").unwrap(),
            ShellCommand::Put {
                local_path: "./local.mp4".into(),
                remote_dir: Some("/tvtemp".to_string()),
                continue_transfer: false,
                overwrite: true,
            }
        );
        assert_eq!(
            parse_shell_command(r#"mkdir "新目录""#).unwrap(),
            ShellCommand::Mkdir {
                remote_path: "新目录".to_string()
            }
        );
        assert_eq!(
            parse_shell_command(r#"rm "旧文件.mp4""#).unwrap(),
            ShellCommand::Rm {
                remote_path: "旧文件.mp4".to_string()
            }
        );
        assert_eq!(
            parse_shell_command(r#"mv "旧名字.mp4" "新名字.mp4""#).unwrap(),
            ShellCommand::Mv {
                remote_path: "旧名字.mp4".to_string(),
                new_name: "新名字.mp4".to_string(),
            }
        );
        assert_eq!(parse_shell_command("quit").unwrap(), ShellCommand::Exit);
    }

    #[test]
    fn parses_dir_as_ls_alias() {
        assert_eq!(
            parse_shell_command("dir").unwrap(),
            ShellCommand::Ls { remote_path: None }
        );
        assert_eq!(
            parse_shell_command("dir 9142a9e0d2ba435d99a98b7acc773e7a").unwrap(),
            ShellCommand::Ls {
                remote_path: Some("9142a9e0d2ba435d99a98b7acc773e7a".to_string())
            }
        );
    }

    #[test]
    fn recognizes_quark_fids() {
        assert!(is_quark_fid("9142a9e0d2ba435d99a98b7acc773e7a"));
        assert!(is_quark_fid("E74ACFA557AA461D9356BA8E38FACDF6"));
        assert!(!is_quark_fid("0531小龙女卷1"));
        assert!(!is_quark_fid("9142a9e0d2ba435d99a98b7acc773e7"));
    }
}
