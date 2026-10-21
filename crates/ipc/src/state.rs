//! AppState：Tauri managed state（§8 命令的共享上下文）。
//!
//! 命令 = 读 state → 组装 engine 上下文 → 调 engine。端口实例（fs / clock /
//! linker / hasher / git）在 init 时创建一次，此后只读共享。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use skillbox_domain::{AppError, AppResult, SkillId, ToolId};
use skillbox_engine::{
    CancellationToken, HealthContextInputs, IndexView, InstallContext, OpsContext, RiskLevel,
    ScanReport, SecurityReport, SkillRecord, TargetRecord, UpdateContext,
};
use skillbox_ports::{
    Clock, FileSystem, GitClient, Hasher, IndexStore, Linker, ProdClock, ProdFileSystem,
    ProdGitClient, ProdHasher, ProdLinker,
};
use skillbox_store::Store;

/// settings key：被禁用工具 id 的 JSON 数组（`tools_set_enabled` 维护）。
pub(crate) const DISABLED_TOOLS_KEY: &str = "tools.disabled";

/// AppState 组装输入：app 层（组合根）负责创建目录后传入，ipc 保持
/// adapter-free（路径计算在 app / engine，不在 ipc）。
pub struct AppStateInputs {
    /// `skillbox.db` 路径（父目录需已存在）。
    pub db_path: PathBuf,
    /// 中央库 skills 根（`~/.skillbox/skills`）。
    pub central_root: PathBuf,
    pub cache_root: PathBuf,
    pub trash_root: PathBuf,
    pub logs_dir: PathBuf,
}

/// Tauri managed state。
pub struct AppState {
    pub store: Store,
    pub index_store: Arc<dyn IndexStore>,
    pub central_root: PathBuf,
    pub cache_root: PathBuf,
    pub trash_root: PathBuf,
    pub logs_dir: PathBuf,
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub linker: Arc<dyn Linker>,
    pub hasher: Arc<dyn Hasher>,
    pub git: Arc<dyn GitClient>,
    /// 扫描会话（单活动实例）。
    pub scan: Arc<Mutex<ScanSession>>,
    /// 安全扫描结果缓存（skill → 最近一次报告；`security_scan` 写入，
    /// `security_report` 汇总读取）。
    pub security_cache: Arc<Mutex<HashMap<SkillId, SecurityReport>>>,
}

impl AppState {
    /// 组装 state。目录创建由 app 层负责（组合根职责）。
    pub fn init(inputs: AppStateInputs) -> AppResult<AppState> {
        let store = Store::open(&inputs.db_path)?;
        let index_store: Arc<dyn IndexStore> = store.index_store();
        Ok(AppState {
            store,
            index_store,
            central_root: inputs.central_root,
            cache_root: inputs.cache_root,
            trash_root: inputs.trash_root,
            logs_dir: inputs.logs_dir,
            fs: Arc::new(ProdFileSystem),
            clock: Arc::new(ProdClock),
            linker: Arc::new(ProdLinker),
            hasher: Arc::new(ProdHasher),
            git: Arc::new(ProdGitClient),
            scan: Arc::new(Mutex::new(ScanSession::default())),
            security_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// DB 快照 → engine `IndexView`（ipc 读侧转换；store 不依赖 engine）。
    pub fn index_view(&self) -> AppResult<IndexView> {
        let skills = self.store.list_skills()?;
        let records = self.store.list_targets()?;
        Ok(IndexView {
            skills: skills
                .into_iter()
                .map(|m| SkillRecord {
                    id: m.id,
                    name: m.name,
                    canonical_path: m.canonical_path,
                    content_hash: m.content_hash,
                })
                .collect(),
            records: records
                .into_iter()
                .map(|r| TargetRecord {
                    skill_id: r.skill_id,
                    tool_id: r.tool_id,
                    path: r.path,
                    mode: r.mode,
                    source_hash: r.source_hash,
                })
                .collect(),
        })
    }

    /// 入库上下文（`library_install_*` 用）。
    pub fn install_context(&self) -> InstallContext {
        InstallContext {
            central_root: self.central_root.clone(),
            cache_root: self.cache_root.clone(),
            fs: self.fs.clone(),
            clock: self.clock.clone(),
            git: self.git.clone(),
            hasher: self.hasher.clone(),
            linker: self.linker.clone(),
            index: self.index_store.clone(),
            watcher: None,
        }
    }

    /// 编排上下文（deploy / revoke / remove 用）。
    pub fn ops_context(&self) -> OpsContext {
        OpsContext {
            central_root: self.central_root.clone(),
            trash_root: self.trash_root.clone(),
            fs: self.fs.clone(),
            clock: self.clock.clone(),
            linker: self.linker.clone(),
            index: self.index_store.clone(),
            watcher: None,
        }
    }

    /// 更新上下文（`updates_check` / `updates_apply` 用）。
    pub fn update_context(&self) -> UpdateContext {
        UpdateContext {
            central_root: self.central_root.clone(),
            cache_root: self.cache_root.clone(),
            trash_root: self.trash_root.clone(),
            fs: self.fs.clone(),
            clock: self.clock.clone(),
            git: self.git.clone(),
            hasher: self.hasher.clone(),
            linker: self.linker.clone(),
            index: self.index_store.clone(),
            watcher: None,
        }
    }

    /// 体检上下文输入（`doctor_run` / `doctor_fix` 用）。
    pub fn health_inputs(&self) -> AppResult<HealthContextInputs> {
        Ok(HealthContextInputs {
            fs: self.fs.clone(),
            clock: self.clock.clone(),
            linker: self.linker.clone(),
            hasher: self.hasher.clone(),
            central_root: self.central_root.clone(),
            index: self.index_view()?,
            disabled: self.disabled_tools()?,
        })
    }

    /// 被禁用工具（settings `tools.disabled`，JSON id 数组）。
    pub fn disabled_tools(&self) -> AppResult<Vec<ToolId>> {
        match self.store.get_setting(DISABLED_TOOLS_KEY)? {
            None => Ok(Vec::new()),
            Some(raw) => {
                let ids: Vec<String> = serde_json::from_str(&raw).map_err(|e| {
                    AppError::database(format!("settings {DISABLED_TOOLS_KEY} 已损坏: {e}"))
                })?;
                Ok(ids.into_iter().map(ToolId).collect())
            }
        }
    }

    /// 写回被禁用工具。
    pub fn set_disabled_tools(&self, ids: &[ToolId]) -> AppResult<()> {
        let raw = serde_json::to_string(&ids.iter().map(|t| t.0.as_str()).collect::<Vec<_>>())
            .map_err(|e| AppError::internal(format!("序列化失败: {e}")))?;
        self.store.set_setting(DISABLED_TOOLS_KEY, &raw)
    }

    /// 缓存单个 skill 的安全扫描结果。
    pub fn put_security_report(&self, id: SkillId, report: SecurityReport) {
        self.security_cache
            .lock()
            .expect("security_cache poisoned")
            .insert(id, report);
    }

    /// 全库安全概览：合并已扫描 skill 的缓存报告
    /// （risk_level 取最高，findings 合并后按文件 / 行排序）。
    pub fn security_overview(&self) -> SecurityReport {
        let cache = self.security_cache.lock().expect("security_cache poisoned");
        let mut risk_level = RiskLevel::Low;
        let mut findings = Vec::new();
        for report in cache.values() {
            if report.risk_level > risk_level {
                risk_level = report.risk_level;
            }
            findings.extend(report.findings.iter().cloned());
        }
        drop(cache);
        findings.sort_by(|a, b| (&a.rule_id, &a.file, a.line).cmp(&(&b.rule_id, &b.file, b.line)));
        SecurityReport { risk_level, findings }
    }
}

/// 扫描会话（单活动实例；`scan_start` / `scan_cancel` / `scan_status` 共享）。
#[derive(Default)]
pub struct ScanSession {
    pub running: bool,
    pub started_at_ms: Option<i64>,
    pub cancel: Option<CancellationToken>,
    pub report: Option<ScanReport>,
    pub error: Option<String>,
}
