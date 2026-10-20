//! §4.2 内置工具声明表。
//!
//! 排列顺序即 §4.2 的形态分组顺序：CLI → IDE/桌面端 → 编辑器插件 →
//! 终端 TUI → 桌面助手。[`grouped_by_form_factor`] 依赖此顺序做连续分组。
//!
//! 数目说明：TASKBOOK 正文写"16 个"，但 §4.2 表格实际列举 19 个
//!（CLI 6 / IDE·桌面端 5 / 编辑器插件 4 / 终端 TUI 2 / 桌面助手 2）。
//! 按任务指示"以 §4.2 表格为准"，本表实现 19 个。
//!
//! 每个声明上方的注释三段式（§4.2 硬约束）：路径来源依据 /
//! 全局与项目级是否不对称 / 平台差异。标注「待复核」的路径需在交付
//! `docs/adapters.md` 前经官方来源核实。

use skillbox_domain::FormFactor;

use crate::ToolAdapter;

// ---------------------------------------------------------------------------
// CLI（6）
// ---------------------------------------------------------------------------

/// 来源：code.claude.com/docs/en/skills（Anthropic Agent Skills 官方文档）。
/// 不对称：全局 `~/.claude/skills` 与项目级 `.claude/skills` 同名，对称。
/// 平台：三平台路径一致（home 相对）。`~/.claude/plugins` 为插件目录
///（2025 插件体系，插件可内含 skills），按 §4.3 只发现登记、不部署。
pub const CLAUDE_CODE: ToolAdapter = ToolAdapter {
    id: "claude_code",
    display_name: "Claude Code",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".claude/skills"],
    detect_paths: &[".claude", ".claude.json"],
    project_skills_dir: Some(".claude/skills"),
    extra_scan_dirs: &[".claude/plugins"],
    recursive_scan: false,
    icon: "claude-code",
    docs_url: "https://code.claude.com/docs/en/skills",
};

/// 来源：github.com/openai/codex（配置根 `~/.codex` 确定；TASKBOOK §4.2
/// 指定 skills 目录 `~/.codex/skills`）。项目级 `.codex/skills` 对称存在。
/// 平台：三平台路径一致。skills 子目录细节待 docs/adapters.md 复核。
pub const CODEX: ToolAdapter = ToolAdapter {
    id: "codex",
    display_name: "Codex",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".codex/skills"],
    detect_paths: &[".codex"],
    project_skills_dir: Some(".codex/skills"),
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "codex",
    docs_url: "https://developers.openai.com/codex/",
};

/// 来源：github.com/google-gemini/gemini-cli（配置根 `~/.gemini` 确定：
/// GEMINI.md、extensions 等）；skills 目录 `~/.gemini/skills` 依据
/// TASKBOOK §4.2。项目级：无独立路径（None，回退全局）。
/// 平台：三平台路径一致。skills 子目录细节待复核。
pub const GEMINI_CLI: ToolAdapter = ToolAdapter {
    id: "gemini_cli",
    display_name: "Gemini CLI",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".gemini/skills"],
    detect_paths: &[".gemini"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "gemini-cli",
    docs_url: "https://github.com/google-gemini/gemini-cli",
};

/// 来源：opencode.ai/docs + github.com/sst/opencode（配置根
/// `~/.config/opencode`，遵循 XDG）。
/// 【不对称】全局 `~/.config/opencode/skills`（经 `.config/` 前缀映射到
/// `dirs::config_dir()`）与项目级 `.opencode/skills`（相对项目根）不同名
/// ——TASKBOOK §4.2 明示的"全局与项目级路径不对称"案例。
/// 平台：`.config/` 在 macOS 映射到 `~/Library/Application Support`、
/// Windows 映射到 `%APPDATA%`；opencode 各平台均遵循 XDG 语义。
pub const OPENCODE: ToolAdapter = ToolAdapter {
    id: "opencode",
    display_name: "OpenCode",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".config/opencode/skills"],
    detect_paths: &[".config/opencode"],
    project_skills_dir: Some(".opencode/skills"),
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "opencode",
    docs_url: "https://opencode.ai/docs/",
};

/// 来源：github.com/QwenLM/qwen-code（Gemini CLI fork；配置根 `~/.qwen`）。
/// 项目级：无独立路径（None，回退全局）。
/// 平台：三平台路径一致。skills 子目录细节待复核。
pub const QWEN_CODE: ToolAdapter = ToolAdapter {
    id: "qwen_code",
    display_name: "Qwen Code",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".qwen/skills"],
    detect_paths: &[".qwen"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "qwen-code",
    docs_url: "https://github.com/QwenLM/qwen-code",
};

/// 来源：Grok 生态 CLI（如 github.com/superagent-ai/grok-cli；配置根
/// `~/.grok` 为 v1 约定候选，【待复核】）。项目级：None，回退全局。
/// 平台：三平台路径一致。
pub const GROK: ToolAdapter = ToolAdapter {
    id: "grok",
    display_name: "Grok",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".grok/skills"],
    detect_paths: &[".grok"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "grok",
    docs_url: "https://github.com/superagent-ai/grok-cli",
};

// ---------------------------------------------------------------------------
// IDE / 桌面端（5）
// ---------------------------------------------------------------------------

/// 来源：cursor.com/docs（Agent Skills：全局 `~/.cursor/skills`、
/// 项目级 `.cursor/skills`，对称）。
/// 平台：三平台路径一致。`~/.cursor/extensions` 为扩展目录（VSCode 系
/// 约定，扩展可能内嵌 skills），按 §4.3 只发现登记、不部署。
pub const CURSOR: ToolAdapter = ToolAdapter {
    id: "cursor",
    display_name: "Cursor",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".cursor/skills"],
    detect_paths: &[".cursor"],
    project_skills_dir: Some(".cursor/skills"),
    extra_scan_dirs: &[".cursor/extensions"],
    recursive_scan: false,
    icon: "cursor",
    docs_url: "https://cursor.com/docs",
};

/// 来源：docs.windsurf.com（Windsurf/Codeium：配置根
/// `~/.codeium/windsurf` 确定，memories 等同根；skills 子目录
/// `.codeium/windsurf/skills`【待复核】）。项目级：None，回退全局。
/// 平台：三平台路径一致。
pub const WINDSURF: ToolAdapter = ToolAdapter {
    id: "windsurf",
    display_name: "Windsurf",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".codeium/windsurf/skills"],
    detect_paths: &[".codeium/windsurf"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "windsurf",
    docs_url: "https://docs.windsurf.com/",
};

/// 来源：docs.trae.ai / docs.trae.com.cn（字节 Trae）。国际版配置根
/// `~/.trae`、国内版 `~/.trae-cn`——两个独立安装的同一 IDE，共用一条
/// 声明、两个候选目录。项目级：None，回退全局。
/// 平台：**仅 Windows / macOS**（无官方 Linux 版），用于平台掩码过滤。
/// skills 子目录细节【待复核】。
pub const TRAE: ToolAdapter = ToolAdapter {
    id: "trae",
    display_name: "Trae / Trae CN",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::WINDOWS.union(skillbox_domain::PlatformMask::MACOS),
    global_skills_dirs: &[".trae/skills", ".trae-cn/skills"],
    detect_paths: &[".trae", ".trae-cn"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "trae",
    docs_url: "https://www.trae.ai/",
};

/// 来源：antigravity.google（Google Antigravity，VSCode 系 fork）。
/// 配置根 `~/.antigravity` 与 skills 子目录均为 v1 约定候选【待复核】。
/// 项目级：None，回退全局。平台：三平台。
pub const ANTIGRAVITY: ToolAdapter = ToolAdapter {
    id: "antigravity",
    display_name: "Antigravity",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".antigravity/skills"],
    detect_paths: &[".antigravity"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "antigravity",
    docs_url: "https://antigravity.google/",
};

/// 来源：zed.dev/docs + TASKBOOK §4.2（`~/.config/zed/...`）。
/// 平台差异：Linux `~/.config/zed`；`.config/` 前缀经
/// `dirs::config_dir()` 映射——macOS `~/Library/Application Support/zed`、
/// Windows `%APPDATA%\zed`（官方目录大小写为 "Zed"，【待复核】后修正）。
/// 项目级：None，回退全局。
pub const ZED: ToolAdapter = ToolAdapter {
    id: "zed",
    display_name: "Zed",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".config/zed/skills"],
    detect_paths: &[".config/zed"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "zed",
    docs_url: "https://zed.dev/docs/",
};

// ---------------------------------------------------------------------------
// 编辑器插件（4）
// ---------------------------------------------------------------------------

/// 来源：docs.github.com/en/copilot + TASKBOOK §4.2（全局指令目录
/// `~/.github/instructions`）。项目级为仓库内 `.github/copilot-instructions.md`
///（文件而非目录，v1 不作项目级扫描，None）。
/// 平台：三平台路径一致。`~/.github` 存在性是弱探测信号（用户可覆写）。
pub const GITHUB_COPILOT: ToolAdapter = ToolAdapter {
    id: "github_copilot",
    display_name: "GitHub Copilot",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".github/instructions"],
    detect_paths: &[".github"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "github-copilot",
    docs_url: "https://docs.github.com/en/copilot",
};

/// 来源：docs.continue.dev（github.com/continuedev/continue；配置根
/// `~/.continue` 确定：config.yaml 等；规则目录 `~/.continue/rules`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const CONTINUE: ToolAdapter = ToolAdapter {
    id: "continue",
    display_name: "Continue",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".continue/rules"],
    detect_paths: &[".continue"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "continue",
    docs_url: "https://docs.continue.dev/",
};

/// 来源：github.com/cline/cline + TASKBOOK §4.2（自定义指令目录
/// `~/Documents/Cline/Rules`）。项目级：None，回退全局。
/// 平台：`Documents/Cline/Rules` 在三平台均相对 home（应用内硬编码，
/// Windows 下即 `Documents\` 相对 home，非 Known Folder 重定向）。
pub const CLINE: ToolAdapter = ToolAdapter {
    id: "cline",
    display_name: "Cline",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &["Documents/Cline/Rules"],
    detect_paths: &["Documents/Cline"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "cline",
    docs_url: "https://github.com/cline/cline",
};

/// 来源：docs.roocode.com（github.com/RooCodeInc/Roo-Code；全局规则目录
/// `~/.roo/rules`）。项目级：None，回退全局。
/// 平台：三平台路径一致。
pub const ROO_CODE: ToolAdapter = ToolAdapter {
    id: "roo_code",
    display_name: "Roo Code",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".roo/rules"],
    detect_paths: &[".roo"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "roo-code",
    docs_url: "https://docs.roocode.com/",
};

// ---------------------------------------------------------------------------
// 终端 TUI（2）
// ---------------------------------------------------------------------------

/// 来源：github.com/charmbracelet/crush（配置根 `~/.config/crush` 确定：
/// Crush.toml；经 `.config/` 前缀映射 `dirs::config_dir()`）。
/// 项目级：None，回退全局。平台：macOS 上 Charm 工具链多遵循 XDG
///（`~/.config/crush`），映射目标【待复核】。skills 子目录【待复核】。
pub const CRUSH: ToolAdapter = ToolAdapter {
    id: "crush",
    display_name: "Crush",
    form_factor: FormFactor::TerminalTui,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".config/crush/skills"],
    detect_paths: &[".config/crush"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "crush",
    docs_url: "https://github.com/charmbracelet/crush",
};

/// 来源：ampcode.com（Sourcegraph Amp；配置根 `~/.amp` 确定）。
/// 项目级：None，回退全局。
/// 平台：**仅 macOS / Linux**（Windows 需 WSL，官方不支持原生 Windows），
/// 用于平台掩码过滤测试。skills 子目录【待复核】。
pub const AMP: ToolAdapter = ToolAdapter {
    id: "amp",
    display_name: "Amp",
    form_factor: FormFactor::TerminalTui,
    platforms: skillbox_domain::PlatformMask::MACOS.union(skillbox_domain::PlatformMask::LINUX),
    global_skills_dirs: &[".amp/skills"],
    detect_paths: &[".amp"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "amp",
    docs_url: "https://ampcode.com/",
};

// ---------------------------------------------------------------------------
// 桌面助手（2）
// ---------------------------------------------------------------------------

/// 来源：openclaw.ai / docs.openclaw.ai（网关根 `~/.openclaw` 确定；
/// skills 子目录 `~/.openclaw/skills`）。项目级：None，回退全局。
/// 平台：三平台路径一致。
pub const OPENCLAW: ToolAdapter = ToolAdapter {
    id: "openclaw",
    display_name: "OpenClaw",
    form_factor: FormFactor::DesktopAssistant,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".openclaw/skills"],
    detect_paths: &[".openclaw"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "openclaw",
    docs_url: "https://openclaw.ai/",
};

/// 来源：TASKBOOK §4.2（桌面助手，`recursive_scan: true`——skills 目录
/// 含嵌套分类子目录，扫描需递归一层分类）。配置根 `~/.hermes` 与
/// skills 子目录均为 v1 约定候选【待复核】，官方文档 URL 待定。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const HERMES: ToolAdapter = ToolAdapter {
    id: "hermes",
    display_name: "Hermes",
    form_factor: FormFactor::DesktopAssistant,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".hermes/skills"],
    detect_paths: &[".hermes"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: true,
    icon: "hermes",
    docs_url: "",
};

// ---------------------------------------------------------------------------
// 表与查询函数
// ---------------------------------------------------------------------------

/// 全部内置 adapter，按 §4.2 形态分组顺序排列
///（CLI 6 → IDE/桌面端 5 → 编辑器插件 4 → 终端 TUI 2 → 桌面助手 2）。
pub const ALL: &[ToolAdapter] = &[
    CLAUDE_CODE,
    CODEX,
    GEMINI_CLI,
    OPENCODE,
    QWEN_CODE,
    GROK,
    CURSOR,
    WINDSURF,
    TRAE,
    ANTIGRAVITY,
    ZED,
    GITHUB_COPILOT,
    CONTINUE,
    CLINE,
    ROO_CODE,
    CRUSH,
    AMP,
    OPENCLAW,
    HERMES,
];

/// 全部内置 adapter（§4.2）。
pub fn all() -> &'static [ToolAdapter] {
    ALL
}

/// 按 id 查找内置 adapter。
pub fn find(id: &str) -> Option<&'static ToolAdapter> {
    ALL.iter().find(|a| a.id == id)
}

/// 按形态分组（依赖 [`ALL`] 的排列顺序，做连续段分组）。
/// UI 侧栏与工具页必须按形态分节呈现（§4.2 硬约束）。
pub fn grouped_by_form_factor() -> Vec<(FormFactor, Vec<&'static ToolAdapter>)> {
    let mut groups: Vec<(FormFactor, Vec<&'static ToolAdapter>)> = Vec::new();
    for adapter in ALL {
        if let Some((form_factor, members)) = groups.last_mut() {
            if *form_factor == adapter.form_factor {
                members.push(adapter);
                continue;
            }
        }
        groups.push((adapter.form_factor, vec![adapter]));
    }
    groups
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// §4.2 表格实际列举 19 个工具（正文"16 个"与表格不符，以表格为准）。
    #[test]
    fn table_covers_all_taskbook_tools() {
        assert_eq!(ALL.len(), 19);
    }

    #[test]
    fn ids_are_unique_and_non_empty() {
        let mut ids: Vec<&str> = ALL.iter().map(|a| a.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "工具 id 必须无重复");
        assert!(ids.iter().all(|id| !id.is_empty()));
    }

    /// 分组计数与 §4.2 表格一致：CLI 6 / IDE·桌面端 5 / 编辑器插件 4 /
    /// 终端 TUI 2 / 桌面助手 2。
    #[test]
    fn grouped_by_form_factor_matches_taskbook() {
        let groups = grouped_by_form_factor();
        let summary: Vec<(FormFactor, usize)> =
            groups.iter().map(|(ff, members)| (*ff, members.len())).collect();
        assert_eq!(
            summary,
            vec![
                (FormFactor::Cli, 6),
                (FormFactor::Ide, 5),
                (FormFactor::EditorPlugin, 4),
                (FormFactor::TerminalTui, 2),
                (FormFactor::DesktopAssistant, 2),
            ]
        );
        // 分组必须覆盖全部工具且无遗漏
        let grouped_total: usize = groups.iter().map(|(_, m)| m.len()).sum();
        assert_eq!(grouped_total, ALL.len());
    }

    /// 每个工具都能解析出非空的全局 skills 目录（要求 4/6 的底线校验）。
    #[test]
    fn every_tool_resolves_non_empty_skills_dir() {
        let scope = crate::PathScope::new("/home/t".into(), "/home/t/.config".into());
        let overrides = crate::ToolOverrides::new();
        for adapter in ALL {
            let dirs = adapter.resolve_global_skills_dirs(&scope, &overrides);
            assert!(!dirs.is_empty(), "{} 必须有全局 skills 目录", adapter.id);
            assert!(
                dirs.iter().all(|d| !d.path.as_os_str().is_empty()),
                "{} 解析路径不得为空",
                adapter.id
            );
            assert!(!adapter.display_name.is_empty(), "{} 展示名不得为空", adapter.id);
            assert!(!adapter.icon.is_empty(), "{} 图标 id 不得为空", adapter.id);
        }
    }

    #[test]
    fn find_returns_registered_tools_only() {
        assert!(find("claude_code").is_some());
        assert!(find("opencode").is_some());
        assert!(find("nonexistent").is_none());
    }

    /// TASKBOOK §4.2 明示的不对称案例必须成立：
    /// OpenCode 全局走 `.config/`（XDG），项目级相对项目根且不同名。
    #[test]
    fn opencode_global_and_project_asymmetry() {
        let opencode = find("opencode").unwrap();
        assert!(opencode.global_skills_dirs[0].starts_with(".config/"));
        assert_eq!(opencode.project_skills_dir, Some(".opencode/skills"));
    }

    /// Hermes 必须开启递归扫描（TASKBOOK §4.2 括号标注）。
    #[test]
    fn hermes_is_recursive_scan() {
        assert!(find("hermes").unwrap().recursive_scan);
        // 其余工具一律非递归
        assert!(ALL.iter().filter(|a| a.id != "hermes").all(|a| !a.recursive_scan));
    }
}
