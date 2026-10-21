# SkillBox 适配器表（docs/adapters.md）

> 来源代码：`crates/adapters/src/table.rs`（声明表）、`crates/adapters/src/resolve.rs`（路径解析）。
> 数目说明：TASKBOOK 正文写"16 个"，但 §4.2 表格实际列举 19 个（CLI 6 / IDE·桌面端 5 / 编辑器插件 4 / 终端 TUI 2 / 桌面助手 2），按"以 §4.2 表格为准"实现。

## 解析规则（§4.2）

- 相对路径默认相对用户 home（`dirs::home_dir()`）。
- `.config/` 前缀自动映射到平台配置根（`dirs::config_dir()`）：Linux `~/.config`（或 `$XDG_CONFIG_HOME`）、macOS `~/Library/Application Support`、Windows `%APPDATA%`。
- 用户覆写（settings 层 `ToolOverrides`）优先于默认表路径；覆写即视为已安装。
- `extra_scan_dirs` 解析结果类型级强制 `ReadOnly`（§4.3）：只发现登记，禁止部署。
- `project_skills_dir` 相对**项目根**解析（与 home/config 无关）；`None` = 无项目级路径，调用方回退全局目录。
- 平台掩码过滤：不满足当前平台的工具整条排除（如 Windows 上无 Amp、Linux 上无 Trae）。

## 工具表（19 个）

| # | id | 名称 | 形态 | 全局 skills 目录 | 项目级 | 探测路径 | extra_scan（只读） | 平台 | 依据 |
|---|----|------|------|------------------|--------|----------|--------------------|------|------|
| 1 | `claude_code` | Claude Code | CLI | `~/.claude/skills` | `.claude/skills`（对称） | `~/.claude`、`~/.claude.json` | `~/.claude/plugins` | 三平台 | code.claude.com/docs/en/skills（官方 Agent Skills 文档） |
| 2 | `codex` | Codex | CLI | `~/.codex/skills` | `.codex/skills`（对称） | `~/.codex` | — | 三平台 | github.com/openai/codex（配置根 `~/.codex` 确定；skills 子目录按 §4.2，**待复核**） |
| 3 | `gemini_cli` | Gemini CLI | CLI | `~/.gemini/skills` | 无（回退全局） | `~/.gemini` | — | 三平台 | github.com/google-gemini/gemini-cli（配置根 `~/.gemini` 确定；skills 子目录**待复核**） |
| 4 | `opencode` | OpenCode | CLI | `~/.config/opencode/skills`（XDG 映射） | `.opencode/skills`（**不对称**：全局走 `.config/`，项目级相对项目根且不同名） | `~/.config/opencode` | — | 三平台 | opencode.ai/docs + github.com/sst/opencode（配置根 `~/.config/opencode` 确定） |
| 5 | `qwen_code` | Qwen Code | CLI | `~/.qwen/skills` | 无（回退全局） | `~/.qwen` | — | 三平台 | github.com/QwenLM/qwen-code（Gemini CLI fork，配置根 `~/.qwen`；skills 子目录**待复核**） |
| 6 | `grok` | Grok | CLI | `~/.grok/skills` | 无（回退全局） | `~/.grok` | — | 三平台 | github.com/superagent-ai/grok-cli（配置根 `~/.grok` 为 v1 约定候选，**待复核**） |
| 7 | `cursor` | Cursor | IDE | `~/.cursor/skills` | `.cursor/skills`（对称） | `~/.cursor` | `~/.cursor/extensions` | 三平台 | cursor.com/docs（Agent Skills 官方文档） |
| 8 | `windsurf` | Windsurf | IDE | `~/.codeium/windsurf/skills` | 无（回退全局） | `~/.codeium/windsurf` | — | 三平台 | docs.windsurf.com（配置根 `~/.codeium/windsurf` 确定；skills 子目录**待复核**） |
| 9 | `trae` | Trae / Trae CN | IDE | `~/.trae/skills`、`~/.trae-cn/skills`（国际版/国内版双候选） | 无（回退全局） | `~/.trae`、`~/.trae-cn` | — | **仅 Win/macOS** | docs.trae.ai / docs.trae.com.cn（双版本同一声明；skills 子目录**待复核**） |
| 10 | `antigravity` | Antigravity | IDE | `~/.antigravity/skills` | 无（回退全局） | `~/.antigravity` | — | 三平台 | antigravity.google（VSCode 系 fork；路径为 v1 约定候选，**待复核**） |
| 11 | `zed` | Zed | IDE | `~/.config/zed/skills`（XDG 映射） | 无（回退全局） | `~/.config/zed` | — | 三平台 | zed.dev/docs（Linux `~/.config/zed`；Windows 官方目录大小写为 "Zed"，**待复核**） |
| 12 | `github_copilot` | GitHub Copilot | 编辑器插件 | `~/.github/instructions` | 无（项目级为单文件 `.github/copilot-instructions.md`，v1 不扫描） | `~/.github` | — | 三平台 | docs.github.com/en/copilot（全局指令目录按 §4.2） |
| 13 | `continue` | Continue | 编辑器插件 | `~/.continue/rules` | 无（回退全局） | `~/.continue` | — | 三平台 | docs.continue.dev（配置根 `~/.continue` 确定：config.yaml 等） |
| 14 | `cline` | Cline | 编辑器插件 | `~/Documents/Cline/Rules` | 无（回退全局） | `~/Documents/Cline` | — | 三平台 | github.com/cline/cline（应用内硬编码 `Documents/Cline/Rules`，三平台均相对 home，Windows 下不跟 Known Folder 重定向） |
| 15 | `roo_code` | Roo Code | 编辑器插件 | `~/.roo/rules` | 无（回退全局） | `~/.roo` | — | 三平台 | docs.roocode.com（github.com/RooCodeInc/Roo-Code） |
| 16 | `crush` | Crush | 终端 TUI | `~/.config/crush/skills`（XDG 映射） | 无（回退全局） | `~/.config/crush` | — | 三平台 | github.com/charmbracelet/crush（配置根 `~/.config/crush` 确定：Crush.toml；skills 子目录**待复核**） |
| 17 | `amp` | Amp | 终端 TUI | `~/.amp/skills` | 无（回退全局） | `~/.amp` | — | **仅 macOS/Linux**（Windows 需 WSL） | ampcode.com（Sourcegraph Amp，配置根 `~/.amp` 确定） |
| 18 | `openclaw` | OpenClaw | 桌面助手 | `~/.openclaw/skills` | 无（回退全局） | `~/.openclaw` | — | 三平台 | openclaw.ai / docs.openclaw.ai（网关根 `~/.openclaw` 确定） |
| 19 | `hermes` | Hermes | 桌面助手 | `~/.hermes/skills`（**递归扫描**：`recursive_scan: true`，skills 目录含嵌套分类子目录，扫描递归一层分类；其余 18 个工具一律非递归） | 无（回退全局） | `~/.hermes` | — | 三平台 | TASKBOOK §4.2（配置根与子目录为 v1 约定候选，**待复核**，官方文档 URL 待定） |

## 形态分组（UI 侧栏 / 工具页呈现顺序）

| 形态 | 数量 | 成员 |
|------|------|------|
| CLI | 6 | claude_code, codex, gemini_cli, opencode, qwen_code, grok |
| IDE / 桌面端 | 5 | cursor, windsurf, trae, antigravity, zed |
| 编辑器插件 | 4 | github_copilot, continue, cline, roo_code |
| 终端 TUI | 2 | crush, amp |
| 桌面助手 | 2 | openclaw, hermes |

## 链接能力（§5.2 回退链起点，按当前平台）

- Unix → Symlink
- Windows → Junction（目录链接，无需特权）
- 其余 → CopyOnly

回退链：Symlink → Junction → Copy；工具能力不足时逐级回退，最终必然收敛到 Copy。

## 待复核清单（交付后需经官方来源核实）

- `codex` / `gemini_cli` / `qwen_code` / `grok` / `windsurf` / `trae` / `antigravity` / `crush` / `hermes` 的 skills 子目录名。
- `zed` 在 Windows 下官方目录大小写（"Zed" vs "zed"）。
- `grok` / `antigravity` / `hermes` 的配置根本身。
- `crush` 在 macOS 上是否遵循 XDG（`~/.config/crush`）。
