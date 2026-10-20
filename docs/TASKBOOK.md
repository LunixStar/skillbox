# SkillBox —— 第一版 Demo 开发任务书（投喂给编码 Agent）

> **你的角色**：资深 Rust + 桌面端架构师。你要从零搭建一个 AI Agent Skills 桌面管理器 `SkillBox`（名字可替换）。
> **你的第一优先级**：底层架构的正确性与运行开销可控。**不要先堆功能**。任何一个"以后可能会用到"的抽象，只要当前阶段没有调用点，就不许写。
> **你必须完整读完全文再动手。** 文中标注 `【硬约束】` 的条目是不可协商的验收项。

---

## 0. 项目定位

### 0.1 一句话
一个 Windows/macOS/Linux 桌面应用，把散落在几十个 AI 编码工具目录里的 Skill（`SKILL.md` 目录包）收敛成**一份正本 + N 处链接**，并用 GUI 管理安装、部署、更新与体检。

### 0.2 必须保留的内核（5 条）

| # | 内核 | 要求 |
|---|---|---|
| K1 | **中央库** | `~/.skillbox/skills/` 是唯一正本；agent 目录里只有链接 |
| K2 | **声明式 adapter 表** | 每个 agent = 一条声明，且**必须区分形态**（CLI / IDE / 编辑器插件 / 终端 TUI / 桌面助手），详见 §4 |
| K3 | **链接部署 + 覆盖授权** | 保留 `TargetState` 分类与 `ReplacePolicy` 授权模型，详见 §5 |
| K4 | **磁盘才是真相** | DB 只是索引。所有状态由磁盘 reconcile 得出，doctor/fix 向磁盘对齐，详见 §6 |
| K5 | **入库与部署分离** | `install` 只入中央库；`deploy` 才写入 agent 目录。两者永不合并 |

### 0.3 明确不做（v1 禁止实现）

【硬约束】以下功能 v1 **一行代码都不许写**，但架构上要留插件位点（见 §2.4）：

- ❌ 内置技能市场 / skills.sh 榜单 / AI 语义搜索
- ❌ Linked Workspace（任意目录当 skill 根）
- ❌ 自定义工具的新增 UI（adapter 表内置即可；v1 不提供"手动添加工具"界面）
- ❌ 系统托盘、驻留后台、开机自启
- ❌ 应用内自动更新
- ❌ 多设备 Git 同步 / 冲突合并 / 快照回滚
- ❌ 命令行 CLI（内核保持 CLI 可复用即可，v1 不发布 CLI 二进制）
- ❌ 除中英外的任何语言
- ❌ Telegram/账号/遥测/任何联网行为（除用户主动触发的 git 更新与来源链接跳转）

### 0.4 简洁性红线

【硬约束】
- 前端运行时依赖 ≤ **10** 个 npm 包
- IPC 命令 ≤ **32** 个（清单见 §8）
- SQLite 表 ≤ **7** 张（见 §3.2）
- 单个源文件 ≤ **600 行**；超过就必须拆分
- i18n 仅 `zh-CN` / `en`，共两份 JSON

---

## 1. 技术栈

| 层 | 选型 | 理由 / 约束 |
|---|---|---|
| 桌面壳 | **Tauri 2** | 不用 Electron。WebView 体积由系统提供 |
| 后端 | **Rust 2021**，workspace 多 crate | 见 §2 |
| 数据库 | **SQLite**（`rusqlite` bundled） | 只做索引，不存正文 |
| 前端 | **React 19 + TypeScript + Vite 7 + Tailwind CSS 3** | 见下方依赖白名单 |
| 编辑器 | **CodeMirror 6**（`@uiw/react-codemirror` 或自封装） | 【硬约束】**禁止 Monaco**（体积 5MB+，与低占用目标冲突） |
| 状态管理 | **TanStack Query v5** + 极少量 Context | 禁止"useState + useEffect 手动 refresh"那种写法 |
| 图标 | `lucide-react` + 自绘 agent 图标（内联 SVG） | 禁止引入图标字体 |
| i18n | 自研 ~60 行 `t()` + 两份 JSON | 禁止引入 i18next（v1 体量不值当） |
| 动画 | **纯 CSS**：`transform` / `opacity` / `backdrop-filter` / `conic-gradient` | 【硬约束】禁止 three.js / WebGL / 任何 3D 与粒子库 |

**前端依赖白名单（超出需说明理由）**：
`react` `react-dom` `@tauri-apps/api` `@tanstack/react-query` `lucide-react` `clsx` `@uiw/react-codemirror` `@codemirror/lang-markdown` `@codemirror/lang-yaml` `react-router-dom`

---

## 2. 架构护栏（最重要的一节）

### 2.1 crate 分层与依赖方向

```
crates/
├─ domain/     纯数据 + 纯逻辑，零 IO、零外部依赖
│              SkillMeta / ToolId / TargetState / ReplacePolicy / ScanFingerprint / AppError
├─ ports/      trait 抽象：FileSystem / Clock / Hasher / Linker / GitClient
│              （唯一目的：让 engine 可在 tmpfs + 假时钟下完整单测）
├─ adapters/   agent 声明表 + 路径解析，纯函数（输入 home/config_dir，输出路径）
├─ engine/     scan / reconcile / install / deploy / doctor / audit / cost
│              依赖 domain + ports + adapters
├─ store/      SQLite 索引读写 + migration
├─ ipc/        Tauri 命令层（薄：校验参数 → 调 engine → 序列化）
└─ app/        Tauri 入口：窗口、菜单、首启流程、进程生命周期
```

【硬约束】依赖方向严格单向：

```
app → ipc → engine → adapters → domain
             ↘ store ↗        ↘ ports
```

- **禁止 `engine` / `domain` / `adapters` / `store` 中出现 `use tauri::...` 或 `tauri::` 任何引用**。engine 必须能在 `cargo test` 下不链接 WebView 完整跑通。
- **禁止反向依赖**（domain 不许 import engine）。
- **禁止跨层直达**（ipc 不许直接调 adapters 拼路径，必须走 engine）。
- 每个 crate 根目录放 `README.md`，三行说明职责与允许依赖什么。

### 2.2 engine 的公共 API 形态（先定签名，再写实现）

先写 `engine/src/lib.rs` 的函数签名与文档注释，**不要先写实现**。签名需覆盖：

```rust
// 扫描：可取消、流式回调、不读文件内容
pub fn scan(req: ScanRequest, cancel: CancellationToken, on_progress: impl Fn(ScanProgress)) -> Result<ScanReport>;

// 对账：以磁盘为真相，产出差异清单（纯计算，不写盘）
pub fn reconcile(disk: &DiskView, index: &IndexView) -> Vec<Discrepancy>;

// 体检
pub fn doctor(opts: DoctorOpts) -> Result<DoctorReport>;
pub fn fix(items: &[IssueId], policy: ReplacePolicy) -> Result<FixReport>;

// 入库与部署分离
pub fn install(req: InstallRequest) -> Result<SkillId>;      // 只写中央库
pub fn deploy(skill: SkillId, tools: &[ToolId], policy: ReplacePolicy) -> Result<DeployReport>;
pub fn undeploy(skill: SkillId, tools: &[ToolId]) -> Result<()>;

// 专业面板
pub fn audit_skill(skill: SkillId) -> Result<SecurityReport>;
pub fn context_cost(tools: &[ToolId]) -> Result<CostReport>;
```

### 2.3 错误处理

沿用并简化：

```rust
pub struct AppError { pub kind: ErrorKind, pub message: String, pub details: Option<ErrorDetails> }
pub enum ErrorKind { Database, Io, Network, Git, NotFound, InvalidInput, Cancelled, Internal, TargetConflict }
```

- `TargetConflict` **必须**携带 `details.conflicts: [{ path, reason }]`，让 UI 能"指着那个目录说话"。
- 【硬约束】任何会删除用户文件的路径，禁止返回模糊的 `Io` 错误；必须能区分"被拒绝"与"失败"。

### 2.4 插件位点（只为留口子，v1 不许实现）

在 `engine` 中定义两个空 trait 与注册表即可，**不写任何具体插件**：

```rust
pub trait SkillSourceProvider { fn id(&self) -> &str; fn fetch(&self, q: &str) -> Result<Vec<RemoteSkill>>; }
pub trait WorkspaceProvider  { fn id(&self) -> &str; fn roots(&self) -> Vec<PathBuf>; }
```

### 2.5 每个阶段结束时必须自检

```
1. cargo clippy --workspace -- -D warnings         # 零警告
2. cargo test  --workspace                          # engine 层覆盖率 ≥ 70%
3. grep -rn "tauri::" crates/{domain,ports,adapters,engine,store}/ | wc -l   # 必须为 0
4. find crates -name "*.rs" | xargs wc -l | sort -rn | head -5   # 无文件 > 600 行
5. du -sh target/release/<bin>                      # 二进制 ≤ 12 MB
```

---

## 3. 数据模型

### 3.1 目录布局

```
~/.skillbox/
├─ skills/                中央库（唯一正本）
├─ trash/<timestamp>/     删除暂存区（v1 内删除一律先进这里，可恢复）
├─ cache/                 扫描指纹缓存、git 拉取缓存（可一键清空）
├─ logs/                  日志（可一键导出 zip）
└─ skillbox.db            SQLite 索引
```

【硬约束】`skillbox.db` 中**禁止**存放 skill 正文、frontmatter 全文（只存 name/description 摘要）、任何密钥。

### 3.2 SQLite（v1 = 6 张表 + migration 框架）

| 表 | 字段要点 |
|---|---|
| `skills` | id, name, description, `origin_kind`(local/git/archive), origin_ref, origin_subpath, origin_revision, canonical_path UNIQUE, content_hash, created_at, updated_at |
| `skill_targets` | skill_id, tool_id, path, `mode`(symlink/junction/copy), `state`, synced_at, source_hash — UNIQUE(skill_id, tool_id) |
| `skill_tags` | skill_id, tag — PK(skill_id, tag) |
| `scan_cache` | path PK, fingerprint, last_seen_at |
| `settings` | key PK, value |
| `audit_log` | id, at, kind, target, message |

`migrate.rs` 从 v0 起就有版本表与 `run_migrations()`，v1 只写一个 `migrate_v0_to_v1`。

### 3.3 领域类型（domain crate）

```rust
pub enum FormFactor { Cli, Ide, EditorPlugin, TerminalTui, DesktopAssistant }
pub enum LinkCapability { Symlink, Junction, CopyOnly }
pub enum SyncMode { Symlink, Copy }
pub enum TargetState { Absent, LinkToSource, ForeignLink, RealDir, RealFile }
pub enum ReplacePolicy<'a> { NoClobber, Recorded { mode: &'a str }, UserConfirmed }
pub enum OriginKind { Local, Git, Archive }
```

---

## 4. Adapter 表（形态化 —— 本项目相对原版的核心改进）

### 4.1 声明结构

```rust
pub struct ToolAdapter {
    pub id: &'static str,               // "claude_code"
    pub display_name: &'static str,     // "Claude Code"
    pub form_factor: FormFactor,        // ★ 形态
    pub platforms: PlatformMask,        // Win/Mac/Linux 位掩码
    pub global_skills_dirs: &'static [&'static str],   // 相对 home，可多个候选
    pub detect_paths: &'static [&'static str],         // 存在即认为"已安装"
    pub project_skills_dir: Option<&'static str>,      // 项目级，None 则回退全局相对路径
    pub extra_scan_dirs: &'static [&'static str],      // 只发现、不部署（如插件缓存目录）
    pub recursive_scan: bool,           // 嵌套分类目录（如 Hermes）
    pub icon: &'static str,             // 内置 SVG id
    pub docs_url: &'static str,
}
```

### 4.2 v1 首批内置 19 个，按形态分组

【硬约束】**必须按形态分组呈现给 UI**，侧栏与工具页都要有"形态"分节/筛选，这是本项目与原版的关键差异。

| 形态 | 工具 |
|---|---|
| **CLI** | Claude Code（`~/.claude/skills`）、Codex（`~/.codex/skills`）、Gemini CLI（`~/.gemini/skills`）、OpenCode（`~/.config/opencode/skills`，项目级 `.opencode/skills`）、Qwen Code、Grok |
| **IDE / 桌面端** | Cursor（`~/.cursor/skills`）、Windsurf、Trae / Trae CN、Antigravity、Zed（`~/.config/zed/...`） |
| **编辑器插件** | GitHub Copilot（`~/.github/instructions`）、Continue（`~/.continue/rules`）、Cline（`~/Documents/Cline/Rules`）、Roo Code（`~/.roo/rules`） |
| **终端 TUI** | Crush、Amp |
| **桌面助手** | OpenClaw、Hermes（`recursive_scan: true`） |

- 每个 adapter 的注释里**必须写清该路径的来源依据**（官方文档/仓库路径），并注明是否存在"全局与项目级路径不对称"的情形。
- 路径解析支持 `.config/` 前缀自动映射到 `dirs::config_dir()`（Linux XDG）。
- `is_installed()` = `detect_paths` 任一存在；路径被用户显式覆写时视为已安装。

### 4.3 插件/缓存类目录的处理

`extra_scan_dirs` 扫到的 skill **只允许"发现与登记"，不允许部署**（部署目标不明确）。UI 上要标注"来源：插件缓存（只读）"。

---

## 5. 链接部署与覆盖授权（K3）

### 5.1 目标状态分类

【硬约束】必须使用 `symlink_metadata()`，**禁止用 `is_dir()` 判定**（`is_dir()` 会跟随链接，把 Windows junction / 目录 symlink 误判为真实目录，进而走 `remove_dir_all`，造成数据丢失）。

| TargetState | 判定 | 可否被覆盖 |
|---|---|---|
| `Absent` | 不存在 | ✅ |
| `LinkToSource` | 链接且指向本次要部署的源 | ✅ |
| `ForeignLink` | 链接但指向别处 / 悬空 | ❌（用户配置，需 UserConfirmed） |
| `RealDir` | 真实目录 | ❌（与 copy 模式部署无法区分，需 UserConfirmed） |
| `RealFile` | 真实文件 | ❌ |

### 5.2 授权策略

```rust
pub enum ReplacePolicy<'a> {
    NoClobber,                      // 只允许在"不可能损失用户数据"时写入
    Recorded { mode: &'a str },     // DB 中有 skill_targets 记录且当前类型与记录模式相符
    UserConfirmed,                  // 用户显式确认（adopt / 强制覆盖 / 从中央库更新副本）
}
```

- 【硬约束】用枚举而非 bool：新增调用点时编译器强制表态。
- `Recorded{mode:"symlink"}` **绝不授权删除**当前是 `RealDir` 的目标（DB 记录只代表历史意图，不代表磁盘现状）。
- 平台能力回退顺序：**symlink → junction（Windows）→ tracked copy**。退化为 copy 时必须在目标目录写入 `.skillbox-source.json` 记录源路径，且 UI 明确标注"副本模式（非实时同步）"。

### 5.3 部署写入纪律

- 先写临时目录 → 原子 rename（同一卷内）。
- 每个会删除的路径，删除前先 `trash/` 暂存（v1 内不做硬删除）。
- 所有写操作执行前 `mute` 文件监听，避免自触发。

---

## 6. 磁盘优先：reconcile / doctor / fix（K4）

### 6.1 唯一真相链

```
磁盘现状 ──reconcile──> 差异清单 ──> UI 展示 ──用户触发──> fix ──> 写盘 ──> 重扫校验
                                        ↑
                              DB 索引只作为"上次意图"参与比对
```

【硬约束】任何 UI 上的"已启用/未启用/同步异常"**必须由磁盘推算**，禁止直接读 DB 字段展示。DB 字段只用于回答"我们上次想做什么"。

### 6.2 Discrepancy 类型（v1）

| 类型 | 含义 | 建议修复 |
|---|---|---|
| `UnmanagedRealDir` | agent 目录里有真实目录，非我们部署 | 提供 adopt（移入中央库 + 原位放链接） |
| `BrokenLink` | 链接悬空 | 重建或移除 |
| `ForeignLink` | 链接指向中央库之外 | 提示，不自动改 |
| `MissingTarget` | DB 记录已部署，磁盘上没有 | 重新部署 |
| `UnrecordedDeployment` | 磁盘上有我们风格的链接，DB 无记录 | 补登记 |
| `CopyDrift` | copy 模式副本与正本内容哈希不一致 | 从正本更新 / 转 symlink |
| `DuplicateName` | 多个 agent 目录同名但内容不同 | 高亮，让用户选正本 |

### 6.3 doctor / fix 要求

- `doctor()` **只读**，可在任何时候安全运行，返回带 `severity` 的问题清单与建议动作。
- `fix()` 接受**显式选中的问题 id 列表**，逐项执行，返回逐项结果（成功/跳过/失败+原因）。
- 【硬约束】`fix()` 禁止"一键修复全部"作为默认行为；危险项（涉及 `RealDir` / `ForeignLink`）必须逐个二次确认。

---

## 7. 扫描引擎性能约束（本项目的技术护城河）

【硬约束】以下每一条都必须落实，并在 `engine/src/scan.rs` 顶部注释中逐条标注行号：

1. **不读文件内容**：候选目录判定只看 `read_dir` + `symlink_metadata`；`SKILL.md` 只在确定为 skill 目录后读取，且**截断读取前 8 KB**（frontmatter 不会超过）。
2. **深度上限**：递归深度 ≤ 6，超出不再深入。
3. **黑名单**：`node_modules .git target dist build venv .venv __pycache__ .cache .next Library AppData` 等，**用户可追加**。
4. **不跟随符号链接**：防止循环与越权。
5. **跳过网络卷 / 可移动卷**（Windows 用 `GetDriveType`，Unix 跳过 `/proc /sys /dev /mnt` 下非本地挂载）。
6. **增量指纹**：`(mtime, size, 子项数)` 未变则直接复用 `scan_cache`，不重新解析。内容哈希只算 ≤ 1 MB 的文件，超限用 mtime+size 近似并标记。
7. **并发上限**：`min(num_cpus, 4)` 个工作线程；I/O 密集，禁止无界 `spawn`。
8. **可中断**：`CancellationToken`，每个目录检查一次，取消后 200 ms 内返回已扫到的部分结果。
9. **流式进度**：每处理 50 个目录 emit 一次 `scan://progress`（已扫/总数/当前路径），UI 增量渲染，禁止"扫完才显示"。
10. **内存上限**：扫描期间峰值 RSS 增量 ≤ **80 MB**；禁止把整个 skill 树结构一次性装进内存后返回，分批产出。
11. **无网络**：扫描过程禁止任何 HTTP / git 操作。

### 7.1 性能预算（验收硬指标）

| 指标 | 目标 |
|---|---|
| 冷启动到窗口可见 | ≤ 1.5 s |
| 空闲 RSS | ≤ 150 MB |
| 扫描 5000 个候选目录 | ≤ 60 s，可中断，RSS 增量 ≤ 80 MB |
| 单次 IPC 往返（list 500 条） | ≤ 50 ms |
| release 二进制 | ≤ 12 MB |
| 安装包（不含 WebView 运行时） | ≤ 20 MB |

`crates/engine/benches/scan_bench.rs` 用 `criterion` 或简单计时，对 1000/5000/20000 目录三档做基准，`README` 记录基线数字。

---

## 8. IPC 命令面（≤ 32 个，资源化分组）

```
scan      : start / cancel / status
library   : list / get / install_local / install_git / remove   (remove 需 mode: undeploy|delete)
deploy    : apply / revoke / set_mode
tools     : list / set_enabled
doctor    : run / fix
doc       : read / write / validate_meta
security  : scan / report
cost      : report
system    : open_path / logs_export / cache_inspect / cache_clear / trash_list / trash_restore
settings  : get / set
updates   : check / apply
```

【硬约束】
- 所有返回给前端的结构必须 `Serialize` + `#[serde(rename_all = "camelCase")]`。
- 写操作统一支持 `dry_run: bool`，UI 的批量操作必须先 dry-run 展示影响清单。
- `system.open_path` 只接受由后端生成的绝对路径，禁止前端自由传参（防路径穿越）。

---

## 9. 专业功能面板（集中在一个 "工具箱" 页面）

### 9.1 安全扫描 `audit_skill`

静态规则表（每条给出 id、严重度、命中说明）：

| 规则 | 检测内容 |
|---|---|
| `shell_exec` | `SKILL.md` 或 scripts/ 中出现 `bash/sh -c`、`: \| bash`、PowerShell 执行 |
| `network_egress` | `curl`/`wget`/`Invoke-WebRequest`/`requests.post`/外发 URL |
| `credential_access` | 读取 `.env`、`~/.ssh`、`~/.aws`、keychain、`process.env.*_TOKEN` |
| `path_traversal` | `../` 越出 skill 目录、绝对路径写入库外 |
| `obfuscation` | base64 解码后执行、长十六进制串、`eval(` |
| `symlink_escape` | skill 包内含指向外部的符号链接 |
| `hidden_files` | 隐藏目录/文件的意外存在 |
| `excessive_scope` | frontmatter 声明了过宽的权限或工具 |

- 输出：`SecurityReport { risk_level, findings: [{rule_id, severity, file, line, snippet, advice}] }`
- UI：卡片上显示风险徽章；详情页列出逐条发现与建议；**只提示，不阻断安装**。
- 【硬约束】扫描器不得执行 skill 内任何脚本，纯静态文本匹配。

### 9.2 上下文成本面板 `context_cost`

- 统计每个已启用 tool 当前常驻的 skill 数量、`description` 总字符数，按 **~3.5 字符/token**（明确标注为估算）折算 token。
- 触发词冲突检测：对 `description` 做分词，计算两两 Jaccard 相似度 ≥ 0.35 的组，标为"触发冲突风险"，并列出重叠关键词。
- 展示：总览数字卡片 + 按 tool 分组的条形图（纯 CSS/SVG，禁止图表库）+ 冲突组列表 + 一键"停用该 skill"。

### 9.3 编辑器 + 元数据校验

- CodeMirror 6 编辑 `SKILL.md`，Markdown + YAML frontmatter 高亮。
- 保存时校验：frontmatter 必须有 `name`（且符合 `^[a-z0-9][a-z0-9-]{1,63}$`）与 `description`；`description` 长度建议 20–200 字符；未闭合代码块、缺失引用文件（`scripts/`、`references/` 路径存在性）给出警告。
- 写入前自动写 `.bak`，编辑历史仅保留最近 1 份。

### 9.4 其他专业入口

| 入口 | 行为 |
|---|---|
| 日志导出 | 打包 `logs/` + 最近 500 条 `audit_log` 为 zip，保存到用户选择位置 |
| 缓存检查与清除 | 列出 `cache/` 各项体积，支持单项/全部清除 |
| 打开本地目录 | 在系统文件管理器中打开中央库 / 某 skill / 某 agent 目录 |
| 重新扫描 | 一个按钮，带进度条，可取消 |
| 溯源 | 展示来源类型、仓库 URL / 本地路径、子路径、revision，一键打开浏览器或文件夹 |
| 更新 | git 来源 `fetch + 比对 revision`；本地来源"重新导入"。更新前展示 diff 摘要 |

---

## 10. 首启开箱即用流程（面向非专业用户）

【硬约束】**首启不得移动或删除任何用户文件。**

1. 窗口打开 → 展示欢迎页（两句话说明"本工具做什么"+ 一个"开始扫描"按钮）。
2. 点扫描 → 自动探测所有 adapter 的 `detect_paths` 与 `global_skills_dirs`，流式显示"发现 Claude Code ×12、Codex ×8…"。
3. 扫描结束 → 展示"发现 N 个 Skill，来自 M 个工具"，默认行为是**原地登记（keep-in-place）**：把每个已存在的 skill 目录登记为中央库条目，其 `canonical_path` 指向首次发现的位置。
4. 提供**可选**的一步"统一到中央库"：明确告知"将把这些目录移动到 `~/.skillbox/skills/`，并在原位置建立链接；操作前会创建快照"。**默认不勾选**。
5. 进入主界面，侧栏显示工具（按形态分组），主区显示中央库。

【硬约束】删除语义分两级，UI 必须区分：
- **取消部署**（从某 agent 移除链接）— 正本仍在中央库
- **删除 Skill**（移入 `trash/`，可恢复）— 需二次确认并说明影响范围

禁止任何未加限定词的"删除"按钮直接删文件。

---

## 11. GUI 设计规范

### 11.1 视觉：PlainCraftLauncher2 式简洁白蓝

| 项 | 取值 |
|---|---|
| 页面底色 | `#FFFFFF` / 内容区 `#F6F8FB` |
| 主色蓝 | `#3B8EF0`（hover `#2F7FD8`，按下 `#256FC4`） |
| 淡蓝底 | `#E8F2FF`（选中态、标签底） |
| 边框 | `#E3E9F0`，1px |
| 卡片 | 白底 + 圆角 10px + `0 1px 3px rgba(16,40,80,.06)` |
| 文本 | 主 `#1B2430`，次 `#5A6675`，弱 `#8A94A3` |
| 危险 / 成功 | `#E5544B` / `#2FA46A` |
| 字体 | `-apple-system, "Segoe UI", "Microsoft YaHei", "PingFang SC", sans-serif`，正文 14px |
| 圆角 | 卡片 10px，按钮 8px，标签 999px |

- 顶部一条 2px 渐变高光条（蓝→青，`linear-gradient(90deg,#3B8EF0,#56C8F0,#3B8EF0)`），缓慢横向流动 —— 这就是要求的"光追感"，**纯 CSS 实现**。
- 卡片 hover：`translateY(-2px)` + 阴影加深 + 边缘出现一圈极淡的蓝色 `conic-gradient` 描边。
- 弹层 / 侧栏抽屉使用 `backdrop-filter: blur(8px)` 的玻璃拟态。
- 侧栏选中项左侧 3px 蓝色指示条。

### 11.2 动画纪律

- 时长：微交互 120 ms，页面/弹层 200 ms，`cubic-bezier(.2,.8,.2,1)`。
- 【硬约束】**只动画 `transform` 与 `opacity`**；禁止动画 `width/height/top/left/box-shadow`（会触发布局与重绘）。
- 【硬约束】必须实现 `@media (prefers-reduced-motion: reduce)` 关闭全部非必要动画。
- 列表项进入用错峰 fade-up（间隔 20 ms，最多 10 项，避免长列表卡顿）。

### 11.3 信息架构（简洁，但专业入口集中）

```
┌─────────────┬──────────────────────────────────────────┐
│ 概览         │  顶部：搜索框 · 视图切换 · 扫描按钮        │
│ 技能库 ★     │──────────────────────────────────────────│
│ 工具          │  主区：卡片网格 / 列表                    │
│  ├ CLI       │                                          │
│  ├ IDE/桌面  │  卡片：名称 · 描述 · 来源徽章 · 风险徽章   │
│  ├ 编辑器插件│        · agent 角标（点击即部署/撤销）     │
│  ├ 终端 TUI  │                                          │
│  └ 桌面助手  │                                          │
│ 体检         │                                          │
│ 工具箱 ▼     │                                          │
│  安全扫描    │                                          │
│  上下文成本  │                                          │
│  日志与缓存  │                                          │
│ 设置         │                                          │
└─────────────┴──────────────────────────────────────────┘
```

- 侧栏**只暴露 5 个顶层项**：概览 / 技能库 / 工具 / 体检 / 设置；"工具箱"是设置同级的一个可折叠分区，把安全扫描、上下文成本、日志与缓存**全部收进去**，主界面不被专业功能污染。
- 卡片上的 agent 角标是最高频操作（点一下即部署/撤销），必须常驻可见，不许塞进菜单。
- 长列表（>300 项）必须虚拟滚动（自研 ~80 行即可，禁止引入虚拟列表库）。
- 顶部命令面板（Ctrl+K）作为专业用户的快捷入口，覆盖全部 IPC 写操作。

---

## 12. 分阶段任务清单

> 每个阶段结束必须跑 §2.5 自检，全部通过才能进入下一阶段。**未通过就继续修，不许带着债往下走。**

### Phase 0 —— 骨架与契约（不写业务）
1. 建立 workspace 与 7 个 crate，写入 §2 的依赖方向约束（`cargo` 依赖声明本身就体现方向）。
2. 在 `domain` 中定义 §3.3 全部类型；在 `ports` 中定义 trait；在 `engine/src/lib.rs` 中**只写 §2.2 的函数签名 + 文档注释，实现体 `todo!()`**。
3. 建 SQLite migration 框架与 v0→v1 的 6 张表。
4. Tauri 窗口能起来，渲染一个空壳页面。
- **DoD**：`cargo clippy -D warnings` 通过；`grep tauri::` 在 engine/domain/adapters/store 中为 0；窗口可启动。

### Phase 1 —— 内核（纯 Rust，可脱离 GUI 测试）
5. `adapters`：19 个工具声明表 + 路径解析 + 形态分类，附单测（含 `.config/` XDG 映射、覆写路径）。
6. `scan`：实现 §7 全部 11 条约束，写 `scan_bench` 与 1000/5000/20000 三档基线。
7. `deploy`：`TargetState` 分类表 + `ReplacePolicy` 决策表，用 `tempfile` 做真实文件系统单测，**必须包含"DB 记录 symlink 但磁盘是 RealDir 时不得删除"这一用例**。
8. `reconcile` + `doctor` + `fix`：§6.2 的 7 类差异全部覆盖，每类至少一个单测。
- **DoD**：engine 单测通过且覆盖率 ≥ 70%；性能预算表全部达标；`fix()` 在危险项上必须逐个确认。

### Phase 2 —— IPC 薄层
9. `ipc`：实现 §8 的 32 个命令，**只做参数校验 + 调 engine + 序列化**，无业务逻辑。
10. 所有写命令支持 `dry_run`；`TargetConflict` 携带冲突路径。
- **DoD**：每个命令可用 Tauri dev 手动验证；命令数 ≤ 32。

### Phase 3 —— GUI 主流程（开箱即用）
11. 设计系统落地：§11.1 全部色值与组件（卡片、按钮、标签、徽章、弹层、抽屉）。
12. 侧栏 + 路由 + 首启流程（§10 五步）。
13. 技能库页：卡片网格 / 列表切换、搜索、来源与标签筛选、agent 角标部署。
14. 工具页：按形态分组，启停、路径展示、一键打开目录。
15. 详情页：SKILL.md 预览、来源溯源、更新（git/本地）、安全徽章、部署状态。
- **DoD**：全新机器（模拟多 agent 目录）走完首启无需任何专业知识；所有删除操作均有两级确认。

### Phase 4 —— 专业面板
16. 体检页：问题清单、按严重度分组、逐项 fix、dry-run 预览。
17. 工具箱：安全扫描（§9.1 八条规则）、上下文成本（§9.2）、日志导出与缓存管理（§9.4）。
18. 编辑器：CodeMirror 6 + frontmatter 校验（§9.3）。
- **DoD**：三类面板可独立运行；安全扫描不执行任何 skill 脚本（写单测证明）。

### Phase 5 —— 打磨
19. 动画与光效（§11.2），含 reduced-motion。
20. 虚拟滚动、命令面板（Ctrl+K）。
21. 空状态、错误态、加载骨架屏；中英文案全部就位。
22. 性能复测：对照 §7.1 表格逐项记录实际数字，写入 `PERFORMANCE.md`。
- **DoD**：全部硬指标达标；中英切换无漏翻（写个脚本比对两份 JSON 的 key）。

---

## 13. 交付物

1. 可运行的 Tauri 应用（三个平台的构建脚本，v1 至少保证一个平台可跑通）
2. `crates/**` 全部源码 + engine 层单测
3. `README.md`：架构图（crate 依赖方向）、首启流程、性能基线表
4. `PERFORMANCE.md`：§7.1 每项的实际测量值
5. `docs/adapters.md`：19 个工具的路径依据与形态说明
6. `docs/security-rules.md`：八条规则的匹配模式与误报说明

---

## 14. 最后三句话，动手前再读一遍

1. **engine 不许碰 Tauri**。它要能脱离 GUI 完整测试，这是底层不崩的前提。
2. **磁盘是真相**。任何从 DB 直接读出来展示给用户看的状态，都是 bug 的来源。
3. **先签约、后实现**（Phase 0 的 `todo!()` 不是偷懒，是先把边界钉死）。功能不够可以后加，架构歪了要推倒重来。
