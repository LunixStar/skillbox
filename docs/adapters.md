# SkillBox 适配器表（docs/adapters.md）

> 来源代码：`crates/adapters/src/table.rs`（声明表）、`crates/adapters/src/resolve.rs`（路径解析）。
> 数目说明：v1.0 内置 19 个；v1.1（REVIEW #2）以开源 skills-manager（MIT，github.com/xingkongliang/skills-manager）的 56 代理适配表为基准扩表至 56 个（CLI 31 / IDE·桌面端 8 / 编辑器插件 8 / 终端 TUI 3 / 桌面助手 6）。

## v1.1 扩表说明（REVIEW #2）

- 重叠工具以 skills-manager 实证路径为准（其注释标注来源为官方文档或源码级验证），共更新六项：`antigravity`、`github_copilot`、`cline`、`roo_code`、`continue`、`amp`（v1.0 旧路径为待复核猜测或指令目录）。
- Trae 国际版/国内版、WorkBuddy 国内版/海外版沿用「一条声明、多候选根」合并先例（`global_skills_dirs` 多条目）。
- `devecocode` 路径来自本机实证：`~/.config/deveco/skills` 与 `~/.local/share/deveco/skills` 双根（17 + 21 个真实 skills）。
- agents 标准共享根 `~/.agents/skills` / `~/.config/agents/skills` 出现于多个工具，一律只作发现回退（`extra_scan_dirs`，§4.3 只读）或该工具自身的部署根，不与其它工具混用。

## 解析规则（§4.2）

- 相对路径默认相对用户 home（`dirs::home_dir()`）。
- `.config/` 前缀**双候选展开**（v1.1 `PathScope::resolve_candidates`）：先取字面 `~/.config/<rel>`（XDG 惯例，Windows 上多数 CLI 仍用字面布局），再取平台配置根 `dirs::config_dir()/<rel>`（macOS `~/Library/Application Support`、Windows `%APPDATA%`），二者相等时去重。探测 = 任一候选存在；部署目录 = 第一个存在的候选，均不存在时回退字面候选。
- 用户覆写（settings 层 `ToolOverrides`）优先于默认表路径；覆写即视为已安装。
- `extra_scan_dirs` 解析结果类型级强制 `ReadOnly`（§4.3）：只发现登记，禁止部署。
- `project_skills_dir` 相对**项目根**解析（与 home/config 无关）；`None` = 无项目级路径，调用方回退全局目录。
- 平台掩码过滤：不满足当前平台的工具整条排除（如 Windows 上无 Amp、Linux 上无 Trae）。
- 多候选根（Trae / WorkBuddy / DevEco Code）：探测任一存在即视为已安装；部署到第一个存在的候选根，均不存在时回退第一个候选。

## 工具表（56 个）

| # | id | 名称 | 形态 | 全局 skills 目录 | 项目级 | 探测路径 | extra_scan（只读） | 平台 | 依据 |
|---|----|------|------|------------------|--------|----------|--------------------|------|------|
| 1 | `claude_code` | Claude Code | CLI | `~/.claude/skills` | `.claude/skills`（对称） | `~/.claude`、`~/.claude.json` | `~/.claude/plugins` | 三平台 | code.claude.com/docs/en/skills |
| 2 | `codex` | Codex | CLI | `~/.codex/skills` | `.codex/skills`（对称） | `~/.codex` | `~/.agents/skills` | 三平台 | github.com/openai/codex（配置根 `~/.codex` 确定；共享根仅发现回退） |
| 3 | `gemini_cli` | Gemini CLI | CLI | `~/.gemini/skills` | 无（回退全局） | `~/.gemini` | — | 三平台 | github.com/google-gemini/gemini-cli（配置根 `~/.gemini` 确定；skills 子目录依据 skills-manager） |
| 4 | `opencode` | OpenCode | CLI | `~/.config/opencode/skills`（双候选展开） | `.opencode/skills`（**不对称**） | `~/.config/opencode` | — | 三平台 | opencode.ai/docs（TASKBOOK §4.2 明示不对称案例） |
| 5 | `qwen_code` | Qwen Code | CLI | `~/.qwen/skills` | 无（回退全局） | `~/.qwen` | — | 三平台 | github.com/QwenLM/qwen-code（Gemini CLI fork） |
| 6 | `grok` | Grok | CLI | `~/.grok/skills` | 无（回退全局） | `~/.grok` | — | 三平台 | docs.x.ai（skills-manager 引官方 skills-plugins 文档） |
| 7 | `omp_agent` | OMP Agent | CLI | `~/.omp/agent/skills` | `.omp/skills`（**不对称**） | `~/.omp/agent` | — | 三平台 | skills-manager（oh-my-pi `discovery/builtin.ts` `native` provider） |
| 8 | `goose` | Goose | CLI | `~/.config/goose/skills`（双候选展开） | 无（回退全局） | `~/.config/goose` | — | 三平台 | block.github.io/goose |
| 9 | `droid` | Droid | CLI | `~/.factory/skills` | 无（回退全局） | `~/.factory` | — | 三平台 | skills-manager（Factory Droid） |
| 10 | `deepagents` | Deep Agents | CLI | `~/.deepagents/agent/skills` | 无（回退全局） | `~/.deepagents` | — | 三平台 | skills-manager |
| 11 | `firebender` | Firebender | CLI | `~/.firebender/skills` | 无（回退全局） | `~/.firebender` | — | 三平台 | skills-manager |
| 12 | `kimi` | Kimi Code CLI | CLI | `~/.kimi-code/skills` | `.kimi-code/skills`（对称） | `~/.kimi-code` | — | 三平台 | github.com/MoonshotAI/Kimi-CLI |
| 13 | `replit` | Replit | CLI | `~/.config/agents/skills`（agents 共享根） | 无（回退全局） | `~/.replit` | — | 三平台 | skills-manager（replit.com） |
| 14 | `bob` | IBM Bob | CLI | `~/.bob/skills` | 无（回退全局） | `~/.bob` | — | 三平台 | skills-manager |
| 15 | `command_code` | Command Code | CLI | `~/.commandcode/skills` | 无（回退全局） | `~/.commandcode` | — | 三平台 | skills-manager |
| 16 | `cortex` | Cortex Code | CLI | `~/.snowflake/cortex/skills` | 无（回退全局） | `~/.snowflake/cortex` | — | 三平台 | skills-manager（Snowflake） |
| 17 | `iflow` | iFlow CLI | CLI | `~/.iflow/skills` | 无（回退全局） | `~/.iflow` | — | 三平台 | skills-manager |
| 18 | `kode` | Kode | CLI | `~/.kode/skills` | 无（回退全局） | `~/.kode` | — | 三平台 | skills-manager |
| 19 | `mcpjam` | MCPJam | CLI | `~/.mcpjam/skills` | 无（回退全局） | `~/.mcpjam` | — | 三平台 | skills-manager |
| 20 | `mistral_vibe` | Mistral Vibe | CLI | `~/.vibe/skills` | 无（回退全局） | `~/.vibe` | — | 三平台 | skills-manager |
| 21 | `mux` | Mux | CLI | `~/.mux/skills` | 无（回退全局） | `~/.mux` | — | 三平台 | skills-manager |
| 22 | `neovate` | Neovate | CLI | `~/.neovate/skills` | 无（回退全局） | `~/.neovate` | — | 三平台 | skills-manager |
| 23 | `openhands` | OpenHands | CLI | `~/.openhands/skills` | 无（回退全局） | `~/.openhands` | — | 三平台 | github.com/All-Hands-AI/OpenHands |
| 24 | `pi` | Pi | CLI | `~/.pi/agent/skills` | `.pi/skills`（**不对称**） | `~/.pi/agent` | `~/.agents/skills` | 三平台 | github.com/badlogic/pi-mono（`docs/skills.md`） |
| 25 | `pochi` | Pochi | CLI | `~/.pochi/skills` | 无（回退全局） | `~/.pochi` | — | 三平台 | skills-manager |
| 26 | `zencoder` | Zencoder | CLI | `~/.zencoder/skills` | 无（回退全局） | `~/.zencoder` | — | 三平台 | skills-manager |
| 27 | `zcode` | ZCode | CLI | `~/.zcode/skills` | 无（回退全局） | `~/.zcode` | — | 三平台 | skills-manager |
| 28 | `adal` | AdaL | CLI | `~/.adal/skills` | 无（回退全局） | `~/.adal` | — | 三平台 | skills-manager |
| 29 | `deepseek_harness` | DeepSeek Harness | CLI | `~/.dsh/skills` | 无（回退全局） | `~/.dsh` | `~/.agents/skills` | 三平台 | skills-manager（`packages/util/home-paths`：`$DSH_HOME` 或 `~/.dsh`） |
| 30 | `minimax_code` | MiniMax Code | CLI | `~/.minimax/skills` | 无（回退全局） | `~/.minimax` | `~/.agents/skills` | 三平台 | skills-manager（内置 runtime skill-root resolver 验证；`.builtin-skills`/`.mavis` 不扫） |
| 31 | `devecocode` | DevEco Code | CLI | `~/.config/deveco/skills`、`~/.local/share/deveco/skills`（**双根**，均实证有真实 skills） | 无（回退全局） | `~/.config/deveco`、`~/.local/share/deveco` | — | 三平台 | **本机实证**（华为 DevEco Code，`devecocli`） |
| 32 | `cursor` | Cursor | IDE | `~/.cursor/skills` | `.cursor/skills`（对称） | `~/.cursor` | `~/.cursor/extensions` | 三平台 | cursor.com/docs（Agent Skills 官方文档） |
| 33 | `windsurf` | Windsurf | IDE | `~/.codeium/windsurf/skills` | 无（回退全局） | `~/.codeium/windsurf` | — | 三平台 | docs.windsurf.com |
| 34 | `trae` | Trae / Trae CN | IDE | `~/.trae/skills`、`~/.trae-cn/skills`（国际版/国内版双候选） | 无（回退全局） | `~/.trae`、`~/.trae-cn` | — | **仅 Win/macOS** | docs.trae.ai / docs.trae.com.cn |
| 35 | `antigravity` | Antigravity | IDE | `~/.gemini/config/skills`（v1.1 按 skills-manager 实证更新，替换 v1.0 `~/.antigravity/skills` 猜测） | `.agents/skills`（**不对称**） | `~/.gemini/antigravity` | — | 三平台 | antigravity.google/docs/skills |
| 36 | `zed` | Zed | IDE | `~/.config/zed/skills`（双候选展开） | 无（回退全局） | `~/.config/zed` | — | 三平台 | zed.dev/docs |
| 37 | `kiro` | Kiro | IDE | `~/.kiro/skills` | 无（回退全局） | `~/.kiro` | — | 三平台 | skills-manager（AWS，kiro.dev） |
| 38 | `qoder` | Qoder | IDE | `~/.qoder/skills` | 无（回退全局） | `~/.qoder` | — | 三平台 | skills-manager（阿里，qoder.com） |
| 39 | `codebuddy` | CodeBuddy | IDE | `~/.codebuddy/skills` | 无（回退全局） | `~/.codebuddy` | — | 三平台 | skills-manager（腾讯，codebuddy.ai） |
| 40 | `github_copilot` | GitHub Copilot | 编辑器插件 | `~/.copilot/skills`（v1.1 更新，替换 v1.0 指令目录 `~/.github/instructions`） | 无（回退全局） | `~/.copilot` | `~/.agents/skills` | 三平台 | skills-manager + docs.github.com/en/copilot |
| 41 | `continue` | Continue | 编辑器插件 | `~/.continue/skills`（v1.1 更新，替换 v1.0 规则目录 `~/.continue/rules`） | 无（回退全局） | `~/.continue` | — | 三平台 | skills-manager + docs.continue.dev |
| 42 | `cline` | Cline | 编辑器插件 | `~/.agents/skills`（agents 标准共享根；v1.1 更新，替换 v1.0 `~/Documents/Cline/Rules`） | 无（回退全局） | `~/.cline` | — | 三平台 | skills-manager + github.com/cline/cline |
| 43 | `roo_code` | Roo Code | 编辑器插件 | `~/.roo/skills`（v1.1 更新，替换 v1.0 规则目录 `~/.roo/rules`） | 无（回退全局） | `~/.roo` | — | 三平台 | skills-manager + docs.roocode.com |
| 44 | `kilo_code` | Kilo Code | 编辑器插件 | `~/.kilocode/skills` | 无（回退全局） | `~/.kilocode` | — | 三平台 | skills-manager（kilocode.ai） |
| 45 | `augment` | Augment | 编辑器插件 | `~/.augment/skills` | 无（回退全局） | `~/.augment` | — | 三平台 | skills-manager（augmentcode.com） |
| 46 | `junie` | Junie | 编辑器插件 | `~/.junie/skills` | 无（回退全局） | `~/.junie` | — | 三平台 | skills-manager（JetBrains，jetbrains.com/junie） |
| 47 | `gitlab_duo` | GitLab Duo | 编辑器插件 | `~/.gitlab/duo/skills`（Windows 真实位置 `%APPDATA%\GitLab\duo`，需覆写） | `.agents/skills`（**不对称**） | `~/.gitlab/duo` | `~/.agents/skills` | 三平台 | skills-manager（内置二进制 `AgentSkillsResolver` 验证） |
| 48 | `crush` | Crush | 终端 TUI | `~/.config/crush/skills`（双候选展开） | 无（回退全局） | `~/.config/crush` | — | 三平台 | github.com/charmbracelet/crush（Crush.toml 配置根确定） |
| 49 | `amp` | Amp | 终端 TUI | `~/.config/agents/skills`（v1.1 按 skills-manager 更新，替换 v1.0 `~/.amp/skills`） | 无（回退全局） | `~/.config/agents` | — | **仅 macOS/Linux** | ampcode.com（Sourcegraph Amp） |
| 50 | `warp` | Warp | 终端 TUI | `~/.agents/skills`（agents 标准共享根） | 无（回退全局） | `~/.warp` | — | 三平台 | skills-manager（warp.dev） |
| 51 | `openclaw` | OpenClaw | 桌面助手 | `~/.openclaw/skills` | 无（回退全局） | `~/.openclaw` | — | 三平台 | openclaw.ai / docs.openclaw.ai |
| 52 | `hermes` | Hermes | 桌面助手 | `~/.hermes/skills`（**递归扫描**：`recursive_scan: true`，其余 55 个工具一律非递归） | 无（回退全局） | `~/.hermes` | — | 三平台 | TASKBOOK §4.2（v1 约定候选，**待复核**） |
| 53 | `qclaw` | QClaw | 桌面助手 | `~/.qclaw/skills` | 无（回退全局） | `~/.qclaw` | — | 三平台 | skills-manager（OpenClaw 生态） |
| 54 | `easyclaw` | EasyClaw | 桌面助手 | `~/.easyclaw/skills` | 无（回退全局） | `~/.easyclaw` | — | 三平台 | skills-manager（OpenClaw 生态） |
| 55 | `autoclaw` | AutoClaw | 桌面助手 | `~/.openclaw-autoclaw/skills` | 无（回退全局） | `~/.openclaw-autoclaw` | — | 三平台 | skills-manager（OpenClaw 生态） |
| 56 | `workbuddy` | WorkBuddy / WorkBuddy AI | 桌面助手 | `~/.workbuddy/skills`、`~/.workbuddy-ai/skills`（国内版/海外版合并声明，沿 Trae 先例） | 无（回退全局） | `~/.workbuddy`、`~/.workbuddy-ai` | — | 三平台 | skills-manager |

## 形态分组（UI 侧栏 / 工具页呈现顺序）

| 形态 | 数量 | 成员 |
|------|------|------|
| CLI | 31 | claude_code, codex, gemini_cli, opencode, qwen_code, grok, omp_agent, goose, droid, deepagents, firebender, kimi, replit, bob, command_code, cortex, iflow, kode, mcpjam, mistral_vibe, mux, neovate, openhands, pi, pochi, zencoder, zcode, adal, deepseek_harness, minimax_code, devecocode |
| IDE / 桌面端 | 8 | cursor, windsurf, trae, antigravity, zed, kiro, qoder, codebuddy |
| 编辑器插件 | 8 | github_copilot, continue, cline, roo_code, kilo_code, augment, junie, gitlab_duo |
| 终端 TUI | 3 | crush, amp, warp |
| 桌面助手 | 6 | openclaw, hermes, qclaw, easyclaw, autoclaw, workbuddy |

## 链接能力（§5.2 回退链起点，按当前平台）

- Unix → Symlink
- Windows → Junction（目录链接，无需特权）
- 其余 → CopyOnly

回退链：Symlink → Junction → Copy；工具能力不足时逐级回退，最终必然收敛到 Copy。

## 待复核清单

- 仅剩 v1 约定候选项：`hermes` 的配置根与 skills 子目录（官方文档 URL 待定）。
- `gitlab_duo` 在 Windows 上的真实位置（`%APPDATA%\GitLab\duo`）需用户覆写，v1 不自动探测。
- skills-manager 来源的工具路径以该仓库当前版为准，若上游更新需同步复核。
