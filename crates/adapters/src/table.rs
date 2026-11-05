//! §4.2 内置工具声明表（v1.1：56 个）。
//!
//! 排列顺序即 §4.2 的形态分组顺序：CLI → IDE/桌面端 → 编辑器插件 →
//! 终端 TUI → 桌面助手。[`grouped_by_form_factor`] 依赖此顺序做连续分组。
//!
//! ## 扩表依据（REVIEW #2）
//!
//! v1.0 只内置 19 个工具。v1.1 以开源 skills-manager（MIT，
//! github.com/xingkongliang/skills-manager）的 56 代理适配表为基准扩表，
//! 另加本机实证的 DevEco Code，合计 56 个：
//!
//! - 重叠工具以 skills-manager 的实证路径为准（其注释均标注来源为官方
//!   文档或源码级验证），共更新六项：`antigravity`、`github_copilot`、
//!   `cline`、`roo_code`、`continue`、`amp`（v1.0 的旧路径为待复核猜测）；
//! - Trae 国际版 / 国内版与 WorkBuddy 国内版 / 海外版沿用「一条声明、
//!   多候选根」的合并先例（Trae 先例，`global_skills_dirs` 多条目）；
//! - DevEco Code 路径来自本机实证（`~/.config/deveco/skills` 与
//!   `~/.local/share/deveco/skills` 双根，均有真实 skills）。
//!
//! 每个声明上方的注释三段式（§4.2 硬约束）：路径来源依据 /
//! 全局与项目级是否不对称 / 平台差异。标注「待复核」的路径需在交付
//! `docs/adapters.md` 前经官方来源核实。

use skillbox_domain::FormFactor;

use crate::ToolAdapter;

// ---------------------------------------------------------------------------
// CLI（31）
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
    icon: "claude_code",
    docs_url: "https://code.claude.com/docs/en/skills",
};

/// 来源：github.com/openai/codex（配置根 `~/.codex` 确定；skills 目录
/// `~/.codex/skills`）。共享根 `~/.agents/skills` 仅作发现回退
///（skills-manager：老版本误部署到此的 skill 仍能浮出），§4.3 只读。
/// 不对称：全局与项目级 `.codex/skills` 同名，对称。
/// 平台：三平台路径一致。
pub const CODEX: ToolAdapter = ToolAdapter {
    id: "codex",
    display_name: "Codex",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".codex/skills"],
    detect_paths: &[".codex"],
    project_skills_dir: Some(".codex/skills"),
    extra_scan_dirs: &[".agents/skills"],
    recursive_scan: false,
    icon: "codex",
    docs_url: "https://developers.openai.com/codex/",
};

/// 来源：github.com/google-gemini/gemini-cli（配置根 `~/.gemini` 确定：
/// GEMINI.md、extensions 等）；skills 目录 `~/.gemini/skills` 依据
/// skills-manager。项目级：无独立路径（None，回退全局）。
/// 平台：三平台路径一致。
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
    icon: "gemini_cli",
    docs_url: "https://github.com/google-gemini/gemini-cli",
};

/// 来源：opencode.ai/docs + github.com/sst/opencode（配置根
/// `~/.config/opencode`，遵循 XDG）。
/// 【不对称】全局 `~/.config/opencode/skills`（`.config/` 前缀展开为
/// 字面 `~/.config` 与平台配置根双候选，见 `PathScope::resolve_candidates`）
/// 与项目级 `.opencode/skills`（相对项目根）不同名——TASKBOOK §4.2
/// 明示的"全局与项目级路径不对称"案例。
/// 平台：`.config/` 条目三平台均先取字面 `~/.config` 候选
///（Windows 上 OpenCode 仍用字面布局），再取 `dirs::config_dir()` 候选。
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
/// 平台：三平台路径一致。
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
    icon: "qwen_code",
    docs_url: "https://github.com/QwenLM/qwen-code",
};

/// 来源：docs.x.ai（Grok 生态 CLI；skills 目录 `~/.grok/skills`，
/// skills-manager 引 docs.x.ai/build/features/skills-plugins-marketplaces）。
/// 项目级：None，回退全局。平台：三平台路径一致。
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
    docs_url: "https://docs.x.ai/",
};

/// 来源：skills-manager（oh-my-pi `discovery/builtin.ts` 的 `native`
/// provider）：全局 `~/.omp/agent/skills`（活跃 profile 的 agent 目录）。
/// 【不对称】项目级 `<repo>/.omp/skills`（无 `agent` 段）。
/// 平台：三平台路径一致。
pub const OMP_AGENT: ToolAdapter = ToolAdapter {
    id: "omp_agent",
    display_name: "OMP Agent",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".omp/agent/skills"],
    detect_paths: &[".omp/agent"],
    project_skills_dir: Some(".omp/skills"),
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "omp_agent",
    docs_url: "",
};

/// 来源：block.github.io/goose（Block Goose；配置根 `~/.config/goose`
/// 遵循 XDG，`.config/` 双候选展开）。
/// 项目级：None，回退全局。平台：三平台（Windows 上字面 `~/.config` 布局）。
pub const GOOSE: ToolAdapter = ToolAdapter {
    id: "goose",
    display_name: "Goose",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".config/goose/skills"],
    detect_paths: &[".config/goose"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "goose",
    docs_url: "https://block.github.io/goose/",
};

/// 来源：skills-manager（Factory Droid；配置根 `~/.factory`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const DROID: ToolAdapter = ToolAdapter {
    id: "droid",
    display_name: "Droid",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".factory/skills"],
    detect_paths: &[".factory"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "droid",
    docs_url: "https://factory.ai/",
};

/// 来源：skills-manager（配置根 `~/.deepagents`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const DEEPAGENTS: ToolAdapter = ToolAdapter {
    id: "deepagents",
    display_name: "Deep Agents",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".deepagents/agent/skills"],
    detect_paths: &[".deepagents"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "deepagents",
    docs_url: "",
};

/// 来源：skills-manager（配置根 `~/.firebender`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const FIREBENDER: ToolAdapter = ToolAdapter {
    id: "firebender",
    display_name: "Firebender",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".firebender/skills"],
    detect_paths: &[".firebender"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "firebender",
    docs_url: "",
};

/// 来源：skills-manager（Moonshot Kimi Code CLI；配置根 `~/.kimi-code`）。
/// 不对称：全局 `~/.kimi-code/skills` 与项目级 `.kimi-code/skills` 同名，对称。
/// 平台：三平台路径一致。
pub const KIMI: ToolAdapter = ToolAdapter {
    id: "kimi",
    display_name: "Kimi Code CLI",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".kimi-code/skills"],
    detect_paths: &[".kimi-code"],
    project_skills_dir: Some(".kimi-code/skills"),
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "kimi",
    docs_url: "https://github.com/MoonshotAI/Kimi-CLI",
};

/// 来源：skills-manager（Replit；skills 走 agents 标准共享根
/// `~/.config/agents/skills`，`.config/` 双候选展开；探测根 `~/.replit`）。
/// 项目级：None，回退全局。平台：三平台。
pub const REPLIT: ToolAdapter = ToolAdapter {
    id: "replit",
    display_name: "Replit",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".config/agents/skills"],
    detect_paths: &[".replit"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "replit",
    docs_url: "https://replit.com/",
};

/// 来源：skills-manager（IBM Bob；配置根 `~/.bob`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const BOB: ToolAdapter = ToolAdapter {
    id: "bob",
    display_name: "IBM Bob",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".bob/skills"],
    detect_paths: &[".bob"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "bob",
    docs_url: "",
};

/// 来源：skills-manager（Command Code；配置根 `~/.commandcode`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const COMMAND_CODE: ToolAdapter = ToolAdapter {
    id: "command_code",
    display_name: "Command Code",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".commandcode/skills"],
    detect_paths: &[".commandcode"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "command_code",
    docs_url: "",
};

/// 来源：skills-manager（Snowflake Cortex Code；配置根
/// `~/.snowflake/cortex`，skills 在其下 `skills/`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const CORTEX: ToolAdapter = ToolAdapter {
    id: "cortex",
    display_name: "Cortex Code",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".snowflake/cortex/skills"],
    detect_paths: &[".snowflake/cortex"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "cortex",
    docs_url: "",
};

/// 来源：skills-manager（iFlow CLI；配置根 `~/.iflow`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const IFLOW: ToolAdapter = ToolAdapter {
    id: "iflow",
    display_name: "iFlow CLI",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".iflow/skills"],
    detect_paths: &[".iflow"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "iflow",
    docs_url: "",
};

/// 来源：skills-manager（Kode；配置根 `~/.kode`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const KODE: ToolAdapter = ToolAdapter {
    id: "kode",
    display_name: "Kode",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".kode/skills"],
    detect_paths: &[".kode"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "kode",
    docs_url: "",
};

/// 来源：skills-manager（MCPJam；配置根 `~/.mcpjam`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const MCPJAM: ToolAdapter = ToolAdapter {
    id: "mcpjam",
    display_name: "MCPJam",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".mcpjam/skills"],
    detect_paths: &[".mcpjam"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "mcpjam",
    docs_url: "",
};

/// 来源：skills-manager（Mistral Vibe；配置根 `~/.vibe`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const MISTRAL_VIBE: ToolAdapter = ToolAdapter {
    id: "mistral_vibe",
    display_name: "Mistral Vibe",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".vibe/skills"],
    detect_paths: &[".vibe"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "mistral_vibe",
    docs_url: "",
};

/// 来源：skills-manager（Mux；配置根 `~/.mux`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const MUX: ToolAdapter = ToolAdapter {
    id: "mux",
    display_name: "Mux",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".mux/skills"],
    detect_paths: &[".mux"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "mux",
    docs_url: "",
};

/// 来源：skills-manager（Neovate；配置根 `~/.neovate`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const NEOVATE: ToolAdapter = ToolAdapter {
    id: "neovate",
    display_name: "Neovate",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".neovate/skills"],
    detect_paths: &[".neovate"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "neovate",
    docs_url: "",
};

/// 来源：github.com/All-Hands-AI/OpenHands（配置根 `~/.openhands`，
/// skills-manager 同）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const OPENHANDS: ToolAdapter = ToolAdapter {
    id: "openhands",
    display_name: "OpenHands",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".openhands/skills"],
    detect_paths: &[".openhands"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "openhands",
    docs_url: "https://github.com/All-Hands-AI/OpenHands",
};

/// 来源：skills-manager（Pi coding agent，`docs/skills.md`）：全局
/// `~/.pi/agent/skills`（含 `agent` 段）；共享根 `~/.agents/skills` 仅作
/// 发现回退（§4.3 只读）。【不对称】项目级 `<repo>/.pi/skills`（无段）。
/// 平台：三平台路径一致。
pub const PI: ToolAdapter = ToolAdapter {
    id: "pi",
    display_name: "Pi",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".pi/agent/skills"],
    detect_paths: &[".pi/agent"],
    project_skills_dir: Some(".pi/skills"),
    extra_scan_dirs: &[".agents/skills"],
    recursive_scan: false,
    icon: "pi",
    docs_url: "https://github.com/badlogic/pi-mono",
};

/// 来源：skills-manager（Pochi；配置根 `~/.pochi`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const POCHI: ToolAdapter = ToolAdapter {
    id: "pochi",
    display_name: "Pochi",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".pochi/skills"],
    detect_paths: &[".pochi"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "pochi",
    docs_url: "",
};

/// 来源：skills-manager（Zencoder；配置根 `~/.zencoder`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const ZENCODER: ToolAdapter = ToolAdapter {
    id: "zencoder",
    display_name: "Zencoder",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".zencoder/skills"],
    detect_paths: &[".zencoder"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "zencoder",
    docs_url: "",
};

/// 来源：skills-manager（ZCode；配置根 `~/.zcode`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const ZCODE: ToolAdapter = ToolAdapter {
    id: "zcode",
    display_name: "ZCode",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".zcode/skills"],
    detect_paths: &[".zcode"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "zcode",
    docs_url: "",
};

/// 来源：skills-manager（AdaL；配置根 `~/.adal`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const ADAL: ToolAdapter = ToolAdapter {
    id: "adal",
    display_name: "AdaL",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".adal/skills"],
    detect_paths: &[".adal"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "adal",
    docs_url: "",
};

/// 来源：skills-manager（DeepSeek Harness，`packages/util/home-paths`：
/// `$DSH_HOME` 或 `~/.dsh`）；共享根 `~/.agents/skills` 仅作发现回退
///（§4.3 只读）。项目级 `<project>/.dsh/skills` 与全局同名，对称。
/// 平台：三平台路径一致。
pub const DEEPSEEK_HARNESS: ToolAdapter = ToolAdapter {
    id: "deepseek_harness",
    display_name: "DeepSeek Harness",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".dsh/skills"],
    detect_paths: &[".dsh"],
    project_skills_dir: None,
    extra_scan_dirs: &[".agents/skills"],
    recursive_scan: false,
    icon: "deepseek_harness",
    docs_url: "",
};

/// 来源：skills-manager（MiniMax Code 3.1.4，读内置 runtime 的
/// skill-root resolver 验证）：全局 `~/.minimax/skills`，项目级同名对称；
/// 共享根 `~/.agents/skills`（`skills.external` 默认开）仅作发现回退
///（§4.3 只读）。`~/.minimax/.builtin-skills` 随启动重播种、`~/.mavis`
/// 为改名前兼容符号链接，二者均不扫。
/// 平台：Windows/macOS 共用 `os.homedir()/.minimax`（无 %APPDATA% 分支）。
pub const MINIMAX_CODE: ToolAdapter = ToolAdapter {
    id: "minimax_code",
    display_name: "MiniMax Code",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".minimax/skills"],
    detect_paths: &[".minimax"],
    project_skills_dir: None,
    extra_scan_dirs: &[".agents/skills"],
    recursive_scan: false,
    icon: "minimax_code",
    docs_url: "https://www.minimax.io/",
};

/// 来源：本机实证（华为 DevEco Code，`devecocli`）：skills 双根
/// `~/.config/deveco/skills` 与 `~/.local/share/deveco/skills`，
/// 两根均有真实 skills（17 + 21 个）。`.config/` 条目按双候选展开
///（字面 `~/.config` 优先）。项目级：None，回退全局。
/// 平台：三平台（Windows 上为字面 `~/.config` 布局）。
pub const DEVECOCODE: ToolAdapter = ToolAdapter {
    id: "devecocode",
    display_name: "DevEco Code",
    form_factor: FormFactor::Cli,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".config/deveco/skills", ".local/share/deveco/skills"],
    detect_paths: &[".config/deveco", ".local/share/deveco"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "devecocode",
    docs_url: "https://developer.huawei.com/consumer/cn/",
};

// ---------------------------------------------------------------------------
// IDE / 桌面端（8）
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
/// `~/.codeium/windsurf` 确定；skills 目录 `.codeium/windsurf/skills`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
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

/// 来源：antigravity.google/docs/skills（v1.1 按 skills-manager 实证更新，
/// 替换 v1.0 的 `~/.antigravity/skills` 猜测）：全局
/// `~/.gemini/config/skills`；`~/.gemini/antigravity/` 仅为运行时
/// appDataDir（brain/scratch/builtin），探测用。【不对称】项目级
/// `<workspace>/.agents/skills`。平台：三平台。
pub const ANTIGRAVITY: ToolAdapter = ToolAdapter {
    id: "antigravity",
    display_name: "Antigravity",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".gemini/config/skills"],
    detect_paths: &[".gemini/antigravity"],
    project_skills_dir: Some(".agents/skills"),
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "antigravity",
    docs_url: "https://antigravity.google/docs/skills",
};

/// 来源：zed.dev/docs + TASKBOOK §4.2（`~/.config/zed/...`）。
/// 平台差异：`.config/` 条目双候选展开——字面 `~/.config/zed`（Linux
/// 惯例）与 `dirs::config_dir()`（macOS `~/Library/Application Support`、
/// Windows `%APPDATA%`）。项目级：None，回退全局。
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

/// 来源：skills-manager（AWS Kiro；配置根 `~/.kiro`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const KIRO: ToolAdapter = ToolAdapter {
    id: "kiro",
    display_name: "Kiro",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".kiro/skills"],
    detect_paths: &[".kiro"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "kiro",
    docs_url: "https://kiro.dev/",
};

/// 来源：skills-manager（阿里 Qoder；配置根 `~/.qoder`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const QODER: ToolAdapter = ToolAdapter {
    id: "qoder",
    display_name: "Qoder",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".qoder/skills"],
    detect_paths: &[".qoder"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "qoder",
    docs_url: "https://qoder.com/",
};

/// 来源：skills-manager（腾讯 CodeBuddy；配置根 `~/.codebuddy`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const CODEBUDDY: ToolAdapter = ToolAdapter {
    id: "codebuddy",
    display_name: "CodeBuddy",
    form_factor: FormFactor::Ide,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".codebuddy/skills"],
    detect_paths: &[".codebuddy"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "codebuddy",
    docs_url: "https://codebuddy.ai/",
};

// ---------------------------------------------------------------------------
// 编辑器插件（8）
// ---------------------------------------------------------------------------

/// 来源：skills-manager（v1.1 更新：Copilot agent skills 读
/// `~/.copilot/skills`，替换 v1.0 的指令目录 `~/.github/instructions`）；
/// 共享根 `~/.agents/skills` 仅作发现回退（§4.3 只读）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const GITHUB_COPILOT: ToolAdapter = ToolAdapter {
    id: "github_copilot",
    display_name: "GitHub Copilot",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".copilot/skills"],
    detect_paths: &[".copilot"],
    project_skills_dir: None,
    extra_scan_dirs: &[".agents/skills"],
    recursive_scan: false,
    icon: "github_copilot",
    docs_url: "https://docs.github.com/en/copilot",
};

/// 来源：skills-manager（v1.1 更新：Continue skills 读
/// `~/.continue/skills`，替换 v1.0 的规则目录 `~/.continue/rules`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const CONTINUE: ToolAdapter = ToolAdapter {
    id: "continue",
    display_name: "Continue",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".continue/skills"],
    detect_paths: &[".continue"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "continue",
    docs_url: "https://docs.continue.dev/",
};

/// 来源：skills-manager（v1.1 更新：Cline 采用 agents 标准共享根
/// `~/.agents/skills`，替换 v1.0 的 `~/Documents/Cline/Rules` 指令目录）。
/// 项目级：None，回退全局。平台：三平台路径一致（共享根相对 home）。
pub const CLINE: ToolAdapter = ToolAdapter {
    id: "cline",
    display_name: "Cline",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".agents/skills"],
    detect_paths: &[".cline"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "cline",
    docs_url: "https://github.com/cline/cline",
};

/// 来源：skills-manager（v1.1 更新：Roo Code skills 读 `~/.roo/skills`，
/// 替换 v1.0 的规则目录 `~/.roo/rules`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const ROO_CODE: ToolAdapter = ToolAdapter {
    id: "roo_code",
    display_name: "Roo Code",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".roo/skills"],
    detect_paths: &[".roo"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "roo_code",
    docs_url: "https://docs.roocode.com/",
};

/// 来源：skills-manager（Kilo Code；配置根 `~/.kilocode`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const KILO_CODE: ToolAdapter = ToolAdapter {
    id: "kilo_code",
    display_name: "Kilo Code",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".kilocode/skills"],
    detect_paths: &[".kilocode"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "kilo_code",
    docs_url: "https://kilocode.ai/",
};

/// 来源：skills-manager（Augment；配置根 `~/.augment`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const AUGMENT: ToolAdapter = ToolAdapter {
    id: "augment",
    display_name: "Augment",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".augment/skills"],
    detect_paths: &[".augment"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "augment",
    docs_url: "https://www.augmentcode.com/",
};

/// 来源：skills-manager（JetBrains Junie；配置根 `~/.junie`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const JUNIE: ToolAdapter = ToolAdapter {
    id: "junie",
    display_name: "Junie",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".junie/skills"],
    detect_paths: &[".junie"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "junie",
    docs_url: "https://www.jetbrains.com/junie/",
};

/// 来源：skills-manager（GitLab Duo CLI，读内置二进制 `AgentSkillsResolver`
/// 验证）：全局 `~/.gitlab/duo/skills`（Windows 真实位置为
/// `%APPDATA%\GitLab\duo`，需用户覆写）；共享根 `~/.agents/skills` 仅作
/// 发现回退（§4.3 只读）。【不对称】项目级 `<repo>/.agents/skills`。
/// 平台：Linux/macOS 默认布局直接命中；Windows 需覆写。
pub const GITLAB_DUO: ToolAdapter = ToolAdapter {
    id: "gitlab_duo",
    display_name: "GitLab Duo",
    form_factor: FormFactor::EditorPlugin,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".gitlab/duo/skills"],
    detect_paths: &[".gitlab/duo"],
    project_skills_dir: Some(".agents/skills"),
    extra_scan_dirs: &[".agents/skills"],
    recursive_scan: false,
    icon: "gitlab_duo",
    docs_url: "https://about.gitlab.com/gitlab-duo/",
};

// ---------------------------------------------------------------------------
// 终端 TUI（3）
// ---------------------------------------------------------------------------

/// 来源：github.com/charmbracelet/crush（配置根 `~/.config/crush` 确定：
/// Crush.toml；`.config/` 双候选展开）。
/// 项目级：None，回退全局。平台：macOS 上 Charm 工具链多遵循 XDG
///（字面 `~/.config/crush`）。
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

/// 来源：ampcode.com（Sourcegraph Amp；v1.1 按 skills-manager 更新：
/// skills 走 agents 标准共享根 `~/.config/agents/skills`，替换 v1.0 的
/// `~/.amp/skills`）。项目级：None，回退全局。
/// 平台：**仅 macOS / Linux**（Windows 需 WSL，官方不支持原生 Windows），
/// 用于平台掩码过滤测试。
pub const AMP: ToolAdapter = ToolAdapter {
    id: "amp",
    display_name: "Amp",
    form_factor: FormFactor::TerminalTui,
    platforms: skillbox_domain::PlatformMask::MACOS.union(skillbox_domain::PlatformMask::LINUX),
    global_skills_dirs: &[".config/agents/skills"],
    detect_paths: &[".config/agents"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "amp",
    docs_url: "https://ampcode.com/",
};

/// 来源：skills-manager（Warp；skills 走 agents 标准共享根
/// `~/.agents/skills`，探测根 `~/.warp`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const WARP: ToolAdapter = ToolAdapter {
    id: "warp",
    display_name: "Warp",
    form_factor: FormFactor::TerminalTui,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".agents/skills"],
    detect_paths: &[".warp"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "warp",
    docs_url: "https://www.warp.dev/",
};

// ---------------------------------------------------------------------------
// 桌面助手（6）
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

/// 来源：skills-manager（OpenClaw 生态 QClaw；配置根 `~/.qclaw`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const QCLAW: ToolAdapter = ToolAdapter {
    id: "qclaw",
    display_name: "QClaw",
    form_factor: FormFactor::DesktopAssistant,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".qclaw/skills"],
    detect_paths: &[".qclaw"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "qclaw",
    docs_url: "",
};

/// 来源：skills-manager（OpenClaw 生态 EasyClaw；配置根 `~/.easyclaw`）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const EASYCLAW: ToolAdapter = ToolAdapter {
    id: "easyclaw",
    display_name: "EasyClaw",
    form_factor: FormFactor::DesktopAssistant,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".easyclaw/skills"],
    detect_paths: &[".easyclaw"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "easyclaw",
    docs_url: "",
};

/// 来源：skills-manager（OpenClaw 生态 AutoClaw；配置根
/// `~/.openclaw-autoclaw`）。项目级：None，回退全局。
/// 平台：三平台路径一致。
pub const AUTOCLAW: ToolAdapter = ToolAdapter {
    id: "autoclaw",
    display_name: "AutoClaw",
    form_factor: FormFactor::DesktopAssistant,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".openclaw-autoclaw/skills"],
    detect_paths: &[".openclaw-autoclaw"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "autoclaw",
    docs_url: "",
};

/// 来源：skills-manager（WorkBuddy 国内版 + 海外版合并声明，沿 Trae
/// 多候选先例）：国内版 skills `~/.workbuddy/skills`（中文文档路径）、
/// 海外版 `~/.workbuddy-ai/skills`（`~/.workbuddy/` 同时存在——应用
/// 持续写入 device id 与日志，故双探测根任一存在即视为已安装）。
/// 项目级：None，回退全局。平台：三平台路径一致。
pub const WORKBUDDY: ToolAdapter = ToolAdapter {
    id: "workbuddy",
    display_name: "WorkBuddy / WorkBuddy AI",
    form_factor: FormFactor::DesktopAssistant,
    platforms: skillbox_domain::PlatformMask::ALL,
    global_skills_dirs: &[".workbuddy/skills", ".workbuddy-ai/skills"],
    detect_paths: &[".workbuddy", ".workbuddy-ai"],
    project_skills_dir: None,
    extra_scan_dirs: &[],
    recursive_scan: false,
    icon: "workbuddy",
    docs_url: "",
};

// ---------------------------------------------------------------------------
// 表与查询函数
// ---------------------------------------------------------------------------

/// 全部内置 adapter，按 §4.2 形态分组顺序排列
///（CLI 31 → IDE/桌面端 8 → 编辑器插件 8 → 终端 TUI 3 → 桌面助手 6）。
pub const ALL: &[ToolAdapter] = &[
    // CLI（31）
    CLAUDE_CODE,
    CODEX,
    GEMINI_CLI,
    OPENCODE,
    QWEN_CODE,
    GROK,
    OMP_AGENT,
    GOOSE,
    DROID,
    DEEPAGENTS,
    FIREBENDER,
    KIMI,
    REPLIT,
    BOB,
    COMMAND_CODE,
    CORTEX,
    IFLOW,
    KODE,
    MCPJAM,
    MISTRAL_VIBE,
    MUX,
    NEOVATE,
    OPENHANDS,
    PI,
    POCHI,
    ZENCODER,
    ZCODE,
    ADAL,
    DEEPSEEK_HARNESS,
    MINIMAX_CODE,
    DEVECOCODE,
    // IDE / 桌面端（8）
    CURSOR,
    WINDSURF,
    TRAE,
    ANTIGRAVITY,
    ZED,
    KIRO,
    QODER,
    CODEBUDDY,
    // 编辑器插件（8）
    GITHUB_COPILOT,
    CONTINUE,
    CLINE,
    ROO_CODE,
    KILO_CODE,
    AUGMENT,
    JUNIE,
    GITLAB_DUO,
    // 终端 TUI（3）
    CRUSH,
    AMP,
    WARP,
    // 桌面助手（6）
    OPENCLAW,
    HERMES,
    QCLAW,
    EASYCLAW,
    AUTOCLAW,
    WORKBUDDY,
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

    /// v1.1 扩表（REVIEW #2）：56 个工具
    ///（skills-manager 55 项合并 Trae/WorkBuddy 双版本 + 本机实证 DevEco Code）。
    #[test]
    fn table_covers_56_tools() {
        assert_eq!(ALL.len(), 56);
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

    /// 分组计数与 v1.1 扩表计划一致：CLI 31 / IDE·桌面端 8 / 编辑器插件 8 /
    /// 终端 TUI 3 / 桌面助手 6。
    #[test]
    fn grouped_by_form_factor_matches_v11_plan() {
        let groups = grouped_by_form_factor();
        let summary: Vec<(FormFactor, usize)> =
            groups.iter().map(|(ff, members)| (*ff, members.len())).collect();
        assert_eq!(
            summary,
            vec![
                (FormFactor::Cli, 31),
                (FormFactor::Ide, 8),
                (FormFactor::EditorPlugin, 8),
                (FormFactor::TerminalTui, 3),
                (FormFactor::DesktopAssistant, 6),
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
        assert!(find("devecocode").is_some());
        assert!(find("workbuddy").is_some());
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

    /// 重叠工具以 skills-manager 实证路径为准（v1.1 更新的六项）。
    #[test]
    fn overlapping_tools_follow_sm_verified_paths() {
        let antigravity = find("antigravity").unwrap();
        assert_eq!(antigravity.global_skills_dirs, &[".gemini/config/skills"]);
        assert_eq!(antigravity.detect_paths, &[".gemini/antigravity"]);
        assert_eq!(antigravity.project_skills_dir, Some(".agents/skills"));

        let copilot = find("github_copilot").unwrap();
        assert_eq!(copilot.global_skills_dirs, &[".copilot/skills"]);
        assert_eq!(copilot.detect_paths, &[".copilot"]);

        let cline = find("cline").unwrap();
        assert_eq!(cline.global_skills_dirs, &[".agents/skills"]);
        assert_eq!(cline.detect_paths, &[".cline"]);

        let roo = find("roo_code").unwrap();
        assert_eq!(roo.global_skills_dirs, &[".roo/skills"]);
        assert_eq!(roo.detect_paths, &[".roo"]);

        let cont = find("continue").unwrap();
        assert_eq!(cont.global_skills_dirs, &[".continue/skills"]);
        assert_eq!(cont.detect_paths, &[".continue"]);

        let amp = find("amp").unwrap();
        assert_eq!(amp.global_skills_dirs, &[".config/agents/skills"]);
        assert_eq!(amp.detect_paths, &[".config/agents"]);
    }

    /// DevEco Code 双根（本机实证）：`.config/deveco`（字面 XDG 布局）与
    /// `.local/share/deveco`，两根均可部署。
    #[test]
    fn devecocode_uses_locally_verified_dual_roots() {
        let deveco = find("devecocode").unwrap();
        assert_eq!(
            deveco.global_skills_dirs,
            &[".config/deveco/skills", ".local/share/deveco/skills"]
        );
        assert_eq!(deveco.detect_paths, &[".config/deveco", ".local/share/deveco"]);
    }

    /// WorkBuddy 合并声明：国内版 + 海外版双候选根（沿 Trae 先例）。
    #[test]
    fn workbuddy_merges_domestic_and_overseas() {
        let wb = find("workbuddy").unwrap();
        assert_eq!(wb.global_skills_dirs, &[".workbuddy/skills", ".workbuddy-ai/skills"]);
        assert_eq!(wb.detect_paths, &[".workbuddy", ".workbuddy-ai"]);
    }
}
