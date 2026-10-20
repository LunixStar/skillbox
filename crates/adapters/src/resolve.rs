//! 路径解析（§4.2）：纯函数层，home / config 根可注入；生产入口用 `dirs`。
//!
//! - `.config/` 前缀自动映射到 `dirs::config_dir()`（Linux XDG）；
//! - 覆写（[`ToolOverrides`]）优先于默认表路径，覆写即视为已安装；
//! - `extra_scan_dirs` 解析结果类型级强制只读（§4.3：只发现登记、不部署）；
//! - 本层零 IO：存在性检查以闭包注入，由 engine 传 `ports::FileSystem`。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use skillbox_domain::{LinkCapability, PlatformMask, ToolId};

use crate::ToolAdapter;

/// `.config/` 前缀：命中则映射到平台配置根（`dirs::config_dir()`）。
const CONFIG_PREFIX: &str = ".config/";

/// 当前用户 home 目录（`dirs::home_dir()`；§4.2 路径解析入口）。
pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// 路径解析作用域：home 与配置根。可注入固定值做确定性测试；
/// 生产用 [`PathScope::current`]（`dirs::home_dir()` / `dirs::config_dir()`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathScope {
    home: PathBuf,
    config: PathBuf,
}

impl PathScope {
    /// 注入式构造（测试用）：home 与 config 根由调用方给定。
    pub fn new(home: PathBuf, config: PathBuf) -> Self {
        Self { home, config }
    }

    /// 生产构造：`dirs::home_dir()` / `dirs::config_dir()`。
    /// config 取不到时回退 `~/.config`（Linux XDG 默认布局）。
    pub fn current() -> Self {
        let home = home();
        let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
        Self { home, config }
    }

    /// home 根。
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// 配置根（Linux XDG 为 `~/.config` 或 `$XDG_CONFIG_HOME`）。
    pub fn config(&self) -> &Path {
        &self.config
    }

    /// 相对路径解析（§4.2）：`.config/` 前缀 → 配置根；其余 → home。
    ///
    /// Linux 默认 XDG 下配置根即 `~/.config`，`.config/opencode/skills`
    /// 解析为 `~/.config/opencode/skills`（与字面路径一致）；
    /// 用户设置 `XDG_CONFIG_HOME` 后自动跟随 `dirs::config_dir()`。
    /// macOS / Windows 下 `.config/` 分别映射到
    /// `~/Library/Application Support` 与 `%APPDATA%`。
    pub fn resolve(&self, rel: &str) -> PathBuf {
        match rel.strip_prefix(CONFIG_PREFIX) {
            Some(rest) => self.config.join(rest),
            None => self.home.join(rel),
        }
    }
}

/// 扫描目录类别（§4.3 类型级强制：`ReadOnly` 只允许发现与登记，禁止部署）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanDirKind {
    /// 全局 skills 目录：可发现、可部署。
    Deployable,
    /// `extra_scan_dirs`（插件缓存等）：只允许发现与登记（read_only）。
    ReadOnly,
}

/// 解析后的扫描目录：路径 + 类别。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedScanDir {
    pub path: PathBuf,
    pub kind: ScanDirKind,
}

impl ResolvedScanDir {
    /// 是否只读（只发现登记、不部署；对应 `DiscoveredSkill.read_only`）。
    pub fn read_only(&self) -> bool {
        matches!(self.kind, ScanDirKind::ReadOnly)
    }
}

/// 用户显式覆写（来自 settings 层；值为绝对路径，绕过默认表解析）。
///
/// 覆写即用户明确使用该工具：[`ToolAdapter::is_installed`] 直接视为 true
///（§4.2"路径被用户显式覆写时视为已安装"）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOverrides {
    global_skills_dirs: HashMap<ToolId, Vec<PathBuf>>,
    detect_paths: HashMap<ToolId, Vec<PathBuf>>,
}

impl ToolOverrides {
    pub fn new() -> Self {
        Self::default()
    }

    /// 覆写某工具的全局 skills 目录（可部署目标）。
    pub fn override_global_skills_dirs(&mut self, tool: ToolId, dirs: Vec<PathBuf>) -> &mut Self {
        self.global_skills_dirs.insert(tool, dirs);
        self
    }

    /// 覆写某工具的探测路径。
    pub fn override_detect_paths(&mut self, tool: ToolId, paths: Vec<PathBuf>) -> &mut Self {
        self.detect_paths.insert(tool, paths);
        self
    }

    /// 某工具被覆写的全局 skills 目录（None = 未覆写，走默认表）。
    pub fn global_skills_dirs(&self, tool: &ToolId) -> Option<&[PathBuf]> {
        self.global_skills_dirs.get(tool).map(Vec::as_slice)
    }

    /// 某工具被覆写的探测路径（None = 未覆写，走默认表）。
    pub fn detect_paths(&self, tool: &ToolId) -> Option<&[PathBuf]> {
        self.detect_paths.get(tool).map(Vec::as_slice)
    }

    /// 该工具是否被显式覆写（任一字段命中）。
    pub fn is_overridden(&self, tool: &ToolId) -> bool {
        self.global_skills_dirs.contains_key(tool) || self.detect_paths.contains_key(tool)
    }
}

impl ToolAdapter {
    fn tool_id(&self) -> ToolId {
        ToolId(self.id.to_string())
    }

    /// 平台可用性（纯函数：可对任意平台掩码断言，测试跨平台过滤用）。
    pub fn available_on(&self, platform: PlatformMask) -> bool {
        self.platforms.contains(platform)
    }

    /// 当前运行平台是否受支持（§4.2 平台掩码过滤）。
    pub fn platform_matches(&self) -> bool {
        self.available_on(PlatformMask::current())
    }

    /// 当前平台链接能力（§5.2 回退链起点）：
    /// Unix → Symlink；Windows → Junction（目录链接无需特权）；
    /// 其余平台 → CopyOnly。
    pub fn link_capability(&self) -> LinkCapability {
        if cfg!(unix) {
            LinkCapability::Symlink
        } else if cfg!(windows) {
            LinkCapability::Junction
        } else {
            LinkCapability::CopyOnly
        }
    }

    /// 解析全局 skills 目录（可发现、可部署）。覆写优先于默认表路径。
    pub fn resolve_global_skills_dirs(
        &self,
        scope: &PathScope,
        overrides: &ToolOverrides,
    ) -> Vec<ResolvedScanDir> {
        let id = self.tool_id();
        if let Some(dirs) = overrides.global_skills_dirs(&id) {
            return dirs
                .iter()
                .map(|path| ResolvedScanDir { path: path.clone(), kind: ScanDirKind::Deployable })
                .collect();
        }
        self.global_skills_dirs
            .iter()
            .map(|rel| ResolvedScanDir { path: scope.resolve(rel), kind: ScanDirKind::Deployable })
            .collect()
    }

    /// 解析 `extra_scan_dirs`（§4.3：类型级强制 `ReadOnly`——
    /// 只允许发现与登记，不允许部署，部署目标不明确）。
    pub fn resolve_extra_scan_dirs(&self, scope: &PathScope) -> Vec<ResolvedScanDir> {
        self.extra_scan_dirs
            .iter()
            .map(|rel| ResolvedScanDir { path: scope.resolve(rel), kind: ScanDirKind::ReadOnly })
            .collect()
    }

    /// 解析探测路径（覆写优先于默认表路径）。
    pub fn resolve_detect_paths(
        &self,
        scope: &PathScope,
        overrides: &ToolOverrides,
    ) -> Vec<PathBuf> {
        let id = self.tool_id();
        if let Some(paths) = overrides.detect_paths(&id) {
            return paths.to_vec();
        }
        self.detect_paths.iter().map(|rel| scope.resolve(rel)).collect()
    }

    /// 项目级 skills 目录：相对**项目根**解析（与 home/config 无关）。
    /// None = 该工具无项目级路径，调用方回退全局目录（§4.1 字段注释）。
    pub fn resolve_project_skills_dir(&self, project_root: &Path) -> Option<PathBuf> {
        self.project_skills_dir.map(|rel| project_root.join(rel))
    }

    /// 是否已安装（§4.2）：探测路径任一存在，或被用户显式覆写。
    /// 存在性检查以闭关注入——本层不做任何 IO
    ///（engine 传 `|p| fs.exists(p)` 形式的 `ports::FileSystem` 包装）。
    pub fn is_installed(
        &self,
        scope: &PathScope,
        overrides: &ToolOverrides,
        exists: impl Fn(&Path) -> bool,
    ) -> bool {
        if overrides.is_overridden(&self.tool_id()) {
            return true;
        }
        self.resolve_detect_paths(scope, overrides).iter().any(|p| exists(p))
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table;
    use skillbox_domain::LinkCapability;

    /// Linux 默认 XDG 情形：config 根 = home/.config（`dirs::config_dir()`
    /// 在未设 `XDG_CONFIG_HOME` 时的值）。
    fn scope_default_xdg() -> PathScope {
        PathScope::new(PathBuf::from("/home/tester"), PathBuf::from("/home/tester/.config"))
    }

    /// `XDG_CONFIG_HOME` 覆写情形：config 根独立于 home。
    fn scope_custom_xdg() -> PathScope {
        PathScope::new(PathBuf::from("/home/tester"), PathBuf::from("/data/xdg"))
    }

    fn no_overrides() -> ToolOverrides {
        ToolOverrides::new()
    }

    // --- 要求 4：`.config/` → XDG 映射（两种情形） ---

    #[test]
    fn config_prefix_maps_to_default_xdg() {
        let scope = scope_default_xdg();
        let opencode = table::find("opencode").unwrap();
        let dirs = opencode.resolve_global_skills_dirs(&scope, &no_overrides());
        assert_eq!(dirs.len(), 1);
        assert_eq!(
            dirs[0].path,
            PathBuf::from("/home/tester").join(".config").join("opencode").join("skills")
        );
        assert!(!dirs[0].read_only());
    }

    #[test]
    fn config_prefix_maps_to_custom_xdg_root() {
        let scope = scope_custom_xdg();
        let opencode = table::find("opencode").unwrap();
        let dirs = opencode.resolve_global_skills_dirs(&scope, &no_overrides());
        assert_eq!(dirs[0].path, PathBuf::from("/data/xdg").join("opencode").join("skills"));
        // Zed 同样走 .config/ 前缀
        let zed = table::find("zed").unwrap();
        let dirs = zed.resolve_global_skills_dirs(&scope, &no_overrides());
        assert_eq!(dirs[0].path, PathBuf::from("/data/xdg").join("zed").join("skills"));
    }

    #[test]
    fn home_relative_paths_resolve_against_home() {
        let scope = scope_default_xdg();
        let claude = table::find("claude_code").unwrap();
        let dirs = claude.resolve_global_skills_dirs(&scope, &no_overrides());
        assert_eq!(dirs[0].path, PathBuf::from("/home/tester").join(".claude").join("skills"));

        // Cline 的 Documents 路径无 .config/ 前缀：三平台均相对 home。
        let cline = table::find("cline").unwrap();
        let dirs = cline.resolve_global_skills_dirs(&scope, &no_overrides());
        assert_eq!(
            dirs[0].path,
            PathBuf::from("/home/tester").join("Documents").join("Cline").join("Rules")
        );
    }

    // --- 要求 4/5：覆写优先于默认路径；覆写即视为已安装 ---

    #[test]
    fn overrides_take_precedence_over_defaults() {
        let scope = scope_default_xdg();
        let claude = table::find("claude_code").unwrap();
        let custom = PathBuf::from("/custom/skills");

        let mut overrides = ToolOverrides::new();
        overrides.override_global_skills_dirs(ToolId("claude_code".into()), vec![custom.clone()]);
        let dirs = claude.resolve_global_skills_dirs(&scope, &overrides);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].path, custom);
        // 未覆写时仍是默认路径
        let dirs = claude.resolve_global_skills_dirs(&scope, &no_overrides());
        assert_eq!(dirs[0].path, PathBuf::from("/home/tester").join(".claude").join("skills"));
    }

    #[test]
    fn overridden_tool_is_installed_even_without_detection() {
        let scope = scope_default_xdg();
        let claude = table::find("claude_code").unwrap();

        // 覆写后：探测路径全部"不存在"也视为已安装（§4.2）
        let mut overrides = ToolOverrides::new();
        overrides.override_detect_paths(ToolId("claude_code".into()), vec![PathBuf::from("/x")]);
        assert!(claude.is_installed(&scope, &overrides, |_| false));
        assert!(overrides.is_overridden(&ToolId("claude_code".into())));

        // 未覆写：依赖探测路径存在性
        assert!(!claude.is_installed(&scope, &no_overrides(), |_| false));
        assert!(claude.is_installed(&scope, &no_overrides(), |p| p.ends_with(".claude.json")));
    }

    #[test]
    fn detect_paths_are_overridable() {
        let scope = scope_default_xdg();
        let claude = table::find("claude_code").unwrap();
        // 默认探测路径解析
        let paths = claude.resolve_detect_paths(&scope, &no_overrides());
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/tester").join(".claude"),
                PathBuf::from("/home/tester").join(".claude.json"),
            ]
        );
        // 覆写后替换默认
        let mut overrides = ToolOverrides::new();
        overrides.override_detect_paths(ToolId("claude_code".into()), vec![PathBuf::from("/d")]);
        assert_eq!(claude.resolve_detect_paths(&scope, &overrides), vec![PathBuf::from("/d")]);
    }

    // --- 要求 5：is_installed 探测语义 ---

    #[test]
    fn is_installed_requires_any_detect_path_to_exist() {
        let scope = scope_default_xdg();
        let trae = table::find("trae").unwrap();
        // Trae 两个候选探测路径，任一存在即已安装
        assert!(!trae.is_installed(&scope, &no_overrides(), |_| false));
        assert!(trae.is_installed(&scope, &no_overrides(), |p| p.ends_with(".trae-cn")));
    }

    // --- 要求 5：平台掩码过滤 ---

    #[test]
    fn platform_mask_filters_unsupported_platforms() {
        // Trae 仅 Win/macOS：Linux 上必须被过滤
        let trae = table::find("trae").unwrap();
        assert!(trae.available_on(PlatformMask::WINDOWS));
        assert!(trae.available_on(PlatformMask::MACOS));
        assert!(!trae.available_on(PlatformMask::LINUX));

        // Amp 仅 macOS/Linux：Windows 上必须被过滤
        let amp = table::find("amp").unwrap();
        assert!(!amp.available_on(PlatformMask::WINDOWS));
        assert!(amp.available_on(PlatformMask::MACOS));

        // Claude Code 三平台全支持
        let claude = table::find("claude_code").unwrap();
        assert!(claude.available_on(PlatformMask::ALL));
    }

    #[test]
    fn platform_filter_applies_to_whole_table() {
        let on_linux: Vec<&str> = table::all()
            .iter()
            .filter(|a| a.available_on(PlatformMask::LINUX))
            .map(|a| a.id)
            .collect();
        assert!(!on_linux.contains(&"trae"), "Trae 无 Linux 版，必须被过滤");

        let on_windows: Vec<&str> = table::all()
            .iter()
            .filter(|a| a.available_on(PlatformMask::WINDOWS))
            .map(|a| a.id)
            .collect();
        assert!(!on_windows.contains(&"amp"), "Amp 无原生 Windows 版，必须被过滤");
        assert!(on_windows.contains(&"claude_code"));
    }

    #[test]
    fn platform_matches_reflects_current_platform() {
        let trae = table::find("trae").unwrap();
        if cfg!(target_os = "linux") {
            assert!(!trae.platform_matches());
        } else {
            assert!(trae.platform_matches());
        }
    }

    // --- 要求 5：link_capability 按平台 ---

    #[test]
    fn link_capability_follows_platform() {
        let claude = table::find("claude_code").unwrap();
        let capability = claude.link_capability();
        if cfg!(windows) {
            assert_eq!(capability, LinkCapability::Junction);
        } else if cfg!(unix) {
            assert_eq!(capability, LinkCapability::Symlink);
        } else {
            assert_eq!(capability, LinkCapability::CopyOnly);
        }
    }

    // --- 要求 6：extra_scan_dirs 类型级只读 ---

    #[test]
    fn extra_scan_dirs_are_read_only_by_type() {
        let scope = scope_default_xdg();
        let cursor = table::find("cursor").unwrap();
        let extras = cursor.resolve_extra_scan_dirs(&scope);
        assert!(!extras.is_empty());
        assert!(extras.iter().all(ResolvedScanDir::read_only), "extra_scan_dirs 必须全部 ReadOnly");

        let claude = table::find("claude_code").unwrap();
        let extras = claude.resolve_extra_scan_dirs(&scope);
        assert!(extras.iter().all(ResolvedScanDir::read_only));

        // 无 extra_scan_dirs 的工具解析为空
        let gemini = table::find("gemini_cli").unwrap();
        assert!(gemini.resolve_extra_scan_dirs(&scope).is_empty());

        // 全局 skills 目录必须全部可部署（与 extra 形成类型对照）
        let dirs = cursor.resolve_global_skills_dirs(&scope, &no_overrides());
        assert!(dirs.iter().all(|d| !d.read_only()));
    }

    #[test]
    fn read_only_scan_dir_serializes_camel_case() {
        let dir = ResolvedScanDir { path: PathBuf::from("/x"), kind: ScanDirKind::ReadOnly };
        let json = serde_json::to_value(&dir).unwrap();
        assert_eq!(json.get("kind"), Some(&serde_json::json!("readOnly")));
        assert_eq!(json.get("path"), Some(&serde_json::json!("/x")));
        let back: ResolvedScanDir = serde_json::from_value(json).unwrap();
        assert!(back.read_only());
    }

    // --- 项目级路径相对项目根 ---

    #[test]
    fn project_dir_resolves_against_project_root() {
        let opencode = table::find("opencode").unwrap();
        let root = Path::new("/work/project");
        assert_eq!(
            opencode.resolve_project_skills_dir(root),
            Some(root.join(".opencode").join("skills"))
        );
        // 无项目级路径的工具返回 None（调用方回退全局）
        let gemini = table::find("gemini_cli").unwrap();
        assert_eq!(gemini.resolve_project_skills_dir(root), None);
    }

    // --- 生产入口冒烟 ---

    #[test]
    fn current_scope_does_not_panic() {
        let scope = PathScope::current();
        assert!(!scope.home().as_os_str().is_empty());
        assert!(!scope.config().as_os_str().is_empty());
    }

    #[test]
    fn traes_two_candidate_dirs_both_resolve() {
        let scope = scope_default_xdg();
        let trae = table::find("trae").unwrap();
        let dirs = trae.resolve_global_skills_dirs(&scope, &no_overrides());
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].path, PathBuf::from("/home/tester").join(".trae").join("skills"));
        assert_eq!(dirs[1].path, PathBuf::from("/home/tester").join(".trae-cn").join("skills"));
    }
}
