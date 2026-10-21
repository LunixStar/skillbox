//! engine 层公共契约类型（TASKBOOK §2.2 签名所需的数据结构）。
//!
//! 这些类型同时是 IPC 序列化契约：全部 `#[serde(rename_all = "camelCase")]`（§8）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use skillbox_domain::{DiscoveredSkill, ScanFingerprint, SkillId, SyncMode, TargetState, ToolId};
use skillbox_ports::{Clock, FileSystem, ScanCache};

// ---------------------------------------------------------------------------
// 取消令牌（§7 约束 8）
// ---------------------------------------------------------------------------

/// 协作式取消令牌：扫描引擎每个目录检查一次，取消后 200ms 内
/// 返回已扫到的部分结果。克隆共享同一底层标志。
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// 已取消的令牌（测试用）。
    pub fn cancelled() -> Self {
        let t = Self::new();
        t.cancel();
        t
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// scan（§7）
// ---------------------------------------------------------------------------

/// 单个扫描根（adapters 解析出的各工具 skills 目录 + 中央库根，由调用方组装）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRoot {
    /// 根目录绝对路径。
    pub path: PathBuf,
    /// 该根所属工具；中央库根为 None。
    pub tool_id: Option<ToolId>,
    /// 位于 `extra_scan_dirs`（插件缓存）：只发现登记、不允许部署（§4.3）。
    pub read_only: bool,
}

impl ScanRoot {
    /// 中央库或未知来源的根。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        ScanRoot { path: path.into(), tool_id: None, read_only: false }
    }

    /// 标记为某工具的 skills 根。
    pub fn for_tool(mut self, tool_id: ToolId) -> Self {
        self.tool_id = Some(tool_id);
        self
    }

    /// 标记为只读根（extra_scan_dirs，§4.3）。
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
}

/// 扫描请求。
///
/// 【注意】本类型**不可序列化**：§2.2 签名固定为三参数，注入的端口实现
/// （文件系统 / 时钟 / 缓存）只能随请求传递。IPC 层使用独立 DTO 组装，
/// 禁止把本类型直接暴露给前端。
#[derive(Clone)]
pub struct ScanRequest {
    /// 扫描根（全局 skills 目录 + extra_scan_dirs + 中央库）。
    /// 根不受黑名单 / 深度检查约束（根是显式意图，非启发式对象）。
    pub roots: Vec<ScanRoot>,
    /// 递归深度上限；运行时一律钳制到 ≤ 6（§7 约束 2）。
    pub max_depth: u8,
    /// 用户追加的黑名单目录名（叠加在内置黑名单之上，只能增不能减，
    /// §7 约束 3）。
    pub blacklist: Vec<String>,
    /// 是否启用 `scan_cache` 增量指纹（§7 约束 6）；false = 全量扫描。
    pub incremental: bool,
    /// 注入端口：文件系统（lstat 语义；tmpfs + FakeFs 可测）。
    pub fs: Arc<dyn FileSystem>,
    /// 注入端口：时钟（假时钟下 duration 可确定性断言）。
    pub clock: Arc<dyn Clock>,
    /// 注入端口：扫描缓存；None = 不读不写缓存。
    pub cache: Option<Arc<Mutex<dyn ScanCache>>>,
}

impl std::fmt::Debug for ScanRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 端口句柄无有意义的 Debug，只呈现行为相关字段。
        f.debug_struct("ScanRequest")
            .field("roots", &self.roots)
            .field("max_depth", &self.max_depth)
            .field("blacklist", &self.blacklist)
            .field("incremental", &self.incremental)
            .field("fs", &"<dyn FileSystem>")
            .field("clock", &"<dyn Clock>")
            .field("cache", &self.cache.is_some())
            .finish()
    }
}

impl ScanRequest {
    /// 以生产端口（真实文件系统 / 系统时钟）构造，无缓存、全量扫描。
    pub fn with_prod_ports(roots: Vec<ScanRoot>) -> Self {
        ScanRequest {
            roots,
            max_depth: 6,
            blacklist: Vec::new(),
            incremental: false,
            fs: Arc::new(skillbox_ports::ProdFileSystem),
            clock: Arc::new(skillbox_ports::ProdClock),
            cache: None,
        }
    }
}

/// 流式进度：每处理 50 个目录 emit 一次（§7 约束 9），UI 增量渲染。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// 已处理的目录数。
    pub scanned: u32,
    /// 总数估计；树大小无法预知，v1 恒为 `None`。
    pub total_hint: Option<u32>,
    /// 当前正在处理的目录。
    pub current: PathBuf,
}

/// 目录被跳过的原因（透明记录，供 UI 回答"为什么没扫到"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// 黑名单目录（内置或用户追加，§7 约束 3）。
    Blacklisted,
    /// 超出深度上限（§7 约束 2）。
    TooDeep,
    /// 符号链接，不跟随（§7 约束 4）。
    Symlink,
    /// 网络卷 / 可移动卷 / 系统伪文件系统（§7 约束 5）。
    NonLocalVolume,
}

/// 单个被跳过的目录及其原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDir {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// 扫描报告。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    /// 发现的 skill 目录（未入库的原始磁盘观测，§6.1 磁盘真相）。
    pub skills: Vec<DiscoveredSkill>,
    /// 被跳过的目录及原因（黑名单 / 超深 / 符号链接 / 非本卷）。
    pub skipped: Vec<SkippedDir>,
    /// 非致命告警（目录不可读、SKILL.md 读取失败、缓存降级等）。
    pub warnings: Vec<String>,
    /// 扫描耗时（毫秒，注入时钟测得）。
    pub duration_ms: i64,
}

// ---------------------------------------------------------------------------
// reconcile（§6）
// ---------------------------------------------------------------------------

/// 磁盘现状快照（唯一真相）。由扫描产出，供 reconcile 与磁盘推算。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskView {
    pub entries: Vec<DiskEntry>,
}

/// agent 目录中单个目标路径的磁盘现状。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskEntry {
    pub tool_id: ToolId,
    pub path: PathBuf,
    /// lstat 语义分类；`LinkToSource` = 链接且指向中央库内某正本。
    pub state: TargetState,
    /// 链接目标（`LinkToSource` / `ForeignLink` 时存在）。
    pub link_target: Option<PathBuf>,
    /// 链接是否悬空（目标不存在）。仅链接类条目有意义。
    pub link_broken: bool,
    /// 目标为真实目录时的指纹。
    pub fingerprint: Option<ScanFingerprint>,
    /// 真实目录的递归内容哈希（排除我们的来源标记文件），
    /// CopyDrift 与 `source_hash` 比对的唯一依据。
    pub content_hash: Option<String>,
    /// 链接指向中央库时解析出的 skill id。
    pub source_skill_id: Option<SkillId>,
}

/// DB 索引快照——只代表"上次意图"，不代表磁盘现状（§6.1 硬约束）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexView {
    /// `skills` 表记录。
    pub skills: Vec<SkillRecord>,
    /// `skill_targets` 表记录。
    pub records: Vec<TargetRecord>,
}

/// 中央库单个 skill 的索引行（`skills` 表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub id: SkillId,
    pub name: String,
    /// 中央库唯一正本路径（`~/.skillbox/skills/<name>`）。
    pub canonical_path: PathBuf,
    /// 正本内容哈希（与 `DiskEntry.content_hash` 同算法可比）。
    pub content_hash: Option<String>,
}

/// 单条部署意图记录（`skill_targets` 行）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRecord {
    pub skill_id: SkillId,
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub mode: SyncMode,
    pub source_hash: Option<String>,
}

/// 差异类型（§6.2，v1 共 7 类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscrepancyKind {
    /// agent 目录里有真实目录，非我们部署。
    UnmanagedRealDir,
    /// 链接悬空。
    BrokenLink,
    /// 链接指向中央库之外。
    ForeignLink,
    /// DB 记录已部署，磁盘上没有。
    MissingTarget,
    /// 磁盘上有我们风格的链接，DB 无记录。
    UnrecordedDeployment,
    /// copy 模式副本与正本内容哈希不一致。
    CopyDrift,
    /// 多个 agent 目录同名但内容不同。
    DuplicateName,
}

impl DiscrepancyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DiscrepancyKind::UnmanagedRealDir => "unmanagedRealDir",
            DiscrepancyKind::BrokenLink => "brokenLink",
            DiscrepancyKind::ForeignLink => "foreignLink",
            DiscrepancyKind::MissingTarget => "missingTarget",
            DiscrepancyKind::UnrecordedDeployment => "unrecordedDeployment",
            DiscrepancyKind::CopyDrift => "copyDrift",
            DiscrepancyKind::DuplicateName => "duplicateName",
        }
    }
}

/// 修复建议动作（doctor 展示 / fix 执行；危险动作必须逐个二次确认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestedFix {
    /// 移入中央库并在原位放链接（adopt，需 UserConfirmed）。
    Adopt,
    /// 重建链接。
    RebuildLink,
    /// 移除悬空/外部链接（需确认）。
    RemoveLink,
    /// 重新部署（MissingTarget）。
    Redeploy,
    /// 补登记 DB 记录。
    RegisterRecord,
    /// 从正本更新副本。
    UpdateCopy,
    /// 副本转 symlink。
    ConvertToSymlink,
    /// 由用户挑选正本。
    ChooseCanonical,
    /// 不建议自动处理。
    ManualReview,
}

/// 单条差异（reconcile 的产出，纯计算结果，不写盘）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discrepancy {
    pub kind: DiscrepancyKind,
    pub severity: Severity,
    pub skill_id: Option<SkillId>,
    pub tool_id: Option<ToolId>,
    /// 主路径。
    pub path: PathBuf,
    /// 相关路径（ForeignLink 的目标、DuplicateName 的其余位置等）。
    pub related: Vec<PathBuf>,
    /// 人可读说明。
    pub detail: String,
    pub suggested_fix: SuggestedFix,
}

impl Discrepancy {
    /// 受影响的全部路径（主路径 + 相关路径）：doctor 展示与 fix 影响
    /// 面预览共用。
    pub fn affected_paths(&self) -> Vec<&Path> {
        let mut paths = Vec::with_capacity(self.related.len() + 1);
        paths.push(self.path.as_path());
        paths.extend(self.related.iter().map(|p| p.as_path()));
        paths
    }

    /// 稳定的问题 id（`kind:tool:path`）：同一磁盘现状产生同一 id。
    /// fix 执行时重新推算差异并按 id 匹配——不信任 doctor 快照（K4）。
    pub fn issue_id(&self) -> IssueId {
        let tool = self.tool_id.as_ref().map(|t| t.0.as_str()).unwrap_or("-");
        IssueId(format!("{}:{}:{}", self.kind.as_str(), tool, self.path.display()))
    }
}

// ---------------------------------------------------------------------------
// doctor / fix（§6.3）
// ---------------------------------------------------------------------------

/// 问题严重度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// 体检问题 id（fix 只接受显式选中的 id 列表）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueId(pub String);

/// 单条体检问题。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorIssue {
    pub id: IssueId,
    pub severity: Severity,
    pub discrepancy: Discrepancy,
}

/// 体检报告：doctor() 只读，任何时候可安全运行。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub issues: Vec<DoctorIssue>,
    pub checked_tools: Vec<ToolId>,
    pub duration_ms: i64,
}

/// 单个 fix 项的执行结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FixOutcome {
    /// 已修复。
    Applied,
    /// 跳过（含原因）。
    Skipped { reason: String },
    /// 失败（含原因）。
    Failed { reason: String },
    /// 危险项（涉及 RealDir / ForeignLink）等待用户逐个二次确认
    ///（§6.3 硬约束：禁止一键修复全部作为默认行为）。
    NeedsConfirmation { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub issue_id: IssueId,
    pub outcome: FixOutcome,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixReport {
    pub results: Vec<FixResult>,
}

// ---------------------------------------------------------------------------
// install / deploy（K5：入库与部署分离）
// ---------------------------------------------------------------------------

/// 入库来源（InstallRequest.source）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallSource {
    /// 本地目录（原地登记 keep-in-place 或复制入库）。
    Local { path: PathBuf },
    /// git 仓库（url + 可选子路径/revision）。
    Git { url: String, subpath: Option<String>, revision: Option<String> },
    /// 压缩包（zip/tar + 可选子路径）。
    Archive { path: PathBuf, subpath: Option<String> },
}

/// 入库请求：只写中央库，不碰任何 agent 目录（K5 硬约束）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub source: InstallSource,
    /// 覆盖名称；None = 取 frontmatter `name` 或目录名。
    pub name: Option<String>,
    /// Local 来源专用：正本留在原位（canonical_path = 来源路径），
    /// 不复制入库（首启 keep-in-place 登记同走此开关）。
    #[serde(default)]
    pub keep_in_place: bool,
}

/// 单个工具上的部署结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolDeployOutcome {
    /// 已部署（或原位链接已存在且正确）。
    Deployed { mode: SyncMode },
    /// 目标已是最新。
    AlreadyUpToDate,
    /// 需要用户确认覆盖（ForeignLink / RealDir / RealFile）。
    NeedsConfirmation { reason: String },
    /// 失败。
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDeployResult {
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub outcome: ToolDeployOutcome,
}

/// 部署报告（deploy / undeploy 复用；undeploy 的 outcome 只有
/// Deployed→已移除语义由 fix 报告承载，此处仅 deploy）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployReport {
    pub skill_id: SkillId,
    pub results: Vec<ToolDeployResult>,
}

// ---------------------------------------------------------------------------
// audit / cost（§9.1、§9.2）
// ---------------------------------------------------------------------------

/// 安全风险等级（只提示，不阻断安装）。默认 `Low`（无发现即低风险）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
}

/// 单条安全发现（§9.1 八条静态规则之一命中）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityFinding {
    /// 规则 id：shell_exec / network_egress / credential_access / path_traversal /
    /// obfuscation / symlink_escape / hidden_files / excessive_scope。
    pub rule_id: String,
    pub severity: Severity,
    pub file: PathBuf,
    /// 行号；0 = 不适用。
    pub line: u32,
    /// 命中片段。
    pub snippet: String,
    /// 建议措施。
    pub advice: String,
}

/// 安全扫描报告。纯静态文本匹配，不执行任何 skill 脚本（§9.1 硬约束）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityReport {
    pub risk_level: RiskLevel,
    pub findings: Vec<SecurityFinding>,
}

/// 单个工具的上下文常驻成本（~3.5 字符/token，估算值）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCost {
    pub tool_id: ToolId,
    /// 常驻 skill 数量。
    pub skill_count: u64,
    /// description 总字符数。
    pub description_chars: u64,
    /// 折算 token（估算）。
    pub estimated_tokens: u64,
}

/// 触发冲突组成员（Jaccard ≥ 0.35 的 description 组）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictMember {
    pub skill_id: SkillId,
    pub name: String,
    pub tool_id: ToolId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictGroup {
    pub members: Vec<ConflictMember>,
    /// 重叠关键词。
    pub keywords: Vec<String>,
    /// 两两最大 Jaccard 相似度。
    pub similarity: f64,
}

/// 上下文成本报告（§9.2）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostReport {
    pub per_tool: Vec<ToolCost>,
    pub conflicts: Vec<ConflictGroup>,
}
