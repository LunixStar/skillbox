//! # skillbox-adapters
//!
//! agent 声明表 + 路径解析（TASKBOOK §4）。本层是**纯函数**：
//! 输入 home / config_dir，输出候选路径；不做任何文件系统访问——
//! [`ToolAdapter::is_installed`] 的存在性检查以闭包注入，
//! 由 engine 传 `ports::FileSystem` 包装。
//!
//! 模块划分（单文件 ≤ 600 行红线）：
//! - [`table`]：§4.2 全部内置工具声明，按形态分组排列；
//! - [`resolve`]：路径解析（`.config/` → XDG 映射、覆写、项目级、只读目录）。
//!
//! 【硬约束】本 crate 禁止引用 tauri 任何命名空间。

mod resolve;
mod table;

pub use resolve::{home, PathScope, ResolvedScanDir, ScanDirKind, ToolOverrides};
pub use table::{all, find, grouped_by_form_factor, ALL};

use skillbox_domain::{FormFactor, PlatformMask};

/// 单个 agent 工具的声明（§4.1）。
///
/// 所有字段为静态数据；路径支持 `.config/` 前缀自动映射到
/// `dirs::config_dir()`（Linux XDG）——解析规则见 [`PathScope::resolve`]。
#[derive(Debug, Clone)]
pub struct ToolAdapter {
    /// 稳定标识（如 `"claude_code"`），即 `ToolId` 的来源。
    pub id: &'static str,
    /// UI 展示名（如 `"Claude Code"`）。
    pub display_name: &'static str,
    /// 产品形态；UI 侧栏与工具页必须按形态分组（§4.2 硬约束）。
    pub form_factor: FormFactor,
    /// 支持平台位掩码。
    pub platforms: PlatformMask,
    /// 全局 skills 目录（相对 home，可多个候选；支持 `.config/` XDG 映射）。
    pub global_skills_dirs: &'static [&'static str],
    /// 存在即认为"已安装"的探测路径。
    pub detect_paths: &'static [&'static str],
    /// 项目级 skills 目录；None 则回退全局相对路径。
    pub project_skills_dir: Option<&'static str>,
    /// 只发现、不部署的目录（插件缓存等，§4.3）。
    pub extra_scan_dirs: &'static [&'static str],
    /// 是否递归扫描嵌套分类目录（如 Hermes）。
    pub recursive_scan: bool,
    /// 内置 SVG 图标 id。
    pub icon: &'static str,
    /// 官方文档链接（UI 溯源跳转）。
    pub docs_url: &'static str,
}
