//! 安全扫描（§9.1）：对单个 skill 的八条纯静态规则。
//!
//! 【硬约束】本模块绝不执行 skill 内任何脚本。唯一的 IO 端口是
//! `FileSystem`（读目录 / 读文件）与 `Linker::read_link`（读链接目标），
//! 二者均无进程执行语义；引擎也不接受 `GitClient` 等任何具备 spawn
//! 能力的端口入参——依赖图中不存在可被 skill 触达的执行面。
//! 单测 `audit_is_pure_static_zero_writes` 用零写入计数文件系统
//! （`CountingFs`）包裹真实环境，证明含 `bash -c` 的假 skill 被标记
//! 且扫描期间没有任何变更类调用、没有任何子进程 spawn。
//!
//! 八条规则（只提示，不阻断安装）：
//!
//! | rule_id           | severity | 人话含义           | 判定要点 |
//! |-------------------|----------|--------------------|----------|
//! | shell_exec        | Warning  | 执行 shell 命令    | `bash -c` / `sh -c` / `cmd /c` / 管道进 shell 等 |
//! | network_egress    | Info     | 网络访问           | curl/wget/Invoke-WebRequest/requests 等；裸 URL 仅在非 markdown 文件中标记（文档引用 URL 不算） |
//! | credential_access | Critical | 读取凭据           | `.ssh` / `.aws` / `.env`（词边界）/ `process.env` + 敏感词组合等 |
//! | path_traversal    | Warning  | 路径越界           | `../../`、系统目录（/etc、C:\Windows 等），仅非 markdown 文件 |
//! | obfuscation       | Critical | 混淆代码           | base64 解码执行 / eval / 超长十六进制串（≥32 字符） |
//! | symlink_escape    | Critical | 符号链接越界       | 包内链接目标位于 skill 根之外（含无法解析的链接） |
//! | hidden_files      | Info     | 隐藏文件           | 点开头文件 / 目录（不递归进去） |
//! | excessive_scope   | Warning  | 过宽的权限声明     | SKILL.md frontmatter `tools`/`permissions` 等含 `*` 或 all |

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skillbox_domain::AppResult;
use skillbox_ports::{EntryKind, FileSystem, Linker};

use crate::types::{RiskLevel, SecurityFinding, SecurityReport, Severity};

/// 递归深度上限（skill 是小包，与扫描引擎一致取 6）。
const MAX_DEPTH: u32 = 6;
/// 最多扫描的文件数（防御性上限）。
const MAX_FILES: usize = 400;
/// 单文件大小上限（超出只按名称 / 链接规则处理，不读内容）。
const MAX_FILE_BYTES: u64 = 512 * 1024;
/// 命中片段截断长度（字符）。
const MAX_SNIPPET_CHARS: usize = 120;
/// 混淆判定的十六进制串最小长度。
const MIN_HEX_RUN: usize = 32;

/// 审计上下文：只注入读取类端口——这是"无子进程"硬约束的结构性保证。
pub struct AuditContext {
    pub fs: Arc<dyn FileSystem>,
    pub linker: Arc<dyn Linker>,
}

/// 对 skill 根目录做纯静态审计。
///
/// 目录读取失败、文件解码失败一律静默跳过（扫描只提示，不因个别
/// 坏文件失败）。`file` 字段为相对 skill 根的路径；`line` 0 = 不适用。
pub fn audit_skill(ctx: &AuditContext, root: &Path) -> AppResult<SecurityReport> {
    let root_norm = normalize(&ctx.fs.canonicalize(root).unwrap_or_else(|_| root.to_path_buf()));
    let mut state = WalkState::default();
    walk_dir(ctx, root, root, &root_norm, 0, &mut state)?;
    let mut findings = state.findings;
    findings.sort_by(|a, b| (&a.file, a.line, &a.rule_id).cmp(&(&b.file, b.line, &b.rule_id)));
    Ok(SecurityReport { risk_level: risk_level_of(&findings), findings })
}

#[derive(Default)]
struct WalkState {
    findings: Vec<SecurityFinding>,
    files_scanned: usize,
}

impl WalkState {
    fn push(&mut self, rule: &str, severity: Severity, file: PathBuf, line: u32, snippet: &str) {
        self.findings.push(SecurityFinding {
            rule_id: rule.to_string(),
            severity,
            file,
            line,
            snippet: truncate_chars(snippet.trim(), MAX_SNIPPET_CHARS),
            advice: advice(rule).to_string(),
        });
    }
}

fn walk_dir(
    ctx: &AuditContext,
    root: &Path,
    dir: &Path,
    root_norm: &Path,
    depth: u32,
    state: &mut WalkState,
) -> AppResult<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let entries = match ctx.fs.read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()), // 不可读目录静默跳过（只提示，不失败）
    };
    for entry in entries {
        let path = dir.join(&entry.name);
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        match entry.kind {
            EntryKind::Symlink => audit_link(ctx, &path, &rel, root_norm, state),
            EntryKind::Dir => {
                if entry.name.starts_with('.') {
                    state.push("hidden_files", Severity::Info, rel, 0, &entry.name);
                    // 隐藏目录不递归：内部文件不参与内容规则。
                } else if entry.name == "node_modules" {
                    // 依赖目录不递归（内容与 skill 本体无关）。
                } else {
                    walk_dir(ctx, root, &path, root_norm, depth + 1, state)?;
                }
            }
            EntryKind::File => {
                if entry.name.starts_with('.') {
                    state.push("hidden_files", Severity::Info, rel.clone(), 0, &entry.name);
                }
                if state.files_scanned >= MAX_FILES || entry.len > MAX_FILE_BYTES {
                    continue;
                }
                state.files_scanned += 1;
                if let Ok(text) = ctx.fs.read_to_string(&path) {
                    scan_text(&rel, &entry.name, &text, state);
                }
                // 解码失败（二进制 / 非 UTF-8）静默跳过。
            }
            EntryKind::Other => {}
        }
    }
    Ok(())
}

/// `symlink_escape`：链接目标位于 skill 根之外（或无法解析）。
/// Windows 上 junction 与符号链接同样以 `EntryKind::Symlink` 报告，
/// 一律不跟随、只读目标。
fn audit_link(ctx: &AuditContext, path: &Path, rel: &Path, root_norm: &Path, state: &mut WalkState) {
    match ctx.linker.read_link(path) {
        Ok(target) => {
            let resolved = if target.is_absolute() {
                target.clone()
            } else {
                path.parent().map(|p| p.join(&target)).unwrap_or_else(|| target.clone())
            };
            if !normalize(&resolved).starts_with(root_norm) {
                state.push(
                    "symlink_escape",
                    Severity::Critical,
                    rel.to_path_buf(),
                    0,
                    &format!("{} -> {}", rel.display(), target.display()),
                );
            }
        }
        Err(_) => state.push(
            "symlink_escape",
            Severity::Critical,
            rel.to_path_buf(),
            0,
            "无法解析链接目标",
        ),
    }
}

fn scan_text(rel: &Path, name: &str, text: &str, state: &mut WalkState) {
    let lower_name = name.to_lowercase();
    let is_markdown = lower_name.ends_with(".md");
    let is_skill_md = lower_name == "skill.md";
    for (idx, raw) in text.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_lowercase();
        scan_line(rel, line, &lower, is_markdown, line_no, state);
    }
    if is_skill_md {
        scan_frontmatter(rel, text, state);
    }
}

fn scan_line(
    rel: &Path,
    line: &str,
    lower: &str,
    is_markdown: bool,
    line_no: u32,
    state: &mut WalkState,
) {
    // shell_exec（Warning）
    for pat in [
        "bash -c", "sh -c", "zsh -c", "dash -c", "ksh -c", "powershell -command",
        "pwsh -command", "powershell -enc", "cmd /c", "| bash", "|bash", "| sh", "|sh",
        "| zsh", "|zsh", "| python", "|python",
    ] {
        if lower.contains(pat) {
            state.push("shell_exec", Severity::Warning, rel.to_path_buf(), line_no, line);
            break;
        }
    }
    // network_egress（Info）：联网命令在任何文件中标记；
    // 裸 URL 只在非 markdown 文件中标记（文档引用 URL 不算联网行为）。
    for pat in [
        "curl ", "curl\t", "wget ", "wget\t", "invoke-webrequest", "invoke-restmethod",
        "iwr ", "irm ", "requests.post", "requests.get", "requests.put", "requests.delete",
        "urllib.request", "http.client", "net::http", "fetch(",
    ] {
        if lower.contains(pat) {
            state.push("network_egress", Severity::Info, rel.to_path_buf(), line_no, line);
            break;
        }
    }
    if !is_markdown && (lower.contains("http://") || lower.contains("https://") || lower.contains("ftp://")) {
        state.push("network_egress", Severity::Info, rel.to_path_buf(), line_no, line);
    }
    // credential_access（Critical）
    let mut credential_hit = [
        "~/.ssh", "/.ssh/", "id_rsa", "id_ed25519", "~/.aws", ".aws/credentials",
        "aws_access_key_id", "aws_secret_access_key", ".netrc", ".kube/config",
        "keychain", "credentials.json",
    ]
    .iter()
    .any(|pat| lower.contains(pat));
    if !credential_hit && has_env_file_ref(lower) {
        credential_hit = true;
    }
    if !credential_hit
        && lower.contains("process.env")
        && ["token", "secret", "password", "passwd", "credential", "_key", "apikey"]
            .iter()
            .any(|pat| lower.contains(pat))
    {
        credential_hit = true;
    }
    if credential_hit {
        state.push("credential_access", Severity::Critical, rel.to_path_buf(), line_no, line);
    }
    // path_traversal（Warning）：仅非 markdown 文件。
    if !is_markdown {
        for pat in [
            "../../", "..\\..", "/etc/", "/usr/", "/var/", "/bin/", "/sbin/", "/root/",
            "c:\\windows", "c:/windows",
        ] {
            if lower.contains(pat) {
                state.push("path_traversal", Severity::Warning, rel.to_path_buf(), line_no, line);
                break;
            }
        }
    }
    // obfuscation（Critical）
    let mut obfuscation_hit = [
        "base64 -d", "base64 --decode", "base64 -D", "openssl base64", "eval(",
        "exec(eval", "atob(", "frombase64string", "certutil -decode",
    ]
    .iter()
    .any(|pat| lower.contains(pat));
    if !obfuscation_hit && has_long_hex_run(lower, MIN_HEX_RUN) {
        obfuscation_hit = true;
    }
    if obfuscation_hit {
        state.push("obfuscation", Severity::Critical, rel.to_path_buf(), line_no, line);
    }
}

/// `excessive_scope`（Warning）：SKILL.md frontmatter 中
/// `tools` / `permissions` 等键声明 `*` 或 all。
fn scan_frontmatter(rel: &Path, text: &str, state: &mut WalkState) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return;
    }
    for (idx, raw) in lines.iter().enumerate().skip(1) {
        let line = raw.trim_end();
        if line.trim() == "---" {
            break;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim().to_lowercase();
        if !["tools", "allowed-tools", "allowed_tools", "permissions", "scope"].contains(&key.as_str()) {
            continue;
        }
        let value = value.trim().to_lowercase();
        if value.contains('*') || value == "all" || value.starts_with("all ") {
            state.push("excessive_scope", Severity::Warning, rel.to_path_buf(), (idx + 1) as u32, line);
        }
    }
}

/// `.env` 文件引用（词边界：`.environment` 之类的单词不算）。
fn has_env_file_ref(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut from = 0usize;
    while let Some(pos) = lower[from..].find(".env") {
        let at = from + pos;
        let after = at + 4;
        let next_is_alnum = bytes.get(after).is_some_and(|b| b.is_ascii_alphanumeric());
        if !next_is_alnum {
            return true;
        }
        from = at + 4;
    }
    false
}

/// 超长十六进制连续串（≥ `min_len` 字符）——混淆载荷的典型形态。
fn has_long_hex_run(lower: &str, min_len: usize) -> bool {
    let mut run = 0usize;
    for ch in lower.chars() {
        if ch.is_ascii_hexdigit() {
            run += 1;
            if run >= min_len {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn risk_level_of(findings: &[SecurityFinding]) -> RiskLevel {
    if findings.iter().any(|f| f.severity == Severity::Critical) {
        RiskLevel::High
    } else if findings.iter().any(|f| f.severity == Severity::Warning) {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

fn advice(rule: &str) -> &'static str {
    match rule {
        "shell_exec" => "该 Skill 会执行 shell 命令。确认脚本来源可信后再安装，部署前建议人工审阅相关行。",
        "network_egress" => "该 Skill 存在联网行为。确认目标地址是否与你预期一致。",
        "credential_access" => "该 Skill 可能读取凭据（环境变量 / 密钥文件）。安装前务必确认其必要性。",
        "path_traversal" => "该 Skill 引用了 skill 目录之外的路径，注意它可能读写预期之外的位置。",
        "obfuscation" => "存在混淆或动态执行代码（base64 解码 / eval / 长十六进制串），无法静态审阅其真实行为，风险较高。",
        "symlink_escape" => "skill 包内含指向外部的符号链接，可能越权读取包外内容。",
        "hidden_files" => "skill 内存在隐藏文件或目录，请人工确认其用途。",
        "excessive_scope" => "frontmatter 声明了过宽的权限或工具范围，建议收窄到实际所需。",
        _ => "请人工审阅该发现。",
    }
}

/// 去掉 Windows verbatim 前缀（`\\?\`），使 canonicalize 结果与
/// 普通路径可比较。
fn normalize(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
    PathBuf::from(stripped)
}

/// 按字符数截断（中文片段不被字节截半）。
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests;
