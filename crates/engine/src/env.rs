//! 运行环境组装（ipc 的 engine 侧支撑层，TASKBOOK §2.1）。
//!
//! ipc 禁止直接触碰 adapters（§2.1 依赖图）；"有哪些工具、装没装、
//! 目录在哪"这类**环境问题**统一由本模块解答。ipc 只做 DTO 映射与
//! 命令分发，不出现任何路径拼接或安装判定逻辑。
//!
//! 签名约定：`fs` / `scope` / `overrides` 显式入参（可注入 FakeFs +
//! 固定 PathScope 做确定性测试；生产传 `ProdFileSystem` +
//! `PathScope::current()`）。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_adapters::{self as adapters, PathScope, ToolAdapter, ToolOverrides};
use skillbox_domain::{AppError, AppResult, FormFactor, LinkCapability, ToolId};
use skillbox_ports::{Clock, FileSystem, Hasher, Linker};

use crate::doctor::{DoctorTarget, HealthContext};
use crate::types::{IndexView, ScanRoot};

// ---------------------------------------------------------------------------
// 工具清单
// ---------------------------------------------------------------------------

/// 单个工具的运行环境快照（`tools_list` 的原料）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub id: ToolId,
    pub display_name: String,
    pub form_factor: FormFactor,
    /// 探测路径任一存在（或被覆写）即视为已安装（§4.2）。
    pub installed: bool,
    /// 部署目标目录：首个**存在**的全局 skills 目录，否则首个候选（§4.1）。
    /// None = 该工具没有全局 skills 目录（仅项目级）。
    pub deploy_dir: Option<PathBuf>,
    /// 当前平台链接能力（symlink → junction → copy 回退链起点，§5.2）。
    pub link_capability: LinkCapability,
    pub icon: String,
    pub docs_url: String,
    /// 该工具的插件缓存目录（`extra_scan_dirs`，已解析，§4.3）：
    /// 只读来源——其中发现的 skill 只允许登记，不允许部署。
    pub extra_scan_dirs: Vec<PathBuf>,
}

/// 当前平台的工具清单（含安装状态与部署目录）。
///
/// 平台掩码过滤（§4.2）：不支持当前平台的工具不出现。
pub fn tools(fs: &dyn FileSystem, scope: &PathScope, overrides: &ToolOverrides) -> Vec<ToolInfo> {
    adapters::all()
        .iter()
        .filter(|a| a.platform_matches())
        .map(|a| ToolInfo {
            id: ToolId(a.id.to_string()),
            display_name: a.display_name.to_string(),
            form_factor: a.form_factor,
            installed: a.is_installed(scope, overrides, |p| fs.exists(p)),
            deploy_dir: deploy_dir_of(a, fs, scope, overrides),
            link_capability: a.link_capability(),
            icon: a.icon.to_string(),
            docs_url: a.docs_url.to_string(),
            extra_scan_dirs: a
                .resolve_extra_scan_dirs(scope)
                .into_iter()
                .map(|d| d.path)
                .collect(),
        })
        .collect()
}

/// `path` 是否落在任一工具的 `extra_scan_dirs`（插件缓存，§4.3）之下。
///
/// 只读来源判定（deploy 前置守卫用）：这些目录里的 skill 只允许发现与
/// 登记，**不允许部署**（§4.3 硬约束）。按路径组件前缀比较。
pub fn is_read_only_source(path: &Path, scope: &PathScope) -> bool {
    adapters::all()
        .iter()
        .filter(|a| a.platform_matches())
        .any(|a| a.resolve_extra_scan_dirs(scope).iter().any(|d| path.starts_with(&d.path)))
}

/// 按 id 解析部署目录（deploy 命令 / `system_open_path("tool:<id>")` 共用）。
///
/// 未知工具 → `NotFound`；无全局目录的工具 → `InvalidInput`。
pub fn tool_deploy_dir(
    tool_id: &ToolId,
    fs: &dyn FileSystem,
    scope: &PathScope,
    overrides: &ToolOverrides,
) -> AppResult<PathBuf> {
    let adapter = adapters::find(&tool_id.0)
        .ok_or_else(|| AppError::not_found(format!("未知工具: {}", tool_id.0)))?;
    deploy_dir_of(adapter, fs, scope, overrides).ok_or_else(|| {
        AppError::invalid_input(format!("工具 {} 没有全局 skills 目录，无法部署", tool_id.0))
    })
}

/// 工具部署目录：首个存在的全局 skills 目录；全部不存在时取首个候选
///（部署会 `create_dir_all` 目标父目录，候选即合法落点）。
fn deploy_dir_of(
    adapter: &ToolAdapter,
    fs: &dyn FileSystem,
    scope: &PathScope,
    overrides: &ToolOverrides,
) -> Option<PathBuf> {
    let dirs = adapter.resolve_global_skills_dirs(scope, overrides);
    dirs.iter()
        .find(|d| fs.exists(&d.path))
        .map(|d| d.path.clone())
        .or_else(|| dirs.first().map(|d| d.path.clone()))
}

// ---------------------------------------------------------------------------
// 扫描根 / 体检目标
// ---------------------------------------------------------------------------

/// 扫描根组装（scan 命令的原料）：
/// - 各工具全局 skills 目录（可部署）与 `extra_scan_dirs`（只读，§4.3）；
/// - 中央库根（tool_id = None）。
///
/// 只收**存在**的目录：根是显式意图（§7），不存在的根只会制造告警噪音。
/// 目录存在本身就代表该工具被使用过，与安装探测结果无关。
///
/// `disabled`：被用户禁用的工具（`tools_set_enabled`）不参与扫描。
pub fn scan_roots(
    fs: &dyn FileSystem,
    scope: &PathScope,
    overrides: &ToolOverrides,
    disabled: &[ToolId],
    central_root: &Path,
) -> Vec<ScanRoot> {
    let disabled: HashSet<&str> = disabled.iter().map(|t| t.0.as_str()).collect();
    let mut roots = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let push = |root: ScanRoot, roots: &mut Vec<ScanRoot>, seen: &mut HashSet<PathBuf>| {
        // 多工具共享同一目录时只保留首个（首个获胜，其余忽略）。
        if seen.insert(root.path.clone()) {
            roots.push(root);
        }
    };
    for adapter in adapters::all().iter().filter(|a| a.platform_matches()) {
        if disabled.contains(adapter.id) {
            continue;
        }
        let tool_id = ToolId(adapter.id.to_string());
        for dir in adapter.resolve_global_skills_dirs(scope, overrides) {
            if fs.exists(&dir.path) {
                push(ScanRoot::new(&dir.path).for_tool(tool_id.clone()), &mut roots, &mut seen);
            }
        }
        for dir in adapter.resolve_extra_scan_dirs(scope) {
            if fs.exists(&dir.path) {
                push(
                    ScanRoot::new(&dir.path).for_tool(tool_id.clone()).read_only(),
                    &mut roots,
                    &mut seen,
                );
            }
        }
    }
    if fs.exists(central_root) {
        push(ScanRoot::new(central_root), &mut roots, &mut seen);
    }
    roots
}

/// 体检目标（doctor 命令的原料）：**已安装**工具的全部全局 skills 根。
///
/// - 含不存在的候选目录：`inspect_disk` 自行跳过，指向该处的残留记录由
///   reconcile 报 `MissingTarget`（§6.1 磁盘真相优先）。
/// - 只看已安装工具：已卸载工具的记录不检查，避免误报
///   `MissingTarget`（用户卸载工具 ≠ 部署损坏）。
/// - `disabled`：被用户禁用的工具不参与体检。
pub fn doctor_targets(
    fs: &dyn FileSystem,
    scope: &PathScope,
    overrides: &ToolOverrides,
    disabled: &[ToolId],
) -> Vec<DoctorTarget> {
    let disabled: HashSet<&str> = disabled.iter().map(|t| t.0.as_str()).collect();
    adapters::all()
        .iter()
        .filter(|a| a.platform_matches())
        .filter(|a| !disabled.contains(a.id))
        .filter(|a| a.is_installed(scope, overrides, |p| fs.exists(p)))
        .flat_map(|a| {
            let tool_id = ToolId(a.id.to_string());
            a.resolve_global_skills_dirs(scope, overrides)
                .into_iter()
                .map(move |d| DoctorTarget { tool_id: tool_id.clone(), path: d.path })
        })
        .collect()
}

/// 体检上下文组装输入（`doctor_run` 的原料）：端口 + 目录 + 索引快照。
pub struct HealthContextInputs {
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub linker: Arc<dyn Linker>,
    pub hasher: Arc<dyn Hasher>,
    pub central_root: PathBuf,
    pub index: IndexView,
    /// 被用户禁用的工具（`tools_set_enabled`）：不参与体检。
    pub disabled: Vec<ToolId>,
}

/// 体检上下文组装：目标来自已安装工具的解析结果。
pub fn health_context(
    inputs: HealthContextInputs,
    scope: &PathScope,
    overrides: &ToolOverrides,
) -> HealthContext {
    HealthContext {
        targets: doctor_targets(inputs.fs.as_ref(), scope, overrides, &inputs.disabled),
        central_root: inputs.central_root,
        index: inputs.index,
        fs: inputs.fs,
        clock: inputs.clock,
        linker: inputs.linker,
        hasher: inputs.hasher,
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use skillbox_domain::PlatformMask;
    use skillbox_ports::ProdFileSystem;

    /// tempdir 作为 home / config 根（路径解析全部落在其中）。
    fn scope() -> (tempfile::TempDir, PathScope) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let scope = PathScope::new(home.clone(), home);
        (dir, scope)
    }

    fn find_tool<'a>(infos: &'a [ToolInfo], id: &str) -> &'a ToolInfo {
        infos.iter().find(|t| t.id.0 == id).unwrap_or_else(|| panic!("缺少工具 {id}"))
    }

    #[test]
    fn tools_report_installed_and_deploy_dir() {
        let (_dir, scope) = scope();
        let fs = ProdFileSystem;
        let home = scope.home().to_path_buf();

        // 未创建探测路径：claude 未安装，部署目录回退首个候选。
        let infos = tools(&fs, &scope, &ToolOverrides::new());
        let claude = find_tool(&infos, "claude_code");
        assert!(!claude.installed);
        assert_eq!(claude.deploy_dir.as_deref(), Some(home.join(".claude").join("skills").as_path()));
        assert_eq!(claude.form_factor, FormFactor::Cli);
        assert!(!claude.icon.is_empty());
        assert!(!claude.docs_url.is_empty());

        // 创建探测文件后：已安装。
        std::fs::write(home.join(".claude.json"), "{}").unwrap();
        let infos = tools(&fs, &scope, &ToolOverrides::new());
        assert!(find_tool(&infos, "claude_code").installed);
    }

    #[test]
    fn tools_filter_current_platform() {
        let (_dir, scope) = scope();
        let infos = tools(&ProdFileSystem, &scope, &ToolOverrides::new());
        // amp 仅 macOS/Linux：是否出现必须与平台掩码判定一致。
        let amp_expected = adapters::find("amp").unwrap().platform_matches();
        assert_eq!(infos.iter().any(|t| t.id.0 == "amp"), amp_expected);
        // 三平台工具永远在列。
        assert!(infos.iter().any(|t| t.id.0 == "claude_code"));
        assert!(infos
            .iter()
            .all(|t| adapters::find(&t.id.0)
                .unwrap()
                .platforms
                .contains(PlatformMask::current())));
    }

    #[test]
    fn deploy_dir_prefers_existing_candidate() {
        let (_dir, scope) = scope();
        let fs = ProdFileSystem;
        // trae 有两个候选目录（.trae / .trae-cn）；只在 trae 支持当前平台时测试。
        if !adapters::find("trae").unwrap().platform_matches() {
            return;
        }
        let second = scope.home().join(".trae-cn").join("skills");
        std::fs::create_dir_all(&second).unwrap();
        let dir = tool_deploy_dir(&ToolId("trae".into()), &fs, &scope, &ToolOverrides::new());
        assert_eq!(dir.unwrap(), second);
    }

    #[test]
    fn scan_roots_collect_existing_dirs_only() {
        let (dir, scope) = scope();
        let fs = ProdFileSystem;
        let home = scope.home().to_path_buf();

        let claude_skills = home.join(".claude").join("skills");
        std::fs::create_dir_all(&claude_skills).unwrap();
        let central = dir.path().join("central");
        std::fs::create_dir_all(&central).unwrap();

        let roots = scan_roots(&fs, &scope, &ToolOverrides::new(), &[], &central);
        // claude 全局目录在列，归属 claude_code。
        let claude_root = roots
            .iter()
            .find(|r| r.path == claude_skills)
            .expect("已存在的 claude skills 目录必须是扫描根");
        assert_eq!(claude_root.tool_id.as_ref().map(|t| t.0.as_str()), Some("claude_code"));
        assert!(!claude_root.read_only);
        // 中央库根在列，无归属。
        assert!(roots.iter().any(|r| r.path == central && r.tool_id.is_none()));
        // 未创建的目录（其余全部工具）不出现。
        assert!(roots.iter().all(|r| fs.exists(&r.path)));
    }

    #[test]
    fn scan_roots_exclude_disabled_tools() {
        let (_dir, scope) = scope();
        let fs = ProdFileSystem;
        let home = scope.home().to_path_buf();
        let claude_skills = home.join(".claude").join("skills");
        std::fs::create_dir_all(&claude_skills).unwrap();

        let disabled = [ToolId("claude_code".into())];
        let roots = scan_roots(&fs, &scope, &ToolOverrides::new(), &disabled, Path::new("/nonexistent"));
        assert!(
            roots.iter().all(|r| r.path != claude_skills),
            "被禁用工具的目录不参与扫描: {roots:?}"
        );
    }

    #[test]
    fn scan_roots_mark_extra_dirs_read_only() {
        let (_dir, scope) = scope();
        let fs = ProdFileSystem;
        // 选一个带 extra_scan_dirs 且支持当前平台的工具，把 extra 目录建出来。
        let adapter = adapters::all()
            .iter()
            .find(|a| a.platform_matches() && !a.extra_scan_dirs.is_empty())
            .expect("当前平台必有带 extra_scan_dirs 的工具");
        let extra = scope.resolve(adapter.extra_scan_dirs[0]);
        std::fs::create_dir_all(&extra).unwrap();

        let roots = scan_roots(&fs, &scope, &ToolOverrides::new(), &[], Path::new("/nonexistent"));
        let root = roots.iter().find(|r| r.path == extra).expect("extra 目录必须是扫描根");
        assert!(root.read_only, "extra_scan_dirs 必须只读（§4.3）");
        assert_eq!(root.tool_id.as_ref().map(|t| t.0.as_str()), Some(adapter.id));
    }

    #[test]
    fn doctor_targets_cover_installed_tools_only() {
        let (_dir, scope) = scope();
        let fs = ProdFileSystem;
        let home = scope.home().to_path_buf();

        // 未安装任何工具：无体检目标。
        assert!(doctor_targets(&fs, &scope, &ToolOverrides::new(), &[]).is_empty());

        // 安装 claude：其全部全局目录成为目标（含不存在的候选路径）。
        std::fs::write(home.join(".claude.json"), "{}").unwrap();
        let targets = doctor_targets(&fs, &scope, &ToolOverrides::new(), &[]);
        assert!(!targets.is_empty());
        assert!(targets.iter().all(|t| t.tool_id.0 == "claude_code"));
        assert!(targets.iter().any(|t| t.path == home.join(".claude").join("skills")));

        // 禁用 claude：目标清空（禁用即不参与体检）。
        let disabled = [ToolId("claude_code".into())];
        assert!(doctor_targets(&fs, &scope, &ToolOverrides::new(), &disabled).is_empty());
    }

    #[test]
    fn tools_expose_extra_scan_dirs() {
        let (_dir, scope) = scope();
        let infos = tools(&ProdFileSystem, &scope, &ToolOverrides::new());
        // 带 extra_scan_dirs 且支持当前平台的工具必须暴露解析后的目录。
        let with_extra: Vec<_> = adapters::all()
            .iter()
            .filter(|a| a.platform_matches() && !a.extra_scan_dirs.is_empty())
            .collect();
        for adapter in with_extra {
            let info = find_tool(&infos, adapter.id);
            assert!(
                !info.extra_scan_dirs.is_empty(),
                "{} 应暴露 extra_scan_dirs",
                adapter.id
            );
        }
    }

    #[test]
    fn is_read_only_source_matches_extra_dirs_only() {
        let (dir, scope) = scope();
        // 选一个带 extra_scan_dirs 且支持当前平台的工具。
        let adapter = adapters::all()
            .iter()
            .find(|a| a.platform_matches() && !a.extra_scan_dirs.is_empty())
            .expect("当前平台必有带 extra_scan_dirs 的工具");
        let extra = scope.resolve(adapter.extra_scan_dirs[0]);
        let inside = extra.join("some-skill");
        assert!(
            is_read_only_source(&inside, &scope),
            "extra_scan_dirs 内的路径必须判定为只读来源"
        );
        // 中央库与无关路径不是只读来源。
        let central = dir.path().join("central").join("some-skill");
        assert!(!is_read_only_source(&central, &scope));
    }

    #[test]
    fn tool_deploy_dir_rejects_unknown_tool() {
        let (_dir, scope) = scope();
        let err =
            tool_deploy_dir(&ToolId("no_such_tool".into()), &ProdFileSystem, &scope, &ToolOverrides::new())
                .unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);
    }

    #[test]
    fn health_context_assembles_targets() {
        let (_dir, scope) = scope();
        let fs: Arc<dyn FileSystem> = Arc::new(ProdFileSystem);
        std::fs::write(scope.home().join(".claude.json"), "{}").unwrap();
        let ctx = health_context(
            HealthContextInputs {
                fs,
                clock: Arc::new(skillbox_ports::ProdClock),
                linker: Arc::new(skillbox_ports::ProdLinker),
                hasher: Arc::new(skillbox_ports::ProdHasher),
                central_root: PathBuf::from("/central"),
                index: IndexView::default(),
                disabled: vec![ToolId("claude_code".into())],
            },
            &scope,
            &ToolOverrides::new(),
        );
        assert_eq!(ctx.central_root, PathBuf::from("/central"));
        assert!(
            ctx.targets.is_empty(),
            "禁用工具不参与体检: {:?}",
            ctx.targets
        );
    }
}
