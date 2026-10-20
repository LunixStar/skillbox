//! # skillbox-domain
//!
//! 纯数据 + 纯逻辑层：领域类型与错误模型（TASKBOOK §3.3、§2.3）。
//!
//! - 零 IO、零框架；唯一外部依赖 `serde`（序列化派生）。
//! - 不依赖任何本地 crate，是依赖方向的最底层。
//! - 所有跨越 IPC 边界的类型一律 `#[serde(rename_all = "camelCase")]`（§8 硬约束）。

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// engine / ipc / store 统一的结果类型。
pub type AppResult<T> = std::result::Result<T, AppError>;

// ---------------------------------------------------------------------------
// 标识符
// ---------------------------------------------------------------------------

/// 工具（agent）唯一标识，对应 adapter 声明中的 `id`（如 `"claude_code"`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolId(pub String);

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Skill 在中央库中的唯一标识（SQLite `skills.id`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillId(pub i64);

impl fmt::Display for SkillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// 形态与平台（§3.3、§4）
// ---------------------------------------------------------------------------

/// agent 的产品形态。UI 必须按形态分组呈现（§4.2 硬约束）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormFactor {
    /// 命令行 CLI（Claude Code、Codex、Gemini CLI…）
    Cli,
    /// IDE / 桌面端（Cursor、Trae、Zed…）
    Ide,
    /// 编辑器插件（GitHub Copilot、Continue、Cline、Roo Code）
    EditorPlugin,
    /// 终端 TUI（Crush、Amp）
    TerminalTui,
    /// 桌面助手（OpenClaw、Hermes）
    DesktopAssistant,
}

/// 平台支持位掩码（Win/Mac/Linux）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformMask(pub u8);

impl PlatformMask {
    pub const WINDOWS: PlatformMask = PlatformMask(0b001);
    pub const MACOS: PlatformMask = PlatformMask(0b010);
    pub const LINUX: PlatformMask = PlatformMask(0b100);
    pub const ALL: PlatformMask = PlatformMask(0b111);

    /// 是否完整包含 `other` 的所有平台位。
    pub const fn contains(self, other: PlatformMask) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: PlatformMask) -> PlatformMask {
        PlatformMask(self.0 | other.0)
    }

    /// 当前编译目标平台对应的掩码。
    pub fn current() -> PlatformMask {
        if cfg!(target_os = "windows") {
            Self::WINDOWS
        } else if cfg!(target_os = "macos") {
            Self::MACOS
        } else if cfg!(target_os = "linux") {
            Self::LINUX
        } else {
            PlatformMask(0)
        }
    }
}

// ---------------------------------------------------------------------------
// 链接与部署（§5）
// ---------------------------------------------------------------------------

/// 平台链接能力：symlink → junction（Windows）→ tracked copy 的回退链（§5.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkCapability {
    Symlink,
    Junction,
    CopyOnly,
}

/// 部署同步模式（`skill_targets.mode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncMode {
    Symlink,
    Copy,
}

impl SyncMode {
    /// 数据库存储字面值（`skill_targets.mode`）。
    pub fn as_str(self) -> &'static str {
        match self {
            SyncMode::Symlink => "symlink",
            SyncMode::Copy => "copy",
        }
    }

    /// 从数据库存储字面值解析。
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "symlink" => Some(SyncMode::Symlink),
            "copy" => Some(SyncMode::Copy),
            _ => None,
        }
    }
}

/// agent 目录中目标路径的现状分类（§5.1）。
///
/// 判定必须基于 `symlink_metadata()`（lstat 语义）；【硬约束】禁止用
/// `is_dir()`——它会跟随链接，把 junction / 目录 symlink 误判为真实目录，
/// 进而走 `remove_dir_all` 造成用户数据丢失。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetState {
    /// 目标不存在。
    Absent,
    /// 链接且指向本次要部署的源。
    LinkToSource,
    /// 链接但指向别处 / 悬空（用户配置，覆盖需 UserConfirmed）。
    ForeignLink,
    /// 真实目录（与 copy 模式部署无法区分，覆盖需 UserConfirmed）。
    RealDir,
    /// 真实文件。
    RealFile,
}

impl TargetState {
    /// §5.1 决策表：该状态是否属于"不可能损失用户数据"的可覆盖态。
    /// 返回 false 的状态（`ForeignLink` / `RealDir` / `RealFile`）必须走
    /// `ReplacePolicy::UserConfirmed`，由 UI 显式授权。
    pub fn overwritable(&self) -> bool {
        matches!(self, TargetState::Absent | TargetState::LinkToSource)
    }
}

/// 覆盖授权策略（§5.2）。【硬约束】用枚举而非 bool：新增调用点时编译器强制表态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplacePolicy<'a> {
    /// 只允许在"不可能损失用户数据"时写入。
    NoClobber,
    /// DB 中有 `skill_targets` 记录且当前磁盘类型与记录模式相符。
    /// 注意：`Recorded { mode: "symlink" }` 绝不授权删除当前是 `RealDir`
    /// 的目标——DB 记录只代表历史意图，不代表磁盘现状。
    Recorded { mode: &'a str },
    /// 用户显式确认（adopt / 强制覆盖 / 从中央库更新副本）。
    UserConfirmed,
}

// ---------------------------------------------------------------------------
// 来源与技能元数据（§3）
// ---------------------------------------------------------------------------

/// skill 的入库来源类型（`skills.origin_kind`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OriginKind {
    Local,
    Git,
    Archive,
}

impl OriginKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OriginKind::Local => "local",
            OriginKind::Git => "git",
            OriginKind::Archive => "archive",
        }
    }

    /// 从数据库存储字面值解析。
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "local" => Some(OriginKind::Local),
            "git" => Some(OriginKind::Git),
            "archive" => Some(OriginKind::Archive),
            _ => None,
        }
    }
}

/// 中央库中一个 skill 的索引元数据。
/// 【硬约束】`skillbox.db` 禁止存放 skill 正文与 frontmatter 全文——
/// 此处只允许 name / description 摘要（§3.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMeta {
    pub id: SkillId,
    /// frontmatter `name`，需符合 `^[a-z0-9][a-z0-9-]{1,63}$`（§9.3）。
    pub name: String,
    /// frontmatter `description` 摘要。
    pub description: String,
    pub origin_kind: OriginKind,
    /// Git 仓库 URL / 本地路径 / 压缩包路径。
    pub origin_ref: Option<String>,
    /// 仓库内子路径（monorepo 场景）。
    pub origin_subpath: Option<String>,
    /// 来源 revision（git）。
    pub origin_revision: Option<String>,
    /// 中央库唯一正本路径（`~/.skillbox/skills/<name>`），UNIQUE。
    pub canonical_path: PathBuf,
    /// 正本内容哈希（≤1MB 文件精确计算，超限近似并标记，§7 约束 6）。
    pub content_hash: Option<String>,
    /// epoch 毫秒。
    pub created_at: i64,
    /// epoch 毫秒。
    pub updated_at: i64,
}

/// 扫描在磁盘上发现的一个 skill 目录（未入库的原始观测，磁盘真相的载体）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredSkill {
    /// 发现该 skill 的工具；中央库或未知来源时为 None。
    pub tool_id: Option<ToolId>,
    /// skill 目录本身（含 `SKILL.md` 的目录）。
    pub path: PathBuf,
    /// 目录名（入库前的事实名称；入库时以 frontmatter `name` 为准）。
    pub dir_name: String,
    /// frontmatter `name`（读到时）。
    pub name: Option<String>,
    /// frontmatter `description`（读到时）。
    pub description: Option<String>,
    /// 目录指纹，用于增量扫描（§7 约束 6）。
    pub fingerprint: ScanFingerprint,
    /// 位于 `extra_scan_dirs`（插件缓存）：只允许发现与登记，不允许部署（§4.3）。
    pub read_only: bool,
}

/// 目录增量指纹：`(mtime, size, 子项数)` 未变则复用 `scan_cache`（§7 约束 6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFingerprint {
    /// 目录 mtime（epoch 毫秒）。
    pub mtime_ms: i64,
    /// 目录条目元数据累计大小（字节）。
    pub size: u64,
    /// 直接子项数。
    pub child_count: u32,
}

// ---------------------------------------------------------------------------
// 错误模型（§2.3）
// ---------------------------------------------------------------------------

/// 统一错误。任何会删除用户文件的路径，禁止返回模糊的 `Io`：
/// 必须能区分"被拒绝"与"失败"（`details.io_denied`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    pub details: Option<ErrorDetails>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Database,
    Io,
    Network,
    Git,
    NotFound,
    InvalidInput,
    Cancelled,
    Internal,
    /// 目标路径与用户已有数据冲突；必须携带 `details.conflicts`
    /// （`[{path, reason}]`），让 UI 能"指着那个目录说话"。
    TargetConflict,
}

/// 结构化错误细节。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetails {
    /// `TargetConflict` 时必须非空。
    pub conflicts: Vec<PathConflict>,
    /// `Io` 类错误的细分：`Some(true)` = 权限被拒绝（用户可自救），
    /// `Some(false)` = 其他 IO 失败，`None` = 不适用。
    pub io_denied: Option<bool>,
}

/// 单个冲突路径及其原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathConflict {
    pub path: PathBuf,
    pub reason: String,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        AppError { kind, message: message.into(), details: None }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Io, message)
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Database, message)
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Network, message)
    }

    pub fn git(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Git, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Cancelled, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }

    /// 目标冲突：携带逐路径的冲突清单。
    pub fn target_conflict(message: impl Into<String>, conflicts: Vec<PathConflict>) -> Self {
        AppError {
            kind: ErrorKind::TargetConflict,
            message: message.into(),
            details: Some(ErrorDetails { conflicts, io_denied: None }),
        }
    }

    /// 权限被拒绝的 IO 错误（区别于普通失败，§2.3 硬约束）。
    pub fn denied(message: impl Into<String>) -> Self {
        AppError {
            kind: ErrorKind::Io,
            message: message.into(),
            details: Some(ErrorDetails { conflicts: Vec::new(), io_denied: Some(true) }),
        }
    }

    /// 是否为"权限被拒绝"类错误。
    pub fn is_denied(&self) -> bool {
        matches!(&self.details, Some(d) if d.io_denied == Some(true))
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)?;
        if let Some(details) = &self.details {
            if !details.conflicts.is_empty() {
                write!(f, " [conflicts: {}]", details.conflicts.len())?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        // 区分"被拒绝"与"失败"（§2.3 硬约束）。
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            AppError::denied(e.to_string())
        } else {
            AppError::io(e.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_state_overwritable_matches_decision_table() {
        assert!(TargetState::Absent.overwritable());
        assert!(TargetState::LinkToSource.overwritable());
        assert!(!TargetState::ForeignLink.overwritable());
        assert!(!TargetState::RealDir.overwritable());
        assert!(!TargetState::RealFile.overwritable());
    }

    #[test]
    fn platform_mask_bits() {
        assert!(PlatformMask::ALL.contains(PlatformMask::WINDOWS));
        assert!(PlatformMask::ALL.contains(PlatformMask::MACOS.union(PlatformMask::LINUX)));
        assert!(!PlatformMask::WINDOWS.contains(PlatformMask::ALL));
    }

    #[test]
    fn serde_is_camel_case() {
        let state = TargetState::RealDir;
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json, serde_json::json!("realDir"));

        let meta = SkillMeta {
            id: SkillId(7),
            name: "pdf-report".into(),
            description: "生成 PDF 报告".into(),
            origin_kind: OriginKind::Local,
            origin_ref: None,
            origin_subpath: None,
            origin_revision: None,
            canonical_path: PathBuf::from("/home/u/.skillbox/skills/pdf-report"),
            content_hash: None,
            created_at: 1,
            updated_at: 2,
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert!(json.get("originKind").is_some(), "originKind 必须是 camelCase");
        assert!(json.get("canonicalPath").is_some(), "canonicalPath 必须是 camelCase");
        // roundtrip
        let back: SkillMeta = serde_json::from_value(json).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn replace_policy_roundtrip() {
        let policy = ReplacePolicy::Recorded { mode: "symlink" };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("noClobber") || json.contains("recorded") || json.contains("userConfirmed"));
        let back: ReplacePolicy<'_> = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ReplacePolicy::Recorded { mode: "symlink" }));
    }

    #[test]
    fn target_conflict_carries_paths() {
        let err = AppError::target_conflict(
            "目标目录已存在",
            vec![PathConflict { path: PathBuf::from("/x/y"), reason: "真实目录".into() }],
        );
        assert!(matches!(err.kind, ErrorKind::TargetConflict));
        let details = err.details.unwrap();
        assert_eq!(details.conflicts.len(), 1);
        assert_eq!(details.conflicts[0].reason, "真实目录");
    }

    #[test]
    fn denied_io_is_distinguishable() {
        let denied = AppError::denied("拒绝访问");
        assert!(denied.is_denied());
        let plain = AppError::io("盘满了");
        assert!(!plain.is_denied());
    }
}
