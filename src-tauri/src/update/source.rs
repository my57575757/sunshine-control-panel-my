//! 基于本地源码树的更新流程：fork 同步 -> 本地 ninja/cpack 构建。
//!
//! 设计约束：
//! - 只用本机 `git` 与凭据管理器，不存储任何 token；
//! - 同步阶段要求固定分支 `my_dev` 且已跟踪文件工作区干净，不自动 stash/reset；
//! - 只快进推送 fork 的 master（等效 GitHub “Sync fork”），非快进一律中止交人工；
//! - 不自动 push 本地 my_dev；
//! - 同步与构建全程不提权、不停 Sunshine 服务，安装阶段才复用提权 helper。

use log::{info, warn};
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 允许同步并入的本地固定分支。
const REQUIRED_BRANCH: &str = "my_dev";
const UPSTREAM_REMOTE: &str = "upstream";
const UPSTREAM_URL: &str = "https://github.com/AlkaidLab/foundation-sunshine.git";
const FORK_REMOTE: &str = "origin";
const BUILD_SCRIPT_RELATIVE: &str = "scripts/update-local-build.ps1";

static SOURCE_UPDATE_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub(super) struct SourceStatus {
    behind: u32,
    ahead: u32,
    head_hash: String,
    commits_markdown: String,
}

// ---------- 目录解析与校验 ----------

fn is_source_tree(root: &Path) -> bool {
    root.join("CMakeLists.txt").is_file()
        && root.join("build").join("CMakeCache.txt").is_file()
        && root.join(BUILD_SCRIPT_RELATIVE).is_file()
        && root.join(".git").exists()
}

fn configured_source_dir() -> Option<PathBuf> {
    let raw = crate::desktop_settings::load_desktop_settings_from_disk().sunshine_source_dir?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// 从当前 GUI exe 位置向上探测源码树（开发态直接从 target 目录启动时可命中）。
fn detect_source_dir() -> Option<PathBuf> {
    let mut current = std::env::current_exe().ok()?.parent().map(Path::to_path_buf);
    for _ in 0..10 {
        let dir = current?;
        if is_source_tree(&dir) {
            return Some(dir);
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// 生效的源码目录：设置项优先（无效则忽略），否则向上探测。
pub(super) fn effective_source_dir() -> Option<PathBuf> {
    if let Some(dir) = configured_source_dir() {
        if is_source_tree(&dir) {
            return Some(dir);
        }
        warn!("配置的 Sunshine 源码目录无效，已忽略: {}", dir.display());
    }
    detect_source_dir()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDirStatus {
    pub valid: bool,
    pub path: Option<String>,
    /// setting | detected | none
    pub source: String,
    pub branch: Option<String>,
    pub reason: Option<String>,
}

/// 前端设置页校验入口：`path` 为 `None` 时回报当前生效的目录（设置项/探测）。
#[tauri::command]
pub fn validate_sunshine_source_dir(path: Option<String>) -> SourceDirStatus {
    let (candidate, source) = match path.as_deref().map(str::trim) {
        Some(dir) if !dir.is_empty() => (Some(PathBuf::from(dir)), "setting".to_string()),
        _ => match configured_source_dir() {
            Some(dir) => (Some(dir), "setting".to_string()),
            None => (detect_source_dir(), "detected".to_string()),
        },
    };

    let Some(dir) = candidate else {
        return SourceDirStatus {
            valid: false,
            path: None,
            source: "none".to_string(),
            branch: None,
            reason: Some("未配置本地 Sunshine 源码目录".to_string()),
        };
    };

    let display = dir.to_string_lossy().to_string();
    if let Err(reason) = validate_tree(&dir) {
        return SourceDirStatus {
            valid: false,
            path: Some(display),
            source,
            branch: None,
            reason: Some(reason),
        };
    }

    let branch = git_trimmed(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    SourceDirStatus {
        valid: true,
        path: Some(display),
        source,
        branch,
        reason: None,
    }
}

fn validate_tree(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("目录不存在：{}", root.display()));
    }
    for relative in ["CMakeLists.txt", "build/CMakeCache.txt", BUILD_SCRIPT_RELATIVE] {
        if !root.join(relative).is_file() {
            return Err(format!(
                "目录中找不到 {relative}，不是已完成 configure 的 Sunshine 源码树"
            ));
        }
    }
    if !root.join(".git").exists() {
        return Err("目录中找不到 .git，不是 git 工作区".to_string());
    }
    Ok(())
}

// ---------- git 基础封装 ----------

fn git_command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir).args(args);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// 运行 git，非零退出视为错误，错误信息取 stderr 最后几行。
fn git_ok(dir: &Path, args: &[&str]) -> Result<Output, String> {
    let output = git_command(dir, args)
        .output()
        .map_err(|e| format!("无法启动 git：{e}。请确认 Git for Windows 已安装并在 PATH 中"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .trim()
            .lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("git {} 失败：{tail}", args.join(" ")));
    }
    Ok(output)
}

/// 只取退出码（0/1 都有语义的命令使用，如 merge-base --is-ancestor）。
fn git_exit_code(dir: &Path, args: &[&str]) -> Result<i32, String> {
    git_command(dir, args)
        .status()
        .map(|status| status.code().unwrap_or(-1))
        .map_err(|e| format!("无法启动 git：{e}。请确认 Git for Windows 已安装并在 PATH 中"))
}

fn git_trimmed(dir: &Path, args: &[&str]) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&git_ok(dir, args)?.stdout).trim().to_string())
}

fn rev_count(dir: &Path, revision_range: &str) -> Result<u32, String> {
    git_trimmed(dir, &["rev-list", "--count", revision_range])?
        .parse()
        .map_err(|_| "无法解析 git 提交计数".to_string())
}

/// upstream 远程缺失则自动添加；已存在但指向别处则报错，不静默改用户配置。
fn ensure_upstream_remote(dir: &Path) -> Result<(), String> {
    match git_trimmed(dir, &["remote", "get-url", UPSTREAM_REMOTE]) {
        Ok(url) if is_alkaid_url(&url) => Ok(()),
        Ok(url) => Err(format!(
            "远程 {UPSTREAM_REMOTE} 指向 {url}，不是 AlkaidLab/foundation-sunshine，请检查后重试"
        )),
        Err(_) => {
            info!("未找到 upstream 远程，自动添加 {UPSTREAM_URL}");
            git_ok(dir, &["remote", "add", UPSTREAM_REMOTE, UPSTREAM_URL])?;
            Ok(())
        }
    }
}

fn is_alkaid_url(url: &str) -> bool {
    let normalized = url.trim().trim_end_matches('/').to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "https://github.com/alkaidlab/foundation-sunshine"
            | "https://github.com/alkaidlab/foundation-sunshine.git"
            | "git@github.com:alkaidlab/foundation-sunshine.git"
    )
}

// ---------- 检查更新（上游提交差） ----------

fn query_status_blocking(dir: &Path) -> Result<SourceStatus, String> {
    ensure_upstream_remote(dir)?;
    git_ok(dir, &["fetch", "--quiet", UPSTREAM_REMOTE, "master"])?;

    let behind = rev_count(dir, "HEAD..upstream/master")?;
    let ahead = rev_count(dir, "upstream/master..HEAD")?;
    let head_hash = git_trimmed(dir, &["log", "-1", "--pretty=%h", "upstream/master"])?;
    let commits_markdown = git_trimmed(
        dir,
        &["log", "-n", "50", "--pretty=- %h %s", "HEAD..upstream/master"],
    )?;

    Ok(SourceStatus {
        behind,
        ahead,
        head_hash,
        commits_markdown,
    })
}

/// 构造源码模式的 `UpdateInfo`；非手动检查且无落后提交时返回 `None`。
pub(super) async fn check_source_update(
    dir: PathBuf,
    manual: bool,
) -> Result<Option<super::UpdateInfo>, String> {
    let source_dir_string = dir.to_string_lossy().to_string();
    let status = tauri::async_runtime::spawn_blocking(move || query_status_blocking(&dir))
        .await
        .map_err(|e| format!("检查任务执行失败: {e}"))??;

    if status.behind == 0 && !manual {
        return Ok(None);
    }

    let mut release_notes = String::new();
    if status.behind > 0 {
        release_notes.push_str(&format!("上游 master 有 **{}** 个新提交：\n\n", status.behind));
        release_notes.push_str(&status.commits_markdown);
    } else {
        release_notes.push_str("本地 my_dev 已与上游 master 同步。");
    }
    if status.ahead > 0 {
        release_notes.push_str(&format!("\n\n本地 my_dev 领先上游 {} 个提交，将通过合并提交保留。", status.ahead));
    }

    Ok(Some(super::UpdateInfo {
        version: status.head_hash.clone(),
        release_notes,
        download_url: None,
        download_name: None,
        download_size: None,
        release_page: "https://github.com/AlkaidLab/foundation-sunshine/commits/master"
            .to_string(),
        is_latest: status.behind == 0,
        kind: "source_build".to_string(),
        behind_count: Some(status.behind),
        ahead_count: Some(status.ahead),
        source_dir: Some(source_dir_string),
    }))
}

// ---------- 同步 + 构建 ----------

fn emit_progress(app: &AppHandle, stage: &str, message: Option<&str>, line: Option<&str>) {
    let _ = app.emit(
        "source-update-progress",
        serde_json::json!({ "stage": stage, "message": message, "line": line }),
    );
}

/// 等效 GitHub Sync fork + 合并进本地 my_dev + 同步子模块。
fn sync_blocking(dir: &Path, app: &AppHandle) -> Result<(), String> {
    let branch = git_trimmed(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch != REQUIRED_BRANCH {
        return Err(format!(
            "同步要求当前分支为 {REQUIRED_BRANCH}，当前在 {branch}。请先手动切换，流程不会自动切分支或暂存改动。"
        ));
    }

    let dirty = git_trimmed(dir, &["status", "--porcelain", "--untracked-files=no"])?;
    if !dirty.is_empty() {
        return Err(
            "my_dev 存在未提交的已跟踪文件改动，请先提交或暂存后再更新（流程不会自动 stash）。"
                .to_string(),
        );
    }

    ensure_upstream_remote(dir)?;

    emit_progress(app, "fetch", Some("正在拉取上游与 fork 的最新提交…"), None);
    git_ok(dir, &["fetch", "--quiet", UPSTREAM_REMOTE, "master"])?;
    git_ok(dir, &["fetch", "--quiet", FORK_REMOTE, "master"])?;

    // fork 的 master 必须是上游的祖先，只允许快进；有分叉就交人工处理。
    match git_exit_code(
        dir,
        &["merge-base", "--is-ancestor", "origin/master", "upstream/master"],
    )? {
        0 => {}
        1 => {
            return Err(
                "fork 的 master 含上游之外的提交，无法快进同步。请先在 GitHub 或本地处理分叉。"
                    .to_string(),
            )
        }
        other => return Err(format!("merge-base 检查异常退出（{other}），已中止")),
    }

    emit_progress(
        app,
        "push",
        Some("正在快进推送到 fork master（等效 Sync fork，使用本机 Git 凭据）…"),
        None,
    );
    let upstream_sha = git_trimmed(dir, &["rev-parse", "upstream/master"])?;
    let refspec = format!("{upstream_sha}:refs/heads/master");
    git_ok(dir, &["push", FORK_REMOTE, &refspec])?;

    emit_progress(app, "merge", Some("正在把上游 master 合并进本地 my_dev…"), None);
    // 本地 my_dev 可能领先（合并提交是预期形态）：先试快进，不行就显式合并。
    if git_exit_code(dir, &["merge", "--ff-only", "upstream/master"])? != 0 {
        if let Err(error) = git_ok(dir, &["merge", "--no-edit", "upstream/master"]) {
            let _ = git_ok(dir, &["merge", "--abort"]);
            return Err(format!("合并上游到 my_dev 失败，已执行 merge --abort，请手工解决冲突：{error}"));
        }
    }

    emit_progress(app, "submodule", Some("正在同步子模块…"), None);
    git_ok(dir, &["submodule", "update", "--init", "--recursive"])?;
    Ok(())
}

#[cfg(windows)]
fn build_blocking(dir: &Path, app: &AppHandle) -> Result<PathBuf, String> {
    let script = dir.join(BUILD_SCRIPT_RELATIVE);
    if !script.is_file() {
        return Err(format!("找不到构建脚本：{}", script.display()));
    }

    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-SourceDir")
        .arg(dir)
        .current_dir(dir)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动 PowerShell：{e}"))?;

    // stdout/stderr 合并到一个有序通道（跨线程到达顺序不保证，但仅用于尾部日志展示）。
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_handle = std::thread::spawn({
        let tx = tx.clone();
        move || {
            BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
                .for_each(|line| {
                    let _ = tx.send(line);
                });
        }
    });
    let stderr_handle = std::thread::spawn(move || {
        BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
            .for_each(|line| {
                let _ = tx.send(line);
            });
    });

    let mut artifact: Option<PathBuf> = None;
    let mut tail: VecDeque<String> = VecDeque::new();
    for line in rx {
        if let Some(stage) = line
            .strip_prefix("@@STAGE:")
            .and_then(|rest| rest.strip_suffix("@@"))
        {
            let message = match stage {
                "configure" => Some("CMake configure（可能需要下载驱动依赖）…"),
                "build" => Some("ninja 增量编译中，耗时取决于改动量…"),
                "package" => Some("cpack 打 NSIS 安装包…"),
                _ => None,
            };
            emit_progress(app, stage, message, None);
        } else if let Some(path) = line
            .strip_prefix("@@ARTIFACT:")
            .and_then(|rest| rest.strip_suffix("@@"))
        {
            artifact = Some(PathBuf::from(path.trim()));
        } else {
            tail.push_back(line.clone());
            if tail.len() > 30 {
                tail.pop_front();
            }
            emit_progress(app, "build", None, Some(&line));
        }
    }
    stdout_handle.join().ok();
    stderr_handle.join().ok();

    let status = child
        .wait()
        .map_err(|e| format!("等待构建进程结束失败：{e}"))?;
    if !status.success() {
        return Err(format!(
            "本地构建失败（退出码 {}）。最后输出：\n{}",
            status.code().unwrap_or(-1),
            tail.iter().cloned().collect::<Vec<_>>().join("\n")
        ));
    }

    match artifact {
        Some(path) if path.is_file() => {
            info!("✅ 本地构建安装包完成: {}", path.display());
            Ok(path)
        }
        Some(path) => Err(format!("构建脚本声称的产物不存在：{}", path.display())),
        None => Err("构建脚本成功结束但未输出 @@ARTIFACT@@ 标记".to_string()),
    }
}

/// 前端“同步并本地构建”入口：跑完同步与构建，返回安装包绝对路径。
/// 安装由前端随后以 NSIS 静默方式调用 `install_update` 完成。
#[tauri::command]
pub async fn start_source_update(app: AppHandle) -> Result<String, String> {
    if SOURCE_UPDATE_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("已有源码更新任务在运行，请勿重复触发".to_string());
    }

    let outcome = async move {
        #[cfg(windows)]
        {
            let dir = effective_source_dir()
                .ok_or_else(|| "未配置有效的 Sunshine 源码目录，请先在设置中选择".to_string())?;
            validate_tree(&dir)?;

            let sync_dir = dir.clone();
            let sync_app = app.clone();
            tauri::async_runtime::spawn_blocking(move || sync_blocking(&sync_dir, &sync_app))
                .await
                .map_err(|e| format!("同步任务执行失败: {e}"))??;

            let artifact = tauri::async_runtime::spawn_blocking(move || build_blocking(&dir, &app))
                .await
                .map_err(|e| format!("构建任务执行失败: {e}"))??;
            Ok(artifact.to_string_lossy().to_string())
        }

        #[cfg(not(windows))]
        {
            let _ = &app;
            Err("本地源码构建仅支持 Windows".to_string())
        }
    }
    .await;

    SOURCE_UPDATE_RUNNING.store(false, Ordering::Release);
    outcome
}
