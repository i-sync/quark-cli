mod shell;

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose};
use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::ProjectDirs;
use futures_util::Stream;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use libquarkpan::{
    ListPage, ProgressStream, QuarkEntry, QuarkPan, QuarkPanError, RetryClass, TransferControl,
    TransferProgress, UploadPrepareResult, UploadResume, UploadResumeState,
};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use sha1::Digest;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

use crate::shell::ShellState;

#[derive(Parser, Debug)]
#[command(name = "quark", version, about = "Command-line client for Quark Drive")]
struct Cli {
    #[arg(long, env = "QUARK_COOKIE")]
    cookie: Option<String>,
    #[arg(long)]
    cookie_file: Option<PathBuf>,
    #[arg(long)]
    config_file: Option<PathBuf>,
    #[arg(long)]
    api_base_url: Option<String>,
    #[arg(long, global = true)]
    quiet: bool,
    #[arg(long, global = true)]
    no_progress: bool,
    #[arg(long, global = true)]
    debug: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Table, global = true)]
    format: OutputFormat,
    #[arg(long, conflicts_with = "format", global = true)]
    json: bool,
    #[arg(long, value_enum, global = true)]
    color: Option<ColorMode>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
enum VerifyMode {
    Auto,
    Always,
    Never,
}

impl FromStr for VerifyMode {
    type Err = QuarkPanError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => Err(QuarkPanError::invalid_argument(
                "verify must be auto, always, or never",
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RetryMode {
    Auto,
    Infinite,
    Count(u32),
}

impl FromStr for RetryMode {
    type Err = QuarkPanError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "infinite" => Ok(Self::Infinite),
            _ => value.parse::<u32>().map(Self::Count).map_err(|_| {
                QuarkPanError::invalid_argument("retry must be auto, infinite, or a number")
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum RetryBackoff {
    Exponential,
    Fixed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
}

impl FromStr for RetryBackoff {
    type Err = QuarkPanError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "exponential" => Ok(Self::Exponential),
            "fixed" => Ok(Self::Fixed),
            _ => Err(QuarkPanError::invalid_argument(
                "retry backoff must be exponential or fixed",
            )),
        }
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    Auth(AuthArgs),
    Delete(DeleteArgs),
    Get(GetArgs),
    Ls(LsArgs),
    List(ListArgs),
    Download(DownloadArgs),
    DownloadDir(DownloadDirArgs),
    Folder(FolderArgs),
    Mkdir(MkdirArgs),
    Mv(MvArgs),
    Probe(ProbeArgs),
    Put(PutArgs),
    Rename(RenameArgs),
    Rm(RmArgs),
    Shell,
    Stat(StatArgs),
    Upload(UploadArgs),
    UploadDir(UploadDirArgs),
}

#[derive(Args, Debug)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Subcommand, Debug)]
enum AuthCommand {
    SetCookie(SetCookieArgs),
    ImportCookie(ImportCookieArgs),
    ClearCookie,
    ShowSource,
}

#[derive(Args, Debug)]
struct SetCookieArgs {
    #[arg(long)]
    cookie: Option<String>,
    #[arg(long, conflicts_with_all = ["cookie", "from_nano", "from_vi"])]
    from_stdin: bool,
    #[arg(long, conflicts_with_all = ["cookie", "from_stdin", "from_vi"])]
    from_nano: bool,
    #[arg(long, conflicts_with_all = ["cookie", "from_stdin", "from_nano"])]
    from_vi: bool,
}

#[derive(Args, Debug)]
struct ImportCookieArgs {
    #[arg(long)]
    from_file: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct DeleteArgs {
    #[arg(long, required = true, num_args = 1..)]
    fid: Vec<String>,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct GetArgs {
    remote_path_or_fid: String,
    local_path: Option<PathBuf>,
    #[arg(long, short = 'o')]
    overwrite: bool,
    #[arg(long = "continue", short = 'c')]
    continue_download: bool,
    #[arg(long, default_value = "auto")]
    retry: RetryMode,
    #[arg(long, default_value_t = 2)]
    retry_delay: u64,
    #[arg(long, default_value_t = 60)]
    retry_max_delay: u64,
    #[arg(long, value_enum, default_value_t = RetryBackoff::Exponential)]
    retry_backoff: RetryBackoff,
    #[arg(long, value_enum, default_value_t = VerifyMode::Auto)]
    verify: VerifyMode,
    #[arg(long, conflicts_with = "verify")]
    no_verify: bool,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct PutArgs {
    local_path: PathBuf,
    remote_dir_or_fid: Option<String>,
    #[arg(long, short = 'c')]
    r#continue: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct LsArgs {
    remote_path_or_fid: Option<String>,
    #[arg(long)]
    long: bool,
    #[arg(long)]
    raw_time: bool,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct RmArgs {
    remote_path_or_fid: String,
    #[arg(long)]
    yes: bool,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct MkdirArgs {
    remote_path: String,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct MvArgs {
    remote_path_or_fid: String,
    new_name_or_path: String,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct StatArgs {
    remote_path_or_fid: String,
}

#[derive(Args, Debug)]
pub(crate) struct ProbeArgs {
    #[command(subcommand)]
    command: ProbeCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ProbeCommand {
    Download(ProbeDownloadArgs),
}

#[derive(Args, Debug, Clone)]
pub(crate) struct ProbeDownloadArgs {
    #[arg(long)]
    fid: String,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct ListArgs {
    #[arg(long, default_value = "0")]
    pdir_fid: String,
    #[arg(long, default_value_t = 1)]
    page: u32,
    #[arg(long, default_value_t = 100)]
    size: u32,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    more: bool,
    #[arg(long)]
    long: bool,
    #[arg(long)]
    raw_time: bool,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct DownloadArgs {
    #[arg(long)]
    fid: String,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    stdout: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
    #[arg(long = "continue", short = 'c')]
    continue_download: bool,
    #[arg(long, default_value = "auto")]
    retry: RetryMode,
    #[arg(long, default_value_t = 2)]
    retry_delay: u64,
    #[arg(long, default_value_t = 60)]
    retry_max_delay: u64,
    #[arg(long, value_enum, default_value_t = RetryBackoff::Exponential)]
    retry_backoff: RetryBackoff,
    #[arg(long, value_enum, default_value_t = VerifyMode::Auto)]
    verify: VerifyMode,
    #[arg(long, conflicts_with = "verify")]
    no_verify: bool,
}

impl DownloadArgs {
    fn verify_mode(&self) -> VerifyMode {
        if self.no_verify {
            VerifyMode::Never
        } else {
            self.verify
        }
    }
}

#[derive(Args, Debug, Clone)]
pub(crate) struct DownloadDirArgs {
    #[arg(long)]
    pdir_fid: String,
    #[arg(long)]
    output: PathBuf,
    #[arg(long = "continue", short = 'c')]
    continue_download: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
    #[arg(long, default_value = "auto")]
    retry: RetryMode,
    #[arg(long, default_value_t = 2)]
    retry_delay: u64,
    #[arg(long, default_value_t = 60)]
    retry_max_delay: u64,
    #[arg(long, value_enum, default_value_t = RetryBackoff::Exponential)]
    retry_backoff: RetryBackoff,
    #[arg(long, value_enum, default_value_t = VerifyMode::Auto)]
    verify: VerifyMode,
    #[arg(long, conflicts_with = "verify")]
    no_verify: bool,
}

impl DownloadDirArgs {
    fn from_get(pdir_fid: String, output: PathBuf, args: GetArgs) -> Self {
        Self {
            pdir_fid,
            output,
            continue_download: args.continue_download,
            overwrite: args.overwrite,
            retry: args.retry,
            retry_delay: args.retry_delay,
            retry_max_delay: args.retry_max_delay,
            retry_backoff: args.retry_backoff,
            verify: args.verify,
            no_verify: args.no_verify,
        }
    }

    fn verify_mode(&self) -> VerifyMode {
        if self.no_verify {
            VerifyMode::Never
        } else {
            self.verify
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct FolderArgs {
    #[command(subcommand)]
    command: FolderCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum FolderCommand {
    Create(FolderCreateArgs),
}

#[derive(Args, Debug)]
pub(crate) struct FolderCreateArgs {
    #[arg(long, default_value = "0")]
    pdir_fid: String,
    #[arg(long)]
    file_name: String,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct RenameArgs {
    #[arg(long)]
    fid: String,
    #[arg(long)]
    file_name: String,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct UploadArgs {
    #[arg(long, default_value = "0")]
    pdir_fid: String,
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    file_name: Option<String>,
    #[arg(long, short = 'c')]
    r#continue: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct UploadDirArgs {
    #[arg(long, default_value = "0")]
    pdir_fid: String,
    #[arg(long)]
    dir: PathBuf,
    #[arg(long)]
    file_name: Option<String>,
    #[arg(long, short = 'c')]
    r#continue: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct OutputFlags {
    quiet: bool,
    no_progress: bool,
    debug: bool,
    format: OutputFormat,
    color: bool,
    interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppConfig {
    api_base_url: Option<String>,
    color: Option<ColorMode>,
}

#[derive(Debug, Clone)]
struct AppPaths {
    config_file: PathBuf,
    cookie_file: PathBuf,
    source: AppPathSource,
    write_config_dir: PathBuf,
    write_cookie_file: PathBuf,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AppPathSource {
    Explicit,
    New,
    Legacy,
}

impl AppPathSource {
    fn stored_source(self) -> &'static str {
        match self {
            AppPathSource::Explicit | AppPathSource::New => "persisted_cookie",
            AppPathSource::Legacy => "stored-legacy",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct FolderCreateOutput {
    fid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UploadDoneOutput {
    fid: String,
    rapid_upload: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct RenameOutput {
    fid: String,
    file_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeleteOutput {
    fids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthSourceOutput {
    source: String,
    path: Option<String>,
    legacy: bool,
}

#[derive(Debug, Serialize)]
struct HashOutput {
    name: String,
    size: u64,
    md5: String,
    sha1: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProbeDownloadOutput {
    fid: String,
    download_url: String,
    md5: String,
    range: String,
    first_bytes: usize,
    sensitive_download_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatOutput {
    fid: String,
    name: String,
    dir: bool,
    size: u64,
    updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadTask {
    kind: String,
    fid: String,
    output_path: String,
    part_path: String,
    md5: Option<String>,
    size: Option<u64>,
    verify: VerifyMode,
}

impl DownloadTask {
    fn new(
        fid: String,
        output_path: &Path,
        part_path: &Path,
        md5: Option<String>,
        size: Option<u64>,
        verify: VerifyMode,
    ) -> Self {
        Self {
            kind: "download".to_string(),
            fid,
            output_path: output_path.to_string_lossy().to_string(),
            part_path: part_path.to_string_lossy().to_string(),
            md5,
            size,
            verify,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadTask {
    kind: String,
    file_path: String,
    file_name: String,
    pdir_fid: String,
    size: u64,
    md5: String,
    sha1: String,
    resume: UploadResume,
    state: UploadResumeState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DirEntryStatus {
    Pending,
    Running,
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadDirEntryTask {
    relative_path: String,
    fid: String,
    md5: Option<String>,
    status: DirEntryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadDirTask {
    kind: String,
    pdir_fid: String,
    output_dir: String,
    entries: Vec<DownloadDirEntryTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadDirEntryTask {
    relative_path: String,
    status: DirEntryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadDirTask {
    kind: String,
    source_dir: String,
    pdir_fid: String,
    target_file_name: String,
    root_fid: String,
    entries: Vec<UploadDirEntryTask>,
}

#[derive(Debug, Clone)]
struct RemoteFileItem {
    relative_path: PathBuf,
    fid: String,
}

#[derive(Debug, Clone)]
struct LocalFileItem {
    relative_path: PathBuf,
}

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("error: {err}");
            1
        }
    };
    std::process::exit(code);
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    validate_cli(&cli)?;

    let paths = app_paths(cli.config_file.clone())?;
    let config = load_config(&paths).await?;
    let flags = OutputFlags {
        quiet: cli.quiet,
        no_progress: cli.no_progress,
        debug: cli.debug,
        format: if cli.json {
            OutputFormat::Json
        } else {
            cli.format
        },
        color: resolve_color(cli.color.or(config.color).unwrap_or(ColorMode::Auto)),
        interactive: std::io::stderr().is_terminal(),
    };

    if let Commands::Auth(args) = cli.command {
        return handle_auth(flags, &paths, args).await;
    }

    let cookie = load_cookie(&cli, &paths).await?;
    let quark_pan = QuarkPan::builder()
        .cookie(cookie)
        .api_base_url(
            cli.api_base_url
                .or(config.api_base_url)
                .unwrap_or_else(|| "https://drive.quark.cn".to_string()),
        )
        .prepare()?;

    match cli.command {
        Commands::Auth(_) => unreachable!(),
        Commands::Delete(args) => handle_delete(flags, &quark_pan, args).await?,
        Commands::Get(args) => handle_get(flags, &quark_pan, args).await?,
        Commands::Ls(args) => handle_ls(flags, &quark_pan, args).await?,
        Commands::List(args) => handle_list(flags, &quark_pan, args).await?,
        Commands::Download(args) => handle_download(flags, &quark_pan, args).await?,
        Commands::DownloadDir(args) => handle_download_dir(flags, &quark_pan, args).await?,
        Commands::Folder(args) => handle_folder(flags, &quark_pan, args).await?,
        Commands::Mkdir(args) => handle_mkdir(flags, &quark_pan, args).await?,
        Commands::Mv(args) => handle_mv(flags, &quark_pan, args).await?,
        Commands::Probe(args) => handle_probe(flags, &quark_pan, args).await?,
        Commands::Put(args) => handle_put(flags, &quark_pan, args).await?,
        Commands::Rename(args) => handle_rename(flags, &quark_pan, args).await?,
        Commands::Rm(args) => handle_rm(flags, &quark_pan, args).await?,
        Commands::Shell => shell::run_shell(flags, &quark_pan).await?,
        Commands::Stat(args) => handle_stat(flags, &quark_pan, args).await?,
        Commands::Upload(args) => handle_upload(flags, &quark_pan, args).await?,
        Commands::UploadDir(args) => handle_upload_dir(flags, &quark_pan, args).await?,
    }
    Ok(())
}

fn validate_cli(cli: &Cli) -> Result<(), QuarkPanError> {
    if cli.cookie.is_some() && cli.cookie_file.is_some() {
        return Err(QuarkPanError::invalid_argument(
            "--cookie and --cookie-file cannot be used together",
        ));
    }
    Ok(())
}

fn app_paths(config_file: Option<PathBuf>) -> Result<AppPaths, QuarkPanError> {
    let Some(new_dirs) = ProjectDirs::from("", "", new_app_name()) else {
        return Err(QuarkPanError::invalid_argument(
            "cannot resolve platform config directory",
        ));
    };
    let Some(legacy_dirs) = ProjectDirs::from("", "", legacy_app_name()) else {
        return Err(QuarkPanError::invalid_argument(
            "cannot resolve platform config directory",
        ));
    };
    select_app_paths(
        config_file,
        new_dirs.config_dir().to_path_buf(),
        legacy_dirs.config_dir().to_path_buf(),
    )
}

fn new_app_name() -> &'static str {
    "quarkcli"
}

fn legacy_app_name() -> &'static str {
    "quarkpan"
}

fn select_app_paths(
    config_file: Option<PathBuf>,
    new_config_dir: PathBuf,
    legacy_config_dir: PathBuf,
) -> Result<AppPaths, QuarkPanError> {
    let new_exists = config_or_cookie_exists(&new_config_dir);
    let legacy_exists = config_or_cookie_exists(&legacy_config_dir);
    select_existing_app_paths(
        config_file,
        new_config_dir,
        new_exists,
        legacy_config_dir,
        legacy_exists,
    )
}

fn select_existing_app_paths(
    config_file: Option<PathBuf>,
    new_config_dir: PathBuf,
    new_exists: bool,
    legacy_config_dir: PathBuf,
    legacy_exists: bool,
) -> Result<AppPaths, QuarkPanError> {
    if let Some(config_file) = config_file {
        let config_dir = config_file
            .parent()
            .ok_or_else(|| QuarkPanError::invalid_argument("invalid --config-file path"))?
            .to_path_buf();
        return Ok(AppPaths {
            config_file: config_file.clone(),
            cookie_file: config_dir.join("cookie.txt"),
            source: AppPathSource::Explicit,
            write_config_dir: config_dir.clone(),
            write_cookie_file: config_dir.join("cookie.txt"),
        });
    }

    let (config_dir, source) = if new_exists || !legacy_exists {
        (new_config_dir.clone(), AppPathSource::New)
    } else {
        (legacy_config_dir, AppPathSource::Legacy)
    };
    Ok(AppPaths {
        config_file: config_dir.join("config.toml"),
        cookie_file: config_dir.join("cookie.txt"),
        source,
        write_cookie_file: new_config_dir.join("cookie.txt"),
        write_config_dir: new_config_dir,
    })
}

fn config_or_cookie_exists(config_dir: &Path) -> bool {
    config_dir.join("config.toml").exists() || config_dir.join("cookie.txt").exists()
}

async fn ensure_config_dir(paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    tokio::fs::create_dir_all(&paths.write_config_dir).await?;
    Ok(())
}

async fn load_config(paths: &AppPaths) -> Result<AppConfig, Box<dyn std::error::Error>> {
    if !paths.config_file.exists() {
        return Ok(AppConfig::default());
    }
    let text = tokio::fs::read_to_string(&paths.config_file).await?;
    Ok(toml::from_str(&text)?)
}

async fn load_cookie(cli: &Cli, paths: &AppPaths) -> Result<String, QuarkPanError> {
    if let Some(cookie) = &cli.cookie {
        return Ok(cookie.clone());
    }
    if let Some(cookie_file) = &cli.cookie_file {
        let cookie = tokio::fs::read_to_string(cookie_file).await?;
        return Ok(cookie.trim().to_string());
    }
    if let Ok(cookie) = std::env::var("QUARK_COOKIE") {
        if !cookie.trim().is_empty() {
            return Ok(cookie);
        }
    }
    if paths.cookie_file.exists() {
        let cookie = tokio::fs::read_to_string(&paths.cookie_file).await?;
        return Ok(cookie.trim().to_string());
    }
    Err(QuarkPanError::missing_field("cookie"))
}

async fn handle_auth(
    flags: OutputFlags,
    paths: &AppPaths,
    args: AuthArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        AuthCommand::SetCookie(args) => {
            ensure_config_dir(paths).await?;
            let cookie = if let Some(cookie) = args.cookie {
                cookie
            } else if args.from_stdin {
                read_cookie_from_stdin()?
            } else if args.from_nano {
                edit_cookie_with("nano")?
            } else if args.from_vi {
                edit_cookie_with("vi")?
            } else {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "one of --cookie, --from-stdin, --from-nano, or --from-vi is required",
                )));
            };
            tokio::fs::write(&paths.write_cookie_file, format!("{}\n", cookie.trim())).await?;
            print_output(
                flags,
                &AuthSourceOutput {
                    source: "persisted_cookie".to_string(),
                    path: Some(paths.write_cookie_file.display().to_string()),
                    legacy: false,
                },
            )?;
        }
        AuthCommand::ImportCookie(args) => {
            ensure_config_dir(paths).await?;
            let cookie = tokio::fs::read_to_string(args.from_file).await?;
            tokio::fs::write(&paths.write_cookie_file, format!("{}\n", cookie.trim())).await?;
            print_output(
                flags,
                &AuthSourceOutput {
                    source: "persisted_cookie".to_string(),
                    path: Some(paths.write_cookie_file.display().to_string()),
                    legacy: false,
                },
            )?;
        }
        AuthCommand::ClearCookie => {
            remove_if_exists(&paths.write_cookie_file).await?;
            print_output(
                flags,
                &AuthSourceOutput {
                    source: "cleared".to_string(),
                    path: Some(paths.write_cookie_file.display().to_string()),
                    legacy: false,
                },
            )?;
        }
        AuthCommand::ShowSource => {
            let output = if std::env::var("QUARK_COOKIE")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .is_some()
            {
                AuthSourceOutput {
                    source: "env".to_string(),
                    path: None,
                    legacy: false,
                }
            } else if paths.cookie_file.exists() {
                AuthSourceOutput {
                    source: paths.source.stored_source().to_string(),
                    path: Some(paths.cookie_file.display().to_string()),
                    legacy: paths.source == AppPathSource::Legacy,
                }
            } else {
                AuthSourceOutput {
                    source: "none".to_string(),
                    path: None,
                    legacy: false,
                }
            };
            print_output(flags, &output)?;
        }
    }
    Ok(())
}

async fn handle_list(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: ListArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.all {
        let page = list_all_entries(quark_pan, &args.pdir_fid, args.size).await?;
        let page = ListPage {
            entries: page,
            page: 1,
            size: args.size,
            total: 0,
        };
        return print_list_output(flags, &page, args.long, args.raw_time);
    }
    if args.more {
        return handle_list_more(flags, quark_pan, args).await;
    }
    let page = quark_pan
        .list()
        .pdir_fid(args.pdir_fid)
        .page(args.page)
        .size(args.size)
        .prepare()?
        .request()
        .await?;
    print_list_output(flags, &page, args.long, args.raw_time)
}

async fn handle_delete(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: DeleteArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    quark_pan.delete(&args.fid).await?;
    print_output(flags, &DeleteOutput { fids: args.fid })?;
    Ok(())
}

async fn handle_ls(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: LsArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ShellState::root();
    let (pdir_fid, _) =
        shell::resolve_dir_path(quark_pan, &state, args.remote_path_or_fid.as_deref()).await?;
    let entries = list_all_entries(quark_pan, &pdir_fid, 100).await?;
    let page = ListPage {
        entries,
        page: 1,
        size: 100,
        total: 0,
    };
    print_list_output(flags, &page, args.long, args.raw_time)
}

async fn handle_get(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: GetArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ShellState::root();
    let (entry, _) = shell::resolve_entry_path(quark_pan, &state, &args.remote_path_or_fid).await?;
    let output = args
        .local_path
        .clone()
        .unwrap_or_else(|| PathBuf::from(&entry.file_name));
    if entry.dir {
        return handle_download_dir(
            flags,
            quark_pan,
            DownloadDirArgs::from_get(entry.fid, output, args),
        )
        .await;
    }
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
            overwrite: args.overwrite,
            continue_download: args.continue_download,
            retry: args.retry,
            retry_delay: args.retry_delay,
            retry_max_delay: args.retry_max_delay,
            retry_backoff: args.retry_backoff,
            verify: args.verify,
            no_verify: args.no_verify,
        },
    )
    .await
}

async fn handle_put(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: PutArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ShellState::root();
    let (pdir_fid, _) =
        shell::resolve_dir_path(quark_pan, &state, args.remote_dir_or_fid.as_deref()).await?;
    if args.local_path.is_dir() {
        handle_upload_dir(
            flags,
            quark_pan,
            UploadDirArgs {
                pdir_fid,
                dir: args.local_path,
                file_name: None,
                r#continue: args.r#continue,
                overwrite: args.overwrite,
            },
        )
        .await
    } else {
        handle_upload(
            flags,
            quark_pan,
            UploadArgs {
                pdir_fid,
                file: args.local_path,
                file_name: None,
                r#continue: args.r#continue,
                overwrite: args.overwrite,
            },
        )
        .await
    }
}

async fn handle_rm(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: RmArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ShellState::root();
    let (entry, path) =
        shell::resolve_entry_path(quark_pan, &state, &args.remote_path_or_fid).await?;
    if !args.yes && flags.interactive {
        eprint!("delete {path}? [y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") && !answer.trim().eq_ignore_ascii_case("yes") {
            return Ok(());
        }
    } else if !args.yes {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "refusing to delete without --yes in non-interactive mode",
        )));
    }
    handle_delete(
        flags,
        quark_pan,
        DeleteArgs {
            fid: vec![entry.fid],
        },
    )
    .await
}

async fn handle_mkdir(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: MkdirArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ShellState::root();
    let (pdir_fid, file_name) =
        shell::resolve_parent_and_name(quark_pan, &state, &args.remote_path).await?;
    handle_folder(
        flags,
        quark_pan,
        FolderArgs {
            command: FolderCommand::Create(FolderCreateArgs {
                pdir_fid,
                file_name,
            }),
        },
    )
    .await
}

async fn handle_mv(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: MvArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.new_name_or_path.contains('/') {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "mv only supports renaming within the same remote directory",
        )));
    }
    let state = ShellState::root();
    let (entry, _) = shell::resolve_entry_path(quark_pan, &state, &args.remote_path_or_fid).await?;
    handle_rename(
        flags,
        quark_pan,
        RenameArgs {
            fid: entry.fid,
            file_name: args.new_name_or_path,
        },
    )
    .await
}

async fn handle_stat(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: StatArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ShellState::root();
    let (entry, _) = shell::resolve_entry_path(quark_pan, &state, &args.remote_path_or_fid).await?;
    print_output(
        flags,
        &StatOutput {
            fid: entry.fid,
            name: entry.file_name,
            dir: entry.dir,
            size: entry.size,
            updated_at: entry.updated_at,
        },
    )
}

async fn handle_probe(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: ProbeArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        ProbeCommand::Download(args) => handle_probe_download(flags, quark_pan, args).await,
    }
}

async fn handle_probe_download(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: ProbeDownloadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = quark_pan.download().fid(args.fid.clone()).prepare()?;
    let info = request.info().await?;
    let download_url_present = !info.download_url.trim().is_empty();
    let md5_present = info
        .md5
        .as_deref()
        .is_some_and(|md5| !md5.trim().is_empty());
    let mut first_bytes = 0usize;
    let range = match quark_pan
        .download()
        .fid(args.fid.clone())
        .start_offset(1)
        .prepare()?
        .stream()
        .await
    {
        Ok(mut stream) => match stream.next().await {
            Some(Ok(bytes)) => {
                first_bytes = bytes.len().min(16);
                "ok".to_string()
            }
            Some(Err(err)) => format!("failed: {err}"),
            None => "ok".to_string(),
        },
        Err(err) => format!("failed: {err}"),
    };
    print_output(
        flags,
        &ProbeDownloadOutput {
            fid: info.fid,
            download_url: if download_url_present {
                "present".to_string()
            } else {
                "missing".to_string()
            },
            md5: if md5_present {
                "present".to_string()
            } else {
                "missing".to_string()
            },
            range,
            first_bytes,
            sensitive_download_url: flags.debug.then_some(info.download_url),
        },
    )
}

async fn handle_list_more(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: ListArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut page_no = args.page;
    let stdin = std::io::stdin();
    loop {
        let page = quark_pan
            .list()
            .pdir_fid(args.pdir_fid.clone())
            .page(page_no)
            .size(args.size)
            .prepare()?
            .request()
            .await?;
        print_list_output(flags, &page, args.long, args.raw_time)?;
        if page.entries.len() < args.size as usize {
            break;
        }
        print!("-- More -- page {} | Enter next page | q quit: ", page_no);
        std::io::stdout().flush()?;
        let mut line = String::new();
        stdin.read_line(&mut line)?;
        if line.trim().eq_ignore_ascii_case("q") {
            break;
        }
        page_no += 1;
    }
    Ok(())
}

pub(crate) fn print_list_output(
    flags: OutputFlags,
    page: &ListPage,
    long: bool,
    raw_time: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if flags.format == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(page)?);
        return Ok(());
    }
    if !flags.quiet {
        println!(
            "page={} shown={} total={}{}",
            page.page,
            page.entries.len(),
            page.total,
            if long { " (long)" } else { "" }
        );
    }
    if long {
        println!(
            "{}",
            format_header(
                flags,
                &format!(
                    "{:<4} {:>12} {:<16} {} {}",
                    "TYPE", "SIZE", "UPDATED", "FID", "NAME"
                )
            )
        );
    } else {
        println!(
            "{}",
            format_header(
                flags,
                &format!("{:<4} {:>12} {} {}", "TYPE", "SIZE", "FID", "NAME")
            )
        );
    }
    for entry in &page.entries {
        let ty = if entry.dir { "DIR" } else { "FILE" };
        let size = if entry.dir {
            "-".to_string()
        } else {
            entry.size.to_string()
        };
        if long {
            println!(
                "{:<4} {:>12} {:<16} {} {}",
                ty,
                size,
                format_time(entry.updated_at, raw_time),
                entry.fid,
                entry.file_name
            );
        } else {
            println!("{:<4} {:>12} {} {}", ty, size, entry.fid, entry.file_name);
        }
    }
    Ok(())
}

pub(crate) async fn handle_download(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: DownloadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    download_file(flags, quark_pan, &args).await
}

async fn download_file(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: &DownloadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.output.is_some() == args.stdout {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "exactly one of --output or --stdout is required",
        )));
    }
    let request = quark_pan.download().fid(args.fid.clone()).prepare()?;
    if args.stdout {
        let mut stream = request.stream().await?;
        let mut stdout = tokio::io::stdout();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            stdout.write_all(chunk.as_ref()).await?;
        }
        stdout.flush().await?;
        return Ok(());
    }

    let info = request.info().await?;
    let verify_mode = args.verify_mode();
    let output = args.output.clone().expect("checked above");
    let part_path = partial_download_path(&output);
    let task_path = file_task_path(&output);
    if has_same_download_target(&output, info.md5.as_deref()).await? {
        cleanup_download_resume_artifacts(&part_path, &task_path).await?;
        if !flags.quiet {
            eprintln!("download skipped: local file already matches remote md5");
        }
        return Ok(());
    }

    if let Some(task) = read_json_file::<DownloadTask>(&task_path).await? {
        let same_target = task.fid == args.fid
            && task.output_path == output.to_string_lossy()
            && task.part_path == part_path.to_string_lossy();
        if !same_target {
            cleanup_download_resume_artifacts(&part_path, &task_path).await?;
        }
    }

    if output.exists() && !args.overwrite {
        return Err(Box::new(QuarkPanError::invalid_argument(format!(
            "output already exists: {} (use --overwrite)",
            output.display()
        ))));
    }
    if args.overwrite && !args.continue_download {
        cleanup_download_artifacts(&output, &part_path, &task_path).await?;
    }

    let task = DownloadTask::new(
        args.fid.clone(),
        &output,
        &part_path,
        info.md5.clone(),
        None,
        verify_mode,
    );
    write_json_file(&task_path, &task).await?;

    download_with_retry(
        flags,
        quark_pan,
        &args.fid,
        &part_path,
        args.continue_download,
        &args.retry,
        args.retry_delay,
        args.retry_max_delay,
        args.retry_backoff,
    )
    .await?;

    let local = md5_file(&part_path).await?;
    match verify_download_checksum(verify_mode, &local, info.md5.as_deref())? {
        VerificationOutcome::Verified | VerificationOutcome::NotAvailable => {}
        VerificationOutcome::Skipped if !flags.quiet => {
            eprintln!("download verification skipped");
        }
        VerificationOutcome::Skipped => {}
    }
    if args.overwrite {
        remove_if_exists(&output).await?;
    }
    tokio::fs::rename(&part_path, &output).await?;
    remove_if_exists(&task_path).await?;
    Ok(())
}

async fn download_with_retry(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    fid: &str,
    output: &Path,
    allow_continue: bool,
    retry: &RetryMode,
    retry_delay: u64,
    retry_max_delay: u64,
    retry_backoff: RetryBackoff,
) -> Result<(), Box<dyn std::error::Error>> {
    let control = if flags.no_progress || flags.quiet || !flags.interactive {
        None
    } else {
        let control = TransferControl::new(None);
        spawn_ctrl_c_cancel(control.clone());
        spawn_progress_printer(control.clone(), progress_label("download", output));
        Some(control)
    };
    let mut attempts = 0_u32;
    loop {
        let start_offset = if allow_continue && output.exists() {
            tokio::fs::metadata(output).await?.len()
        } else if attempts > 0 && output.exists() {
            tokio::fs::metadata(output).await?.len()
        } else {
            0
        };

        let mut builder = quark_pan.download().fid(fid.to_string());
        if start_offset > 0 {
            builder = builder.start_offset(start_offset);
        }
        let raw_stream = builder.prepare()?.stream().await;
        let raw_stream = match raw_stream {
            Ok(stream) => stream,
            Err(err) if start_offset > 0 && is_unsupported_resume_error(&err) => {
                remove_if_exists(output).await?;
                if flags.debug && !flags.quiet {
                    eprintln!(
                        "download resume unavailable at offset {start_offset}; restarting file"
                    );
                }
                continue;
            }
            Err(err) if should_retry_download(retry, attempts, &err) => {
                attempts += 1;
                if let Some(control) = &control {
                    control.increment_reconnects();
                }
                if flags.debug && !flags.quiet {
                    eprintln!("download reconnect {attempts}: {err}");
                }
                let sleep_secs =
                    retry_sleep_secs(attempts, retry_delay, retry_max_delay, retry_backoff);
                tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
                continue;
            }
            Err(err) => return Err(Box::new(err)),
        };

        let mut file = if start_offset > 0 {
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(output)
                .await?
        } else {
            tokio::fs::File::create(output).await?
        };

        let result = if let Some(control) = &control {
            let mut stream = ProgressStream::new(raw_stream, control.clone());
            write_stream_to_file(&mut stream, &mut file).await
        } else {
            let mut stream = raw_stream;
            write_stream_to_file(&mut stream, &mut file).await
        };
        file.flush().await?;

        match result {
            Ok(()) => {
                if control.is_some() {
                    if let Some(control) = &control {
                        control.finish();
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    eprintln!();
                }
                return Ok(());
            }
            Err(err) if should_retry_boxed_download(retry, attempts, err.as_ref()) => {
                attempts += 1;
                if let Some(control) = &control {
                    control.increment_reconnects();
                }
                if flags.debug && !flags.quiet {
                    eprintln!("download reconnect {attempts}: {err}");
                }
                let sleep_secs =
                    retry_sleep_secs(attempts, retry_delay, retry_max_delay, retry_backoff);
                tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

fn should_retry_download(retry: &RetryMode, attempts: u32, err: &QuarkPanError) -> bool {
    is_retryable_error(err) && retry_allows_attempt(retry, attempts)
}

fn should_retry_boxed_download(
    retry: &RetryMode,
    attempts: u32,
    err: &(dyn std::error::Error + 'static),
) -> bool {
    is_retryable_boxed_error(err) && retry_allows_attempt(retry, attempts)
}

fn retry_allows_attempt(retry: &RetryMode, attempts: u32) -> bool {
    match retry {
        RetryMode::Auto => attempts < 1000,
        RetryMode::Infinite => true,
        RetryMode::Count(limit) => attempts < *limit,
    }
}

fn retry_sleep_secs(attempt: u32, base: u64, max: u64, backoff: RetryBackoff) -> u64 {
    match backoff {
        RetryBackoff::Fixed => base.min(max),
        RetryBackoff::Exponential => {
            let shift = attempt.saturating_sub(1).min(6);
            base.saturating_mul(1_u64 << shift).min(max)
        }
    }
}

fn is_retryable_error(err: &QuarkPanError) -> bool {
    matches!(
        err.retry_class(),
        RetryClass::Transient | RetryClass::RateLimited
    )
}

fn is_retryable_boxed_error(err: &(dyn std::error::Error + 'static)) -> bool {
    err.downcast_ref::<QuarkPanError>()
        .is_some_and(is_retryable_error)
}

fn is_unsupported_resume_error(err: &QuarkPanError) -> bool {
    matches!(
        err,
        QuarkPanError::InvalidArgument(message)
            if message == "server did not honor range request for resume download"
    )
}

pub(crate) async fn handle_download_dir(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: DownloadDirArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let task_path = dir_task_path(&args.output)?;
    let existing_task = read_json_file::<DownloadDirTask>(&task_path).await?;
    let merge_mode = args.continue_download && args.overwrite;
    let verify_mode = args.verify_mode();

    if args.output.exists() && !args.continue_download && !args.overwrite {
        return Err(Box::new(QuarkPanError::invalid_argument(format!(
            "output directory already exists: {}",
            args.output.display()
        ))));
    }
    if args.continue_download && existing_task.is_none() && args.output.exists() && !merge_mode {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "no interrupted directory task found; local directory already exists or download already completed",
        )));
    }

    tokio::fs::create_dir_all(&args.output).await?;
    let files = collect_remote_files(quark_pan, &args.pdir_fid, Path::new("")).await?;
    let mut task = existing_task.unwrap_or(DownloadDirTask {
        kind: "download_dir".to_string(),
        pdir_fid: args.pdir_fid.clone(),
        output_dir: args.output.to_string_lossy().to_string(),
        entries: files
            .iter()
            .map(|item| DownloadDirEntryTask {
                relative_path: item.relative_path.to_string_lossy().to_string(),
                fid: item.fid.clone(),
                md5: None,
                status: DirEntryStatus::Pending,
            })
            .collect(),
    });
    write_json_file(&task_path, &task).await?;

    for idx in 0..task.entries.len() {
        if matches!(
            task.entries[idx].status,
            DirEntryStatus::Done | DirEntryStatus::Skipped
        ) {
            continue;
        }
        let output_path = args.output.join(&task.entries[idx].relative_path);
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let info = quark_pan
            .download()
            .fid(task.entries[idx].fid.clone())
            .prepare()?
            .info()
            .await?;
        task.entries[idx].md5 = info.md5.clone();

        if merge_mode && has_same_download_target(&output_path, info.md5.as_deref()).await? {
            task.entries[idx].status = DirEntryStatus::Skipped;
            write_json_file(&task_path, &task).await?;
            continue;
        }
        if output_path.exists() && !merge_mode && !args.continue_download {
            return Err(Box::new(QuarkPanError::invalid_argument(format!(
                "local file already exists: {}",
                output_path.display()
            ))));
        }

        task.entries[idx].status = DirEntryStatus::Running;
        write_json_file(&task_path, &task).await?;
        let file_args = DownloadArgs {
            fid: task.entries[idx].fid.clone(),
            output: Some(output_path),
            stdout: false,
            overwrite: merge_mode,
            continue_download: args.continue_download,
            retry: args.retry.clone(),
            retry_delay: args.retry_delay,
            retry_max_delay: args.retry_max_delay,
            retry_backoff: args.retry_backoff,
            verify: verify_mode,
            no_verify: false,
        };
        match download_file(flags, quark_pan, &file_args).await {
            Ok(()) => task.entries[idx].status = DirEntryStatus::Done,
            Err(err) => {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(err);
            }
        }
        write_json_file(&task_path, &task).await?;
    }

    remove_if_exists(&task_path).await?;
    Ok(())
}

pub(crate) async fn handle_folder(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: FolderArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        FolderCommand::Create(args) => {
            let fid = quark_pan
                .create_folder()
                .pdir_fid(args.pdir_fid)
                .file_name(args.file_name)
                .prepare()?
                .request()
                .await?;
            print_output(flags, &FolderCreateOutput { fid })?;
        }
    }
    Ok(())
}

pub(crate) async fn handle_rename(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: RenameArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    quark_pan
        .rename()
        .fid(args.fid.clone())
        .file_name(args.file_name.clone())
        .prepare()?
        .request()
        .await?;
    print_output(
        flags,
        &RenameOutput {
            fid: args.fid,
            file_name: args.file_name,
        },
    )?;
    Ok(())
}

pub(crate) async fn handle_upload(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: UploadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let task_path = file_task_path(&args.file);
    if args.r#continue {
        return resume_upload(flags, quark_pan, args, task_path).await;
    }

    let local = hash_file(&args.file, args.file_name.as_deref()).await?;
    let prepared = quark_pan
        .upload()
        .pdir_fid(args.pdir_fid.clone())
        .file_name(local.name.clone())
        .size(local.size)
        .md5(local.md5.clone())
        .sha1(local.sha1.clone())
        .prepare()
        .await?;

    match prepared {
        UploadPrepareResult::RapidUploaded { fid } => {
            remove_if_exists(&task_path).await?;
            print_output(
                flags,
                &UploadDoneOutput {
                    fid,
                    rapid_upload: true,
                },
            )?;
        }
        UploadPrepareResult::NeedUpload(session) => {
            let upload_task = UploadTask {
                kind: "upload".to_string(),
                file_path: args.file.to_string_lossy().to_string(),
                file_name: local.name.clone(),
                pdir_fid: args.pdir_fid,
                size: local.size,
                md5: local.md5,
                sha1: local.sha1,
                resume: session.to_resume(),
                state: UploadResumeState {
                    next_part_number: 1,
                    part_etags: Vec::new(),
                },
            };
            write_json_file(&task_path, &upload_task).await?;
            let completed = upload_file_with_task(
                flags,
                quark_pan,
                &args.file,
                upload_task.clone(),
                task_path.as_path(),
            )
            .await?;
            remove_if_exists(&task_path).await?;
            print_output(
                flags,
                &UploadDoneOutput {
                    fid: completed.fid,
                    rapid_upload: completed.rapid_upload,
                },
            )?;
        }
    }
    Ok(())
}

async fn resume_upload(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: UploadArgs,
    task_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(task) = read_json_file::<UploadTask>(&task_path).await? else {
        return Err(Box::new(QuarkPanError::invalid_argument(format!(
            "upload task file not found: {}",
            task_path.display()
        ))));
    };
    let file_path = PathBuf::from(&task.file_path);
    if file_path != args.file {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "--file does not match task file",
        )));
    }
    let local = hash_file(&args.file, Some(&task.file_name)).await?;
    if local.size != task.size || local.md5 != task.md5 || local.sha1 != task.sha1 {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "local file size/md5/sha1 does not match upload task",
        )));
    }
    let completed =
        upload_file_with_task(flags, quark_pan, &args.file, task, task_path.as_path()).await?;
    remove_if_exists(&task_path).await?;
    print_output(
        flags,
        &UploadDoneOutput {
            fid: completed.fid,
            rapid_upload: completed.rapid_upload,
        },
    )?;
    Ok(())
}

pub(crate) async fn handle_upload_dir(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: UploadDirArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_dir = tokio::fs::canonicalize(&args.dir).await?;
    let task_path = dir_task_path(&source_dir)?;
    let existing_task = read_json_file::<UploadDirTask>(&task_path).await?;
    let root_name = args.file_name.clone().unwrap_or_else(|| {
        source_dir
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("root")
            .to_string()
    });
    let merge_mode = args.r#continue && args.overwrite;

    let root_fid = if let Some(task) = &existing_task {
        task.root_fid.clone()
    } else {
        let existing = find_entry_by_name(quark_pan, &args.pdir_fid, &root_name).await?;
        match existing {
            Some(entry) if entry.dir && !args.r#continue && !args.overwrite => {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "target cloud folder already exists",
                )));
            }
            Some(entry) if entry.dir && args.r#continue && !merge_mode => {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "no interrupted directory task found; cloud folder already exists or upload already completed",
                )));
            }
            Some(entry) if entry.dir => entry.fid,
            Some(_) => {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "target cloud entry exists and is not a folder",
                )));
            }
            None => {
                quark_pan
                    .create_folder()
                    .pdir_fid(args.pdir_fid.clone())
                    .file_name(root_name.clone())
                    .prepare()?
                    .request()
                    .await?
            }
        }
    };

    let files = collect_local_files(&source_dir).await?;
    let mut task = existing_task.unwrap_or(UploadDirTask {
        kind: "upload_dir".to_string(),
        source_dir: source_dir.to_string_lossy().to_string(),
        pdir_fid: args.pdir_fid.clone(),
        target_file_name: root_name.clone(),
        root_fid: root_fid.clone(),
        entries: files
            .iter()
            .map(|item| UploadDirEntryTask {
                relative_path: item.relative_path.to_string_lossy().to_string(),
                status: DirEntryStatus::Pending,
            })
            .collect(),
    });
    write_json_file(&task_path, &task).await?;

    let mut folder_cache = HashMap::new();
    folder_cache.insert(PathBuf::new(), root_fid);

    for idx in 0..task.entries.len() {
        if matches!(
            task.entries[idx].status,
            DirEntryStatus::Done | DirEntryStatus::Skipped
        ) {
            continue;
        }
        task.entries[idx].status = DirEntryStatus::Running;
        write_json_file(&task_path, &task).await?;

        let relative_path = PathBuf::from(&task.entries[idx].relative_path);
        let absolute_path = source_dir.join(&relative_path);
        let parent_relative = relative_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        let remote_parent = ensure_remote_folder_chain(
            quark_pan,
            &mut folder_cache,
            &task.root_fid,
            &parent_relative,
        )
        .await?;
        let file_name = absolute_path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| QuarkPanError::invalid_argument("invalid file name"))?
            .to_string();

        if let Some(existing) = find_entry_by_name(quark_pan, &remote_parent, &file_name).await? {
            if !merge_mode {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(Box::new(QuarkPanError::invalid_argument(format!(
                    "cloud file already exists: {}",
                    relative_path.display()
                ))));
            }
            if existing.dir {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(Box::new(QuarkPanError::invalid_argument(format!(
                    "cloud entry is a folder but local path is a file: {}",
                    relative_path.display()
                ))));
            }
            let local = hash_file(&absolute_path, Some(&file_name)).await?;
            let remote = quark_pan
                .download()
                .fid(existing.fid.clone())
                .prepare()?
                .info()
                .await?;
            if let Some(md5) = remote.md5 {
                if md5.eq_ignore_ascii_case(&local.md5) {
                    task.entries[idx].status = DirEntryStatus::Skipped;
                    write_json_file(&task_path, &task).await?;
                    continue;
                }
            }
            quark_pan.delete(&[existing.fid]).await?;
        }

        let upload_args = UploadArgs {
            pdir_fid: remote_parent,
            file: absolute_path,
            file_name: Some(file_name),
            r#continue: false,
            overwrite: false,
        };
        match handle_upload(flags, quark_pan, upload_args).await {
            Ok(()) => task.entries[idx].status = DirEntryStatus::Done,
            Err(err) => {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(err);
            }
        }
        write_json_file(&task_path, &task).await?;
    }

    remove_if_exists(&task_path).await?;
    Ok(())
}

async fn upload_file_with_task(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    file_path: &Path,
    mut task: UploadTask,
    task_path: &Path,
) -> Result<libquarkpan::UploadComplete, Box<dyn std::error::Error>> {
    let mut file = tokio::fs::File::open(file_path).await?;
    let start_part = task.state.next_part_number.max(1);
    let seek_to = ((start_part - 1) as u64) * task.resume.part_size;
    if seek_to > 0 {
        file.seek(std::io::SeekFrom::Start(seek_to)).await?;
    }
    let stream = ReaderStream::new(file);
    let session = quark_pan.upload().resume(task.resume.clone());
    let total_remaining = task.size.saturating_sub(seek_to);
    let state = task.state.clone();
    let task_path = task_path.to_path_buf();

    let on_part_uploaded = move |state: &UploadResumeState| -> libquarkpan::Result<()> {
        task.state = state.clone();
        let data = serde_json::to_vec_pretty(&task)?;
        std::fs::write(&task_path, data)?;
        Ok(())
    };

    if flags.no_progress || flags.quiet || !flags.interactive {
        Ok(session
            .upload_stream_resumable(stream, state, on_part_uploaded)
            .await?)
    } else {
        let control = TransferControl::new(Some(total_remaining));
        spawn_ctrl_c_cancel(control.clone());
        spawn_progress_printer(control.clone(), progress_label("upload", file_path));
        let stream = ProgressStream::new(stream, control);
        let completed = session
            .upload_stream_resumable(stream, state, on_part_uploaded)
            .await?;
        eprintln!();
        Ok(completed)
    }
}

async fn write_stream_to_file<S>(
    stream: &mut S,
    file: &mut tokio::fs::File,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Stream<Item = Result<bytes::Bytes, QuarkPanError>> + Unpin,
{
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(chunk.as_ref()).await?;
    }
    Ok(())
}

pub(crate) async fn list_all_entries(
    quark_pan: &QuarkPan,
    pdir_fid: &str,
    size: u32,
) -> Result<Vec<QuarkEntry>, Box<dyn std::error::Error>> {
    let mut page_no = 1;
    let mut entries = Vec::new();
    loop {
        let page = quark_pan
            .list()
            .pdir_fid(pdir_fid.to_string())
            .page(page_no)
            .size(size)
            .prepare()?
            .request()
            .await?;
        let count = page.entries.len();
        entries.extend(page.entries);
        if count < size as usize {
            break;
        }
        page_no += 1;
    }
    Ok(entries)
}

async fn collect_remote_files(
    quark_pan: &QuarkPan,
    pdir_fid: &str,
    prefix: &Path,
) -> Result<Vec<RemoteFileItem>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    let mut stack = vec![(pdir_fid.to_string(), prefix.to_path_buf())];
    while let Some((current_pdir_fid, current_prefix)) = stack.pop() {
        let entries = list_all_entries(quark_pan, &current_pdir_fid, 100).await?;
        for entry in entries {
            let path = current_prefix.join(&entry.file_name);
            if entry.dir {
                stack.push((entry.fid, path));
            } else {
                out.push(RemoteFileItem {
                    relative_path: path,
                    fid: entry.fid,
                });
            }
        }
    }
    Ok(out)
}

async fn collect_local_files(dir: &Path) -> Result<Vec<LocalFileItem>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    collect_local_files_inner(dir, dir, &mut out).await?;
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(out)
}

async fn collect_local_files_inner(
    root: &Path,
    current: &Path,
    out: &mut Vec<LocalFileItem>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut dirs = vec![current.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let mut read_dir = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let meta = entry.metadata().await?;
            if meta.is_dir() {
                dirs.push(path);
            } else if meta.is_file() {
                out.push(LocalFileItem {
                    relative_path: path.strip_prefix(root)?.to_path_buf(),
                });
            }
        }
    }
    Ok(())
}

pub(crate) async fn find_entry_by_name(
    quark_pan: &QuarkPan,
    pdir_fid: &str,
    name: &str,
) -> Result<Option<QuarkEntry>, Box<dyn std::error::Error>> {
    let entries = list_all_entries(quark_pan, pdir_fid, 100).await?;
    Ok(entries.into_iter().find(|entry| entry.file_name == name))
}

async fn ensure_remote_folder_chain(
    quark_pan: &QuarkPan,
    cache: &mut HashMap<PathBuf, String>,
    root_fid: &str,
    relative: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if relative.as_os_str().is_empty() {
        return Ok(root_fid.to_string());
    }
    if let Some(found) = cache.get(relative) {
        return Ok(found.clone());
    }
    let mut current_rel = PathBuf::new();
    let mut current_id = root_fid.to_string();
    for component in relative.components() {
        current_rel.push(component.as_os_str());
        if let Some(found) = cache.get(&current_rel) {
            current_id = found.clone();
            continue;
        }
        let name = component.as_os_str().to_string_lossy().to_string();
        let next_id =
            if let Some(existing) = find_entry_by_name(quark_pan, &current_id, &name).await? {
                if !existing.dir {
                    return Err(Box::new(QuarkPanError::invalid_argument(format!(
                        "cloud path component exists as file: {}",
                        current_rel.display()
                    ))));
                }
                existing.fid
            } else {
                quark_pan
                    .create_folder()
                    .pdir_fid(current_id.clone())
                    .file_name(name)
                    .prepare()?
                    .request()
                    .await?
            };
        cache.insert(current_rel.clone(), next_id.clone());
        current_id = next_id;
    }
    Ok(current_id)
}

async fn read_json_file<T>(path: &Path) -> Result<Option<T>, Box<dyn std::error::Error>>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(None);
    }
    let data = tokio::fs::read(path).await?;
    Ok(Some(serde_json::from_slice(&data)?))
}

async fn write_json_file<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::to_vec_pretty(value)?;
    tokio::fs::write(path, data).await?;
    Ok(())
}

fn file_task_path(path: &Path) -> PathBuf {
    let base = path.as_os_str().to_string_lossy().to_string();
    PathBuf::from(format!("{base}.quark.task"))
}

fn partial_download_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_os_string();
    path.push(".part");
    PathBuf::from(path)
}

fn dir_task_path(path: &Path) -> Result<PathBuf, QuarkPanError> {
    let parent = path
        .parent()
        .ok_or_else(|| QuarkPanError::invalid_argument("directory has no parent"))?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| QuarkPanError::invalid_argument("invalid directory name"))?;
    Ok(parent.join(format!("{name}.quark.task")))
}

async fn cleanup_download_artifacts(
    output: &Path,
    part_path: &Path,
    task_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    remove_if_exists(output).await?;
    cleanup_download_resume_artifacts(part_path, task_path).await
}

async fn cleanup_download_resume_artifacts(
    part_path: &Path,
    task_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    remove_if_exists(part_path).await?;
    remove_if_exists(task_path).await?;
    Ok(())
}

async fn remove_if_exists(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

async fn has_same_download_target(
    output: &Path,
    remote_md5: Option<&str>,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !output.exists() {
        return Ok(false);
    }
    let Some(remote_md5) = remote_md5 else {
        return Ok(false);
    };
    let local_md5 = md5_file(output).await?;
    Ok(md5_matches_remote(&local_md5, remote_md5))
}

async fn hash_file(
    path: &Path,
    name: Option<&str>,
) -> Result<HashOutput, Box<dyn std::error::Error>> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut md5_ctx = md5::Context::new();
    let mut sha1_ctx = sha1::Sha1::new();
    let mut size = 0_u64;
    let mut buf = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        size += read as u64;
        md5_ctx.consume(&buf[..read]);
        sha1_ctx.update(&buf[..read]);
    }
    let name = match name {
        Some(name) => name.to_string(),
        None => path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| QuarkPanError::invalid_argument("invalid file name"))?
            .to_string(),
    };
    Ok(HashOutput {
        name,
        size,
        md5: format!("{:x}", md5_ctx.compute()),
        sha1: format!("{:x}", sha1_ctx.finalize()),
    })
}

async fn md5_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut md5_ctx = md5::Context::new();
    let mut buf = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        md5_ctx.consume(&buf[..read]);
    }
    Ok(format!("{:x}", md5_ctx.compute()))
}

fn print_output<T: Serialize>(
    flags: OutputFlags,
    data: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::to_value(data)?;
    if flags.format == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    if let Ok(upload) = serde_json::from_value::<UploadDoneOutput>(value.clone()) {
        let rendered = if upload.rapid_upload {
            format!("rapid upload completed: {}", upload.fid)
        } else {
            format!("upload completed: {}", upload.fid)
        };
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(delete) = serde_json::from_value::<DeleteOutput>(value.clone()) {
        let rendered = format!(
            "deleted {} entr{}",
            delete.fids.len(),
            if delete.fids.len() == 1 { "y" } else { "ies" }
        );
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(rename) = serde_json::from_value::<RenameOutput>(value.clone()) {
        let rendered = format!("renamed {} -> {}", rename.fid, rename.file_name);
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(folder) = serde_json::from_value::<FolderCreateOutput>(value.clone()) {
        let rendered = format!("folder created: {}", folder.fid);
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(auth) = serde_json::from_value::<AuthSourceOutput>(value.clone()) {
        let rendered = match auth.path {
            Some(path) => format!("{}: {}", auth.source, path),
            None => auth.source,
        };
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(probe) = serde_json::from_value::<ProbeDownloadOutput>(value.clone()) {
        println!("fid: {}", probe.fid);
        println!("download_url: {}", probe.download_url);
        println!("md5: {}", probe.md5);
        println!("range: {}", probe.range);
        println!("first_bytes: {}", probe.first_bytes);
        if let Some(url) = probe.sensitive_download_url {
            println!("sensitive_download_url: {url}");
        }
    } else {
        let rendered = serde_json::to_string_pretty(&value)?;
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    }
    Ok(())
}

fn read_cookie_from_stdin() -> Result<String, Box<dyn std::error::Error>> {
    if std::io::stdin().is_terminal() {
        eprintln!("paste cookie, then press Enter:");
    }
    let stdin = std::io::stdin();
    let mut line = String::new();
    stdin.read_line(&mut line)?;
    let cookie = line.trim().to_string();
    if cookie.is_empty() {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "cookie cannot be empty",
        )));
    }
    Ok(cookie)
}

fn edit_cookie_with(editor: &str) -> Result<String, Box<dyn std::error::Error>> {
    let temp_path = temporary_cookie_path(editor);
    std::fs::write(&temp_path, b"")?;
    let status = Command::new(editor).arg(&temp_path).status()?;
    let result = if status.success() {
        let cookie = std::fs::read_to_string(&temp_path)?.trim().to_string();
        if cookie.is_empty() {
            Err(
                Box::new(QuarkPanError::invalid_argument("cookie cannot be empty"))
                    as Box<dyn std::error::Error>,
            )
        } else {
            Ok(cookie)
        }
    } else {
        Err(Box::new(QuarkPanError::invalid_argument(format!(
            "{editor} exited with status {status}"
        ))) as Box<dyn std::error::Error>)
    };
    let _ = std::fs::remove_file(&temp_path);
    result
}

fn temporary_cookie_path(editor: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("quark-cookie-{editor}-{pid}-{nanos}.txt"))
}

fn md5_matches_remote(local_hex_md5: &str, remote_md5: &str) -> bool {
    if local_hex_md5.eq_ignore_ascii_case(remote_md5) {
        return true;
    }
    let Ok(raw) = decode_hex(local_hex_md5) else {
        return false;
    };
    general_purpose::STANDARD.encode(raw) == remote_md5
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum VerificationOutcome {
    Verified,
    NotAvailable,
    Skipped,
}

fn verify_download_checksum(
    mode: VerifyMode,
    local_hex_md5: &str,
    remote_md5: Option<&str>,
) -> Result<VerificationOutcome, QuarkPanError> {
    if mode == VerifyMode::Never {
        return Ok(VerificationOutcome::Skipped);
    }
    let Some(remote_md5) = remote_md5.filter(|value| !value.trim().is_empty()) else {
        return match mode {
            VerifyMode::Always => Err(QuarkPanError::invalid_argument(
                "download verification required but remote md5 is missing",
            )),
            VerifyMode::Auto => Ok(VerificationOutcome::NotAvailable),
            VerifyMode::Never => Ok(VerificationOutcome::Skipped),
        };
    };
    if md5_matches_remote(local_hex_md5, remote_md5) {
        Ok(VerificationOutcome::Verified)
    } else {
        Err(QuarkPanError::invalid_argument(format!(
            "download md5 mismatch: local={}, remote={}",
            local_hex_md5, remote_md5
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_paths_uses_explicit_config_file_exactly() {
        let base = std::env::temp_dir().join("quarkcli-test-explicit");
        let config_file = base.join("custom.toml");

        let paths = select_app_paths(
            Some(config_file.clone()),
            base.join("new"),
            base.join("legacy"),
        )
        .unwrap();

        assert_eq!(paths.source, AppPathSource::Explicit);
        assert_eq!(paths.config_file, config_file);
        assert_eq!(paths.cookie_file, base.join("cookie.txt"));
        assert_eq!(paths.write_config_dir, base);
    }

    #[test]
    fn app_paths_prefers_existing_new_config_path() {
        let base = std::env::temp_dir().join("quarkcli-test-new");
        let new_dir = base.join("quarkcli");
        let legacy_dir = base.join("quarkpan");

        let paths = select_app_paths(None, new_dir.clone(), legacy_dir).unwrap();

        assert_eq!(paths.source, AppPathSource::New);
        assert_eq!(paths.config_file, new_dir.join("config.toml"));
        assert_eq!(paths.write_config_dir, new_dir);
    }

    #[test]
    fn app_paths_accepts_legacy_when_new_missing() {
        let base = std::env::temp_dir().join("quarkcli-test-legacy");
        let new_dir = base.join("quarkcli");
        let legacy_dir = base.join("quarkpan");

        let paths =
            select_existing_app_paths(None, new_dir.clone(), false, legacy_dir.clone(), true)
                .unwrap();

        assert_eq!(paths.source, AppPathSource::Legacy);
        assert_eq!(paths.config_file, legacy_dir.join("config.toml"));
        assert_eq!(paths.cookie_file, legacy_dir.join("cookie.txt"));
        assert_eq!(paths.write_config_dir, new_dir);
        assert_eq!(paths.write_cookie_file, new_dir.join("cookie.txt"));
    }

    #[test]
    fn legacy_auth_source_is_identified() {
        assert_eq!(AppPathSource::Legacy.stored_source(), "stored-legacy");
    }

    #[test]
    fn verify_auto_fails_on_remote_md5_mismatch() {
        let outcome = verify_download_checksum(
            VerifyMode::Auto,
            "4dbede38d219d5e194cabe3863cab2ca",
            Some("eccef295b1bfee6ffd98a4bd75717f08"),
        );

        assert!(outcome.is_err());
    }

    #[test]
    fn verify_never_allows_mismatch() {
        let outcome = verify_download_checksum(
            VerifyMode::Never,
            "4dbede38d219d5e194cabe3863cab2ca",
            Some("eccef295b1bfee6ffd98a4bd75717f08"),
        );

        assert!(matches!(outcome, Ok(VerificationOutcome::Skipped)));
    }

    #[test]
    fn verify_always_fails_when_remote_md5_missing() {
        let outcome =
            verify_download_checksum(VerifyMode::Always, "4dbede38d219d5e194cabe3863cab2ca", None);

        assert!(outcome.is_err());
    }

    #[test]
    fn partial_download_path_appends_part_suffix() {
        assert_eq!(
            partial_download_path(Path::new("file.bin")),
            PathBuf::from("file.bin.part")
        );
    }

    #[test]
    fn download_task_tracks_output_and_part_paths() {
        let output = PathBuf::from("file.bin");
        let part = partial_download_path(&output);
        let task = DownloadTask::new(
            "fid1".to_string(),
            &output,
            &part,
            Some("remote-md5".to_string()),
            Some(42),
            VerifyMode::Auto,
        );

        assert_eq!(task.output_path, "file.bin");
        assert_eq!(task.part_path, "file.bin.part");
        assert_eq!(task.verify, VerifyMode::Auto);
    }

    #[test]
    fn retry_helper_retries_transient_quark_errors_only() {
        let transient = QuarkPanError::Io(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "interrupted",
        ));
        let hard = QuarkPanError::invalid_argument("bad request");

        assert!(is_retryable_error(&transient));
        assert!(!is_retryable_error(&hard));
    }

    #[test]
    fn detects_unsupported_resume_range_error() {
        let unsupported = QuarkPanError::invalid_argument(
            "server did not honor range request for resume download",
        );
        let other = QuarkPanError::invalid_argument("download md5 mismatch");

        assert!(is_unsupported_resume_error(&unsupported));
        assert!(!is_unsupported_resume_error(&other));
    }

    #[test]
    fn retry_mode_parses_auto() {
        assert_eq!("auto".parse::<RetryMode>().unwrap(), RetryMode::Auto);
    }

    #[test]
    fn retry_mode_parses_infinite() {
        assert_eq!(
            "infinite".parse::<RetryMode>().unwrap(),
            RetryMode::Infinite
        );
    }

    #[test]
    fn retry_mode_parses_number() {
        assert_eq!("300".parse::<RetryMode>().unwrap(), RetryMode::Count(300));
    }

    #[test]
    fn retry_sleep_uses_exponential_backoff_with_cap() {
        assert_eq!(retry_sleep_secs(1, 2, 60, RetryBackoff::Exponential), 2);
        assert_eq!(retry_sleep_secs(3, 2, 60, RetryBackoff::Exponential), 8);
        assert_eq!(retry_sleep_secs(10, 2, 60, RetryBackoff::Exponential), 60);
    }

    #[test]
    fn retry_sleep_uses_fixed_backoff_with_cap() {
        assert_eq!(retry_sleep_secs(3, 120, 60, RetryBackoff::Fixed), 60);
    }

    #[test]
    fn progress_message_includes_reconnect_count_only_when_nonzero() {
        assert_eq!(
            progress_message("download file.bin", 0),
            "download file.bin"
        );
        assert_eq!(
            progress_message("download file.bin", 3),
            "download file.bin reconnects:3"
        );
    }

    #[test]
    fn parses_path_first_get_command() {
        let cli =
            Cli::try_parse_from(["quark", "get", "/tvtemp/01.mp4", "./01.mp4", "-c"]).unwrap();

        assert!(matches!(
            cli.command,
            Commands::Get(GetArgs {
                remote_path_or_fid,
                local_path: Some(_),
                continue_download: true,
                ..
            }) if remote_path_or_fid == "/tvtemp/01.mp4"
        ));
    }

    #[test]
    fn get_directory_args_preserve_no_verify_for_download_dir() {
        let get = GetArgs {
            remote_path_or_fid: "ai".to_string(),
            local_path: None,
            overwrite: false,
            continue_download: true,
            retry: RetryMode::Auto,
            retry_delay: 2,
            retry_max_delay: 60,
            retry_backoff: RetryBackoff::Exponential,
            verify: VerifyMode::Auto,
            no_verify: true,
        };

        let dir = DownloadDirArgs::from_get("fid1".to_string(), PathBuf::from("ai"), get);

        assert_eq!(dir.verify_mode(), VerifyMode::Never);
    }

    #[test]
    fn parses_path_first_mutation_commands() {
        assert!(matches!(
            Cli::try_parse_from(["quark", "put", "./file.bin", "/backup/"])
                .unwrap()
                .command,
            Commands::Put(PutArgs { .. })
        ));
        assert!(matches!(
            Cli::try_parse_from(["quark", "ls", "/"]).unwrap().command,
            Commands::Ls(LsArgs { .. })
        ));
        assert!(matches!(
            Cli::try_parse_from(["quark", "rm", "/old.bin", "--yes"])
                .unwrap()
                .command,
            Commands::Rm(RmArgs { yes: true, .. })
        ));
        assert!(matches!(
            Cli::try_parse_from(["quark", "mkdir", "/backup/new"])
                .unwrap()
                .command,
            Commands::Mkdir(MkdirArgs { .. })
        ));
        assert!(matches!(
            Cli::try_parse_from(["quark", "mv", "/old.bin", "new.bin"])
                .unwrap()
                .command,
            Commands::Mv(MvArgs { .. })
        ));
        assert!(matches!(
            Cli::try_parse_from(["quark", "stat", "/file.bin"])
                .unwrap()
                .command,
            Commands::Stat(StatArgs { .. })
        ));
    }

    #[test]
    fn parses_json_output_flags() {
        let cli = Cli::try_parse_from(["quark", "--json", "ls", "/"]).unwrap();
        assert!(cli.json);

        let cli = Cli::try_parse_from(["quark", "ls", "/", "--json"]).unwrap();
        assert!(cli.json);

        let cli = Cli::try_parse_from(["quark", "--format", "json", "stat", "/file.bin"]).unwrap();
        assert_eq!(cli.format, OutputFormat::Json);
    }

    #[test]
    fn parses_probe_download_command() {
        let cli = Cli::try_parse_from(["quark", "probe", "download", "--fid", "abc123"]).unwrap();

        assert!(matches!(
            cli.command,
            Commands::Probe(ProbeArgs {
                command: ProbeCommand::Download(ProbeDownloadArgs { fid })
            }) if fid == "abc123"
        ));
    }

    #[test]
    fn stat_output_uses_name_field_for_json() {
        let value = serde_json::to_value(StatOutput {
            fid: "fid1".to_string(),
            name: "file.bin".to_string(),
            dir: false,
            size: 123,
            updated_at: 456,
        })
        .unwrap();

        assert_eq!(value["name"], "file.bin");
        assert!(value.get("file_name").is_none());
    }
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, QuarkPanError> {
    if !hex.len().is_multiple_of(2) {
        return Err(QuarkPanError::invalid_argument("invalid md5 hex length"));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_value(bytes[i])?;
        let lo = hex_value(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Result<u8, QuarkPanError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(QuarkPanError::invalid_argument("invalid hex digit")),
    }
}

fn spawn_ctrl_c_cancel(control: TransferControl) {
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        control.cancel();
    });
}

fn spawn_progress_printer(control: TransferControl, label: String) {
    let progress_bar = create_progress_bar(&label, control.snapshot().total);
    tokio::spawn(async move {
        let mut rx = control.subscribe();
        while rx.changed().await.is_ok() {
            let progress = *rx.borrow();
            update_progress_bar(&progress_bar, &label, progress);
        }
        progress_bar.finish_and_clear();
    });
}

fn create_progress_bar(label: &str, total: Option<u64>) -> ProgressBar {
    let bar = match total {
        Some(total) => ProgressBar::new(total),
        None => ProgressBar::new_spinner(),
    };
    let style = match total {
        Some(_) => ProgressStyle::with_template(
            "{spinner:.green} {msg:<28} [{bar:36.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta})",
        )
        .unwrap()
        .progress_chars("=> "),
        None => ProgressStyle::with_template("{spinner:.green} {msg:<28} {bytes} ({bytes_per_sec})")
            .unwrap(),
    };
    bar.set_style(style);
    bar.set_message(label.to_string());
    bar
}

fn update_progress_bar(progress_bar: &ProgressBar, label: &str, progress: TransferProgress) {
    progress_bar.set_message(progress_message(label, progress.reconnects));
    progress_bar.set_position(progress.transferred);
    if progress.total.is_none() {
        progress_bar.tick();
    }
}

fn progress_message(label: &str, reconnects: u32) -> String {
    if reconnects == 0 {
        label.to_string()
    } else {
        format!("{label} reconnects:{reconnects}")
    }
}

fn progress_label(action: &str, path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let text = format!("{action} {name}");
    truncate_label(&text, 28)
}

fn truncate_label(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= max_chars {
            break;
        }
        out.push(ch);
        count += 1;
    }
    if text.chars().count() > max_chars && max_chars > 1 {
        out.pop();
        out.push('…');
    }
    out
}

fn resolve_color(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stdout().is_terminal() || std::io::stderr().is_terminal(),
    }
}

fn format_header(flags: OutputFlags, text: &str) -> String {
    if flags.color {
        text.bold().cyan().to_string()
    } else {
        text.to_string()
    }
}

fn format_time(ts_millis: u64, raw: bool) -> String {
    if raw {
        return ts_millis.to_string();
    }
    let secs = (ts_millis / 1000) as i64;
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}
