# SkillBox 开发进度

## Step 13 收尾（Phase 5 Final）：动画收尾 + 命令面板 Ctrl+K + dev 路由守卫 + §7.1 性能实测 + 交付文档

状态：完成

做了：
- **动画收尾（§11.2）**：
  - `index.css` reduced-motion 块补 `animation-delay: 0ms !important`——错峰入场在 reduced-motion 下彻底关闭。
  - LibraryPage renderCard/renderRow、DoctorPage renderIssue 加 `sb-fade-up` + `--sb-i`（CSS `min()` 截断 10 项），列表 >300 行虚拟滚动（VirtualList）与弹层过渡（Modal sb-pop-enter / Drawer sb-fade-enter）此前已就位。
- **命令面板 Ctrl+K（§11.3）**：`components/CommandPalette.tsx`（新，~470 行）：
  - 覆盖全部无复杂参数 IPC 写操作：rescan（scan_start）/ scan_cancel / doctor 重跑 / 安全全扫（逐 skill securityScan）/ 打开中央库 / 切语言 / 登记本地目录（library_install_local keepInPlace 弹窗）/ 逐 skill 部署（deploy_apply）/ 撤销（deploy_revoke）/ 模式切换（deploy_set_mode，经 ipc.ts 新增 `deploySetMode` 直连封装）/ 删除（library_remove delete，**两段 Enter 确认**）/ 逐工具启停（tools_set_enabled）+ 6 视图导航。
  - 键盘 ↑↓/Enter/Esc；过滤 label+group+keywords；渲染与导航共用 slice(0,50) 截断集；busy 态；危险命令 `confirmId` 二次 Enter（不引入 window.confirm）。
  - **例外备案**（需页面上下文，保留原页面流）：doc_write（编辑器内容）、doctor_fix（dry-run 影响清单 + 逐项确认）、updates_apply（diff 确认）。
  - MainShell 挂 `<CommandPalette>` + 全局 Ctrl+K/Cmd+K keydown 切换；palette.* 共 30 key 补入 zh-CN/en 两份 JSON。
- **dev 路由守卫**：MainShell `const IS_DEV = import.meta.env.DEV`（Vite 静态替换）门控 design/dev 两视图渲染；Sidebar 加 `showDev` prop 门控开发者入口显示。release 构建下两页面**不可达**（备案：静态 import 仍在 bundle 内，tree-shake 需动态导入，v1 接受）。
- **lint 接线**：`package.json` += `"lint": "node scripts/check-i18n.mjs && tsc --noEmit"`（check-i18n 正式进 lint 链）。
- **§7.1 性能实测（docs/PERFORMANCE.md 全表真实数字）**：
  - 新增 `crates/store/benches/list_bench.rs`：500 skill 灌库 + 20 轮 list_skills/serde 计时——**list 500 条命令体+序列化 min 0.49ms / avg 1.15ms**（预算 ≤50ms；invoke 信封未计，口径备案）。
  - 重跑 scan_bench：5000 目录冷 124ms / 暖 99ms，RSS 增量 1544KB（预算 ≤60s / ≤80MB）。
  - 重跑 db_cache_bench：5000 目录 DbCache 冷 163ms / 暖 105ms；20000 目录冷 1299ms。
  - PowerShell 实测 release exe：冷启动（主窗口出现）首启 **1181ms** / 后续 216-254ms（预算 ≤1.5s）；空闲 RSS **26.8-30MB**（预算 ≤150MB）。
  - release 体积 **7.60MB ≤ 12MB**；安装包 N/A（NSIS bundle 从未构建，如实记录）。
- **交付文档**：
  - `docs/adapters.md`（新）：19 工具全表（路径依据 / 全局-项目级对称性 / 平台差异 / extra_scan 只读目录 / 待复核清单）+ 解析规则 + 形态分组 + 链接能力回退链。
  - `docs/security-rules.md`（新）：八条规则逐条匹配模式 + 严重度 + 误报说明 + 扫描上限 + risk level 汇总；「纯静态零执行」硬约束的结构性保证说明。

自检输出（真实结果）：
1. `npm run lint`（check-i18n + tsc）→ `zh=311 en=311 used=244 failures=0`
2. `npm run build` → 通过（5.32s，2064 modules）
3. `cargo clippy --workspace --all-targets -- -D warnings` → Finished in 1.07s，零警告
4. `cargo test --workspace` → 205 passed / 0 failed（adapters 23、domain 6、engine 154、ports 9、store 13）
5. 全部 .rs ≤ 600 行（59 文件，含新 list_bench.rs；0 超限）；全部 tsx/ts ≤ 600 行（39 文件，0 超限）
6. 核心五 crate `tauri::` grep → 0；IPC 命令 32 个 = §8 上限；npm 依赖 8 个 ≤ 10
7. touch main.rs + `cargo build --release -p skillbox-app` → **7.60 MB ≤ 12 MB**（1m27s）
8. release exe 冷启动/RSS 三轮实测 → 1181/216/254ms，RSS 26.8-30MB（见 PERFORMANCE.md）

遗留风险 / 备案：
- 命令面板命令集随数据规模增长（每 skill 最多 2×工具数条命令）：>300 skill 时过滤前命令数大，但过滤/slice(0,50) 渲染有界，min/截断保证 UI 不卡。
- palette 安全全扫为顺序 Promise.all（skill 极多时耗时），busy 态明示执行中；未做进度流（v1 接受）。
- dev 页静态 import 进 release bundle（不可达但增大 JS ~几十 KB）；v2 可改动态 import() 彻底剔除。

## 已知问题与未做事项（v1 收尾清单）

已知问题（含误报/取舍，均不阻塞交付）：
1. dev 页（DesignSystemPage/IPCTester）代码在 release bundle 内但运行时不可达（IS_DEV 守卫）；v2 动态 import 可剔除。
2. check-i18n unused-key 告警（约 62 条 + palette 后新增）均为 Record 映射/三元假阳性，脚本只扫 `t("字面量")`。
3. doc 校验误报情形（unclosed_fence 教学示例、missing_file 裸域名链接等）——Step 12 详列，告警级不阻断。
4. 静态安全规则的保守误报（文档中 shell 示例文本等）——只提示不阻断，见 docs/security-rules.md 逐条说明。
5. 前端只读来源判定为路径前缀匹配（体验层）；权威判定在后端 deploy 守卫。
6. 扫描事件经 Tauri event 的重放窗口由 1.2s 轮询 scan_status 兜底。
7. Windows 无开发者模式环境走 junction；有开发者模式时 symlink，两者分类等价（测试钉死）。
8. hash_dir >1MB 文件近似哈希，极端构造下 CopyDrift 理论可漏报。

未做事项（v2 候选，非 v1 承诺范围）：
1. NSIS 安装包构建与体积实测（tauri.conf.json targets 已配 ["nsis"]，工具链未构建过）。
2. 回收站恢复 UI、缓存管理 UI、日志导出 UI（IPC 均已有：trash_restore/cache_clear/system_logs_export）。
3. 标签编辑（skill_tags 表无写入来源，标签过滤入口保留诚实空态）。
4. 更新/体检的定时自动检查（v1 全手动触发，防后台惊喜）。
5. WatchMute 真实文件监听（端口已预留）。
6. adapters 表中标注【待复核】的路径核验（见 docs/adapters.md 待复核清单：codex/gemini_cli/qwen_code/grok/windsurf/trae/antigravity/crush/hermes 的 skills 子目录等）。
7. 命令面板对 doc_write/doctor_fix/updates_apply 的覆盖（需页面上下文，保留原页面流，已备案）。
8. 多窗口 / 多显示器适配（v1 单窗口）。

## Step 13 i18n（Phase 5）：自研 t() + zh-CN/en 双语 + 语言切换 + 设置页实装

状态：完成

做了：
- **i18n 核心**（禁 i18next，§0.4）：`src/i18n/index.tsx`（新）——`I18nProvider`（挂载时经 settings_get 读 `language` 键；`setLang` 时 settings_set 持久化 + 同步 `document.documentElement.lang`）、`useI18n()` → `{ lang, setLang, t }`、`t(key, params?)` 扁平点分 key + `{x}` 插值，回退链 当前语言 → zh-CN → key 本身。`main.tsx` 接线：QueryClientProvider 内、ToastProvider 外。
- **词表**：`src/i18n/zh-CN.json` + `en.json` —— 各 **281** 个扁平 key（common/sidebar/overview/tools/library/detail/security/doctor/toolbox/firstrun/settings 前缀分组），两份 key 集合一致。
- **比对脚本**：`scripts/check-i18n.mjs`（新）——双向 key 集合比对（missing-en/missing-zh 为 error）、源码 `t("...")` 字面量扫描（undefined-key 为 error）、unused-key 告警；`package.json` += `check:i18n`。
- **全量文案替换**（14 个文件）：Sidebar（NavItem.labelKey）、OverviewPage、ToolsPage（GROUP_LABEL_KEY/FORM_SHORT_KEY Record 重构）、SecurityKit（RiskBadge/FindingList 组件内 useI18n + RULE_LABEL_KEY 映射，未知 ruleId 回退 id）、FirstRun（全部五步）、DoctorPage（SEVERITY_LABEL_KEY/KIND_TITLE_KEY/IssueAction.labelKey 重构，locale 按 lang 取 zh-CN/en-US）、ToolboxPage、LibraryPage（搜索/过滤/角标/两级删除确认/重扫弹窗全套接 t）、SkillDetailPage（编辑/校验/更新/部署/溯源/确认弹窗）、ui 基础组件 Drawer/Tag/Toggle（三处硬编码 aria-label 一并接 t）。
- **拆分（600 行上限）**：SkillDetailPage **617 → 555**（底部问题列表拆出 `components/DocIssueList.tsx`，DOC_RULE_LABEL_KEY 映射随迁）；LibraryPage **631 → 590**（扫描对话框拆出 `components/ScanModals.tsx`：ScanResultModal / ScanRunningModal / TagFilterModal）。
- **设置页实装**：新建 `pages/SettingsPage.tsx`（语言分段切换 简体中文/English + 中央库说明 + 打开按钮）；MainShell 去 PLACEHOLDERS 占位；删除 `pages/PlaceholderPage.tsx`。

没做：
- dev 页（DesignSystemPage / IPCTester）不 i18n —— 内部开发者工具，在此备案。
- 语言选项名「简体中文」/「English」按惯例各自原文显示，不互译。
- 代码注释保持中文（工程约定，非 UI 文案，不在 DoD「无漏翻」范围）。

自检输出（真实结果）：
1. `npm run check:i18n` → `zh=281 en=281 used=214 failures=0`
2. `npm run build`（tsc --noEmit + vite）→ 通过（built in 2.88s，2062 modules）
3. `cargo clippy --workspace --all-targets -- -D warnings` → Finished in 0.79s，零警告
4. `cargo test --workspace` → 205 passed / 0 failed（adapters 23、domain 6、engine 154、ports 9、store 13）
5. 全部 .rs ≤ 600 行（58 文件，0 超限）；全部前端 tsx/ts ≤ 600 行（0 超限）
6. 核心五 crate `tauri::` grep → 0；IPC 命令 32 个（crates\ipc）= §8 上限
7. touch main.rs + `cargo build --release` → **7.60 MB ≤ 12 MB**（42.79s）
8. release exe 启动存活验证 → ALIVE（pid=4212，5 秒）后正常关闭

遗留风险 / 备案：
- 语言持久化复用 settings 表 `language` 键（settings_get/set 既有 IPC，**零新增命令**）；读取失败回退 zh-CN；持久化失败不阻断会话内切换。
- check-i18n 的 unused-key 告警共 62 条，均为 Record 映射 / 三元引用经非字面量路径触达的假阳性（脚本只扫 `t("字面量")`）；已人工核对无真实漏用。
- verify_ui 不可用（仅支持 HarmonyOS 工程，本项目为 Tauri 桌面应用）；语言切换经 build + 启动冒烟验证。

## Step 12 SKILL.md 编辑器（§9.3）：CodeMirror 6 + 元数据校验 + .bak + 增量重扫

状态：完成

做了：
- **后端——校验行号化**（`engine/src/doc.rs` 重写，测试拆至 `doc/tests.rs`）：
  - `DocIssue` 增加 `line: u32`（1 起始，0 = 无确切行）——行内标记数据源。
  - `check_text` 行级重写：`name:`/`description:` 键定位到所在行；无 frontmatter 时锚点第 1 行；未闭合 ``` 围栏用**行级开合配对**（行首 ``` 切换）定位到未闭合围栏的**起始行**（不再整段奇偶计数）；缺失引用文件逐行提取 `](target)` 定位到引用行（同一目标只报首次）。
  - 新增 `name_mismatch` 告警：frontmatter name 与登记名不一致时提示（索引与目录名不随编辑改名；keep-in-place 场景可能本来不同，故为告警非错误）。
  - 新增 `validate_text`（公开入口）：校验编辑器中的**任意文本**，零写盘——实时校验数据源；`validate_meta` 增加 `expected_name` 参数。
  - `doc_write` 拒绝信息带行号（`（第 N 行）`）。
- **后端——增量重扫**（§9.3「编辑保存后触发一次该 skill 的增量重扫，不要全量」）：
  - `rescan_after_doc_save`：只重算该 skill 的 frontmatter description + 内容哈希（`hash_dir`，≤1MB 文件精确）→ `index_store.refresh_skill` 刷新索引行；name / canonical_path / origin 不动；幂等（内容未变哈希稳定，有测试断言）。
  - `DocWriteOutcome` 增加 `index_refreshed` / `index_error`（保存已成功、仅索引刷新失败时不谎报保存失败）。
- **后端——IPC**（复用既有命令槽位，命令总数 32 不变）：
  - `library_doc_validate` 增加 `contents: Option<String>`：给定时校验编辑器文本（实时），省略时校验盘上内容；两者均带登记名做 name_mismatch 告警。
  - `library_doc_write`：写入成功（非 dry_run）后自动触发 `rescan_after_doc_save`。
- **前端**：
  - 依赖（§1 白名单内，**未引入 Monaco**）：`@uiw/react-codemirror` + `@codemirror/lang-markdown` + `@codemirror/lang-yaml`；诊断装饰经 @uiw 再导出的 `@codemirror/view`/`@codemirror/state` API，零额外依赖。
  - `components/DocEditor.tsx`（新）：CodeMirror 6 编辑器——Markdown 高亮；```yaml/```yml 围栏用真 YAML 解析高亮；**YAML frontmatter 装饰高亮**（首段 `---` 区域底色 + `key:` 键着色 + `---` 淡显，与后端 frontmatter 判定同规则）；**校验行内标记**（StateField + Decoration：错误行红底红边、告警行蓝底蓝边，同行命中错误与告警时错误优先）。
  - `pages/SkillDetailPage.tsx`：左列升级为**预览/编辑双模式**——「编辑」进入 CodeMirror，右上「保存 / 取消」；**防抖 350ms 实时校验**（`docValidate`，零写盘）；**底部问题列表**（错误红徽章在前、告警蓝徽章在后，行号 + 人话规则标签 + 后端原文说明；空态「校验通过」）；**错误阻断保存**（保存按钮禁用 + 后端 doc_write 权威双闸）；**告警不阻断**；未保存改动离开（取消/返回）先弹「放弃修改」确认，防静默丢失。
  - `api/queries.ts`：`docValidate` 直连函数（高频调用不走 react-query）+ `useDocSave`（保存成功后统一失效 skillDoc / library / security / securityOverview / cost——**编辑→保存→UI 刷新链路**）。
  - `api/ipc.ts`：DocIssue / DocCheckReport / DocWriteOutcome 类型镜像 + `libraryDocValidate(contents?)` 封装；`libraryDocWrite` 返回类型从 unknown 收紧为 DocWriteOutcome。
  - `index.css`：frontmatter 与诊断样式（设计令牌 CSS 变量，无新颜色正本）。
  - `dev/IPCTester.tsx`：doc 三命令参数更新（validate 带 contents 示例）。

没做：
- Markdown 工具栏（加粗/列表快捷按钮）——v1 编辑器已够用，避免堆功能。
- 多标签/多文档同时编辑（单详情页单编辑器）。
- .bak 恢复 UI（备份文件在正本目录，用户可手动恢复；回收站恢复 UI 属后续步骤）。

自检输出（真实结果）：
1. `cargo clippy --workspace --all-targets -- -D warnings` → `Finished dev profile in 7.40s`，零警告
2. `cargo test --workspace` → 205 passed / 0 failed（adapters 23、domain 6、engine 154、ports 9、store 13；doc 测试迁移 +3：行号断言、unclosed_fence 定位、name_mismatch、rescan 幂等/只刷可变列）
3. `npm run build`（tsc --noEmit + vite）→ 通过（12.50s，2058 modules；修复 lang-markdown codeLanguages 需 Language 非 LanguageSupport 的 TS 错误）
4. 核心五 crate `tauri::` grep → 0；全部 .rs ≤ 600 行（58 文件；doc.rs 419 / doc/tests.rs 292）
5. IPC 命令 32 个 = §8 上限（doc_validate 加 contents 参数、doc_write 加增量重扫均为既有槽位内扩展）
6. `cargo build --release -p skillbox-app`（touch main.rs 强制嵌入新 dist）→ **7.60 MB ≤ 12 MB**（3m38s）
7. release exe 启动存活验证 → ALIVE（pid=16540，6 秒）后正常关闭
8. DoD 链路：编辑→校验（validate_text 单测：错误/告警/行号）→保存（doc_write 单测：错误拒绝写入、.bak keep 1 轮换、dry_run 零写盘）→增量重扫（rescan 单测：description/hash 刷新、name/canonical_path 不动、幂等）→UI 刷新（useDocSave onSuccess 失效 5 个查询键，tsc 类型校验）。**UI 自动化验证不可用**：verify_ui 仅支持 HarmonyOS 工程，本项目为 Tauri 桌面应用；已在 `~/.codex/skills/editor-demo-skill` 备好手工验证数据。

误报情形（已知，均为告警级不阻断）：
- `unclosed_fence`：正文行首出现 ``` 字样会切换围栏开关——教学文档演示围栏语法（奇数个行首 ```）、```` 包裹 ``` 的嵌套示例可能误报；4+ 空格深缩进的 ``` 也会被当围栏（严格 Markdown 中是缩进代码块）。
- `missing_file`：无 scheme 的裸域名链接 `](www.example.com)`、带查询串 `](file.md?x=1)`、示例模板链接会被当相对路径误报缺失。
- 假阴性（漏报非误报）：`<img src>`、引用式链接 `[ref]: file.md`、行内代码中的路径不检查。
- `name_mismatch`：keep-in-place 登记名来自目录名/覆盖名时与 frontmatter name 本来就可能不同——提示性质。
- description 20–200 按 Unicode 字符数（CJK 1 字 = 1 字符）仅作建议。

遗留风险 / 备案：
- 编辑器实时校验为 350ms 防抖 IPC 调用（纯静态只读，实测廉价）；保存时后端 doc_write 仍是权威闸门，前端校验仅体验层。
- 增量重扫失败（如 DB 锁）不回滚保存——文件已安全落盘（含 .bak），经 toast 提示 indexError，下次保存或更新检查会再次刷新索引。
- CodeMirror 引入后前端 JS 包 353KB → 约 +120KB（gzip 后约 +40KB），release 总体积 7.60 MB 不变（≤12MB 预算内）。

## Step 11 安全扫描（§9.1 八条静态规则）+ 上下文成本面板（§9.2）

状态：完成

做了：
- **后端——安全扫描引擎**（`engine/src/audit.rs` 新文件 + `audit/tests.rs` 13 测试）：
  - `AuditContext{fs, linker}`：**只注入读取类端口**——这是"无子进程"硬约束的结构性保证（引擎依赖图中不存在可被 skill 触达的执行面）；`audit_skill(ctx, root) → SecurityReport{risk_level, findings}`。
  - **八条纯静态规则**（只提示不阻断安装）：shell_exec(Warning) / network_egress(Info) / credential_access(Critical) / path_traversal(Warning) / obfuscation(Critical) / symlink_escape(Critical) / hidden_files(Info) / excessive_scope(Warning)；risk_level = 命中最高严重度（High/Medium/Low）。
  - 规则要点：联网命令（curl/wget/requests 等）任何文件标记，裸 URL 仅非 markdown（文档引用不算）；`.env` 词边界判定（`.environment` 不误报）；`process.env` + 敏感词组合；超长十六进制串 ≥32 字符判混淆；隐藏文件/目录标记但不递归（`.git` 内部不参与内容规则）；node_modules 不递归；链接（含 Windows junction）一律不跟随、读目标判越界；SKILL.md frontmatter `tools`/`permissions` 含 `*`/all 判过宽。
  - 防御性上限：深度 ≤6、文件数 ≤400、单文件 ≤512KB；目录读取/解码失败静默跳过。
  - **【硬约束证明】**`audit_is_pure_static_zero_writes_and_flags_shell_exec`：CountingFs（计数全部变更类端口调用）包裹真实文件系统，含 `bash -c` + `curl | bash` 的假 skill → shell_exec 命中且写入计数 == 0（纯静态读取，无子进程可 spawn）。
- **后端——上下文成本引擎**（`engine/src/cost.rs` 新文件，8 内嵌测试）：
  - `context_cost(skills, targets, enabled_tools) → CostReport{per_tool, conflicts}`：逐启用工具统计常驻 skill 数 + description 字符数，**~3.5 字符/token 折算（`CHARS_PER_TOKEN`，估算值）**。
  - 冲突检测：description 分词（小写 ASCII 字母数字段 ≥2 字符 + 中文逐字）→ 两两 Jaccard ≥ 0.35（`CONFLICT_THRESHOLD`）→ 连通分量成组（A~B、B~C 传递同组）；keywords = 全员交集（空则退化 ≥2 成员高频词，截 8）；similarity = 组内最大两两 Jaccard。
- **后端——IPC 实装**（复用既有 3 个占位命令槽位，命令总数 32 不变）：
  - `security_scan(skillId)`：store.get_skill → audit → 结果缓存进 `AppState.security_cache`（Mutex<HashMap<SkillId, SecurityReport>>）。
  - `security_report()`：合并缓存（risk 取最高、findings 合并）→ 工具箱总览。
  - `cost_report()`：DB 快照（list_skills/list_targets）+ env_snapshot 过滤已启用工具 → context_cost。
  - state.rs 增 `put_security_report` / `security_overview`；tools.rs `env_snapshot` 改 pub(crate) 供 cost_report 复用。
- **前端**：
  - `api/ipc.ts`：SecurityFinding/SecurityReport/RiskLevel/ToolCost/ConflictMember/ConflictGroup/CostReport 类型镜像 + securityScan/securityReport/costReport 封装；`api/queries.ts`：queryKeys.security(skillId)/securityOverview/cost + useSecurityReport（staleTime 5min，懒挂载触发）/useSecurityOverview/useCostReport。
  - `components/SecurityKit.tsx`（新）：八条规则人话标签、RiskBadge（低/中/高风险/未体检）、FindingList（规则·严重度·位置·片段·建议）——库页/详情页/工具箱三处共用。
  - **工具箱页**（`pages/ToolboxPage.tsx` 新文件，MainShell 替换占位）：
    - 安全扫描区：总体风险徽章 +「全部体检」按钮（逐 skill 顺序扫描 + 进度）+ 逐 skill 行（徽章 + 发现数，点击展开明细）。
    - 上下文成本区：数字卡片（常驻 skill 总数 / 估算 token，标注"估算值 ~3.5 字符/token"）+ 按工具**纯 CSS 条形图**（width % 相对最大值，无图表库）+ 冲突风险组卡片（相似度 % + 重叠关键词 Tag + 成员列表 + **一键停用** = deploy_revoke(skillId,[toolId])）。
  - **SkillDetailPage**：右列新增「安全体检」卡（打开即扫描，FindingList 逐条列出发现与建议）；头部徽章从「未体检」占位升级为真实风险徽章。
  - **LibraryPage**：卡片/行风险徽章（CardRiskBadge 挂载即触发扫描，TanStack + 后端双缓存幂等廉价）。
- 布局遵守 §11.3：专业面板全部收进侧栏「工具箱」，主界面（库/工具/体检）不受污染。

没做：
- 安全扫描的定时/后台自动重扫（v1 进页/开卡懒触发 + 手动「全部体检」即可）。
- 日志导出、缓存/回收站管理 UI（IPC 已有，界面留工具箱后续步骤）。
- 标签编辑、设置页。

自检输出（真实结果）：
1. `cargo clippy --workspace --all-targets -- -D warnings` → `Finished dev profile in 4.37s`，零警告
2. `cargo test --workspace` → 202 passed / 0 failed（adapters 23、domain 6、engine 151、ports 9、store 13；新增 audit 13 + cost 8）
3. `npm run build`（tsc --noEmit + vite）→ 通过（1.97s，2021 modules）
4. 核心五 crate（domain/ports/adapters/engine/store）`tauri::` grep → 0
5. 全部 .rs ≤ 600 行（57 文件；audit.rs 362 / audit/tests.rs 262 / cost.rs 340）
6. IPC 命令 32 个 = §8 上限（security_scan/security_report/cost_report 为既有槽位实装，无新增）
7. `cargo build --release -p skillbox-app`（touch main.rs 强制嵌入新 dist）→ **7.60 MB ≤ 12 MB**（1m49s）
8. release exe 启动存活验证 → ALIVE（pid=33188，6 秒）后正常关闭

遗留风险 / 备案：
- 库页网格视图所有卡片挂载即触发扫描：skill 数量大时首开有一批 IPC 调用（纯静态读取、后端缓存，实测廉价；列表视图 VirtualList 懒挂载天然限流）。
- security_report 总览依赖后端缓存（应用重启后为空），工具箱「全部体检」可主动补全——诚实显示「还没有已汇总的发现」而非造假数据。
- 静态规则有保守取向的误报可能（如正文中的 `sh -c` 示例文本）；只提示不阻断，snippet + advice 引导人工判断。

## Step 10 体检页：severity 分组 + 人话解释 + 逐项 dry-run 修复流

状态：完成

做了：
- **纯前端步骤**（doctor_run / doctor_fix IPC 与引擎在 Step 5 已就位，命令总数 32 不变）：
  - `api/ipc.ts`：补全体检类型镜像（`Severity` / `DiscrepancyKind` 七类 / `SuggestedFix` / `DoctorIssue` / `DoctorReport` / serde 外部标签 `FixOutcome` / `FixReport`）+ `doctorRun()` / `doctorFix({items,confirmed,dryRun})` 封装；`api/queries.ts`：`queryKeys.doctor` + `useDoctorReport()`（TanStack 自动挂载体检）+ `useDoctorFix()`（真实执行才失效 library/targets/tools，dry-run 零写入不刷）。
- **体检页**（`pages/DoctorPage.tsx` 新文件，MainShell 替换占位）：
  - **按 severity 三组**（严重问题 danger / 需要注意 blue / 提示 neutral），组内逐条卡片；空态「体检通过」、加载骨架屏、错误重试齐备。
  - **人话解释**（不抛枚举名）：七类各自一句话，如 UnmanagedRealDir →「『Claude Code』目录里有一个不是 SkillBox 管理的文件夹，它可能是你自己放的」；MissingTarget →「记录里显示已部署，但磁盘上找不到」；DuplicateName →「多个目录里有同名但内容不同的 Skill」。
  - **统一修复流（DoD：影响清单在修复前可见）**：每条问题独立按钮 → `doctor_fix(items=[单个id], dry_run=true)` → 弹层展示 NeedsConfirmation 影响清单（含"将影响以下路径"逐行列出）→ 用户确认 → `doctor_fix(items=[单个id], confirmed=true, dry_run=false)` → toast 逐项结果 + 自动重新体检。**无任何"一键修复全部"/批量按钮**。
  - **危险项纪律**：UnmanagedRealDir（收编）/ ForeignLink（移除外来链接）/ CopyDrift（用正本更新副本）/ 悬空链接移除均标「危险项」徽章 + danger 按钮，逐个 dry-run → 逐个二次确认；每条确认弹层只含该条的影响路径。
  - **DuplicateName 只提示不处理**：无修复按钮，仅「查看差异」弹层——列出全部冲突路径 + 明示"SkillBox 不会自动挑选正本，也不会改动这些目录"，提供「打开中央库」辅助人工比对。
  - **顶部「重新体检」**（doctor 只读随时可安全重跑）+ 「上次体检时间」（TanStack `dataUpdatedAt`，页面显示）+ 已检查工具数/耗时。
- IPCTester doctor 参数保持现有（doctor_run/doctor_fix 已在 32 命令全集内）。

没做：
- 体检结果的持久化快照（每次进页即时体检，无历史对比；v1 无此需求）。
- 自动定时体检（v1 手动 + 进页自动触发即可）。

自检输出（真实结果）：
1. DoD-doctor 只读性：`doctor_is_read_only_zero_disk_writes`（engine/doctor/tests.rs:35，CountingFs 零写入证明，七类场景非空前提下断言）——本步未改引擎，测试原样通过
2. DoD-危险项无法被批量修复：UI 无批量修复入口（仅逐条按钮）；每条修复调用 `items=[单个id]`；危险项确认弹层逐个出现；引擎侧 `confirm_gate` 未确认返回 NeedsConfirmation
3. DoD-影响清单修复前可见：所有修复先 dry-run，弹层展示影响描述 + "将影响以下路径"逐行清单，确认后才执行
4. `cargo clippy --workspace --all-targets -- -D warnings` → `Finished dev profile in 1.16s`，零警告
5. `cargo test --workspace` → 181 passed / 0 failed（adapters 23、domain 6、engine 130、ports 9、store 13）
6. `npm run build` → 通过（7.19s）
7. 核心五 crate `tauri::` grep → 0；全部 .rs ≤ 600 行；IPC 命令 32 = 上限
8. `cargo build --release -p skillbox-app`（touch main.rs 强制嵌入新 dist）→ **7.5 MB ≤ 12 MB**（1m36s）；release exe 启动存活验证 → ALIVE（8 秒）后正常关闭

遗留风险 / 备案：
- 「上次体检时间」取自 TanStack `dataUpdatedAt`（会话内准确；应用重启后进页自动重新体检，时间随之刷新——不落库、不造假历史）。
- 修复成功后自动 invalidate doctor 查询即重跑体检：问题场景大时可能有短暂加载骨架屏，属预期。

## Step 9 Skill 详情 / 溯源 / 更新 / 只读来源 + 工具页重排

状态：完成

做了：
- **后端——索引可变列刷新**：
  - `ports IndexStore::refresh_skill(skill_id, description, origin, content_hash)`（MemoryIndexStore + DbIndexStore 实现；name/canonical_path 不可变，0 行 → NotFound；store UPDATE skills SET description/origin_*/content_hash/updated_at）；更新的最小原语。
- **后端——更新引擎**（`engine/update.rs` 新文件 + `update/tests.rs` 11 测试）：
  - `UpdateContext{central_root,cache_root,trash_root,fs,clock,git,hasher,linker,index,watcher}`；`update_check(target,&ctx)` / `update_apply(target,dry_run,&ctx)`。
  - **git 来源**：clone_head 到缓存临时目录 → 与已记录 revision 比对 → diff_summary（失败降级 None）；apply = 临时副本 fresh_tmp → copy_dir_all → 旧正本 move_to_trash → rename 原子换入（失败回滚 rename 回来）→ hash_dir → refresh_skill 记录新 revision；git 不可用 → Unsupported + 说明；revision 未记录 → UpdateAvailable + note「无法证明已是最新」。
  - **local 来源**：fresh_state 重算 hash/description 比对；apply = refresh_skill(OriginInfo::local())（重新导入语义）。
  - **archive 来源**：Unsupported（在线更新无意义，诚实提示重新导入）。
  - 旧正本永远进 trash（无硬删除）；apply 全程 MuteGuard。
- **后端——只读来源（§4.3 硬约束）**：
  - `env ToolInfo.extra_scan_dirs`（resolve_extra_scan_dirs 已解析路径）+ `is_read_only_source(path,scope)`；**ipc deploy_apply / deploy_set_mode 入口硬守卫**：正本位于插件缓存内 → InvalidInput 拒绝（deploy_revoke 仍允许——撤销不算写入）。
- **后端——IPC 实装**（复用既有 2 个命令槽位，命令总数 32 不变）：
  - `updates_check(skill_id) → UpdateCheckOutcome{status,currentRevision,newRevision,diffSummary,note}`；`updates_apply(skill_id,dry_run) → UpdateOutcome{Applied|UpToDate|Preview|Unsupported}`；state 增 `update_context()`。
  - `system_open_path` 支持 http(s) URL 分支：scheme 白名单 + 拒绝空白/控制字符 → explorer/open/xdg-open 打开（溯源卡「打开仓库」用）。
  - `install.rs` 拆分（`install/tests.rs`），为 update.rs 腾挪空间。
- **前端——详情页**（`pages/SkillDetailPage.tsx` 新文件）：
  - 头部：名称 · 来源徽章 · 「未体检」风险徽章（诚实占位）· 只读来源徽章。
  - **来源溯源卡**：类型 / 仓库 URL（「打开仓库」走 URL 分支）/ 子路径 / revision（12 字符截断）/「打开目录」（`skill:<id>`）。
  - **SKILL.md 预览**：`library_doc_read` + 自建 `Markdown.tsx`（~215 行，frontmatter 剥离 + 标题/列表/代码块/引用/行内粗斜体代码链接，全 React 元素零 dangerouslySetInnerHTML——依赖白名单内无 markdown 库）。
  - **更新卡**：git「检查更新」→ diff 摘要展示 → 确认弹层（明示"旧版移入回收站"）→ 应用；local「检查本地变更」→ 重新导入；archive 诚实提示不支持。
  - **部署状态卡**：逐工具 mode 徽章（链接/副本）+ 路径 + 取消；未部署工具一键部署；只读来源整卡替换为「只读来源，不可部署」。
- **前端——库页/工具页接线**：
  - LibraryPage：卡片/行点击进详情（按 id 派生，skill 被删自动回落列表）；`isReadOnlySkill`（前端 extraScanDirs 前缀匹配）只读徽章 + agent 角标禁用（后端守卫为权威，前端为体验层）。
  - ToolsPage 重排：按 form_factor 五组（命令行（CLI）/ IDE · 桌面端 / 编辑器插件 / 终端 TUI / 桌面助手）；已安装：图标瓦片 + 已检测徽章 + 形态徽标 + skills 路径 + 目录按钮 + 启停 Toggle；**未检测工具折叠区**（默认收起，说明"该工具的配置目录不存在——启动一次该工具即可被检测到"，展示性行不做开关）。
- **修复历史违规**：`fix.rs` 632 行超 600 上限（Step 8 遗漏）→ 拆出 `fix/exec.rs`（执行器 + TOCTOU 复查，456 行），fix.rs 留入口/确认门/影响面文案/valid_skill_name（doc.rs、install.rs 引用不变）；IPCTester 过期分组标题与缺失参数修正。

没做：
- 安全体检实装（风险徽章仍为「未体检」占位，Phase 4）；标签编辑；文档编辑器（本步骤仅预览）；回收站恢复 UI（trash_list/restore 已有 IPC，界面留工具箱）。
- 更新的定时自动检查（v1 手动触发即可，避免后台惊喜流量）。

自检输出（真实结果）：
1. `cargo clippy --workspace --all-targets -- -D warnings` → `Finished dev profile in 11.46s`，EXIT=0，零警告
2. `cargo test --workspace` → 181 passed / 0 failed（adapters 23、domain 6、engine 130、ports 9、store 13；新增 update 11 测试 + env 2 + store/ports refresh_skill 2）
3. `npm run build`（tsc --noEmit + vite）→ 通过（8.12s，2018 modules）
4. 核心五 crate（domain/ports/adapters/engine/store）`tauri::` grep → 0
5. 全部 .rs ≤ 600 行（fix.rs 632 违规修复：fix.rs 197 / fix/exec.rs 456；update.rs 372 / update/tests.rs 322 / install.rs 310 / install/tests.rs 293 / env.rs 452）
6. IPC 命令 32 个 = §8 上限（updates_check/updates_apply 为既有槽位实装，无新增）
7. `cargo build --release -p skillbox-app` → **7.5 MB ≤ 12 MB**（3m41s）
8. release exe 启动存活验证 → ALIVE（8 秒）后正常关闭

遗留风险 / 备案：
- 更新 apply 的回滚为"尽力 rename 回来"：回滚失败时保留新正本 + trash 双份并如实报错（不丢数据优先）。
- git 更新强依赖 git 在 PATH；缺失时 Unsupported + 说明，不阻塞其他功能。
- 前端只读判定为路径前缀匹配（体验层）；权威判定在后端 deploy 守卫，绕过 UI 也会被拒。
- diff_summary 依赖 git diff 输出，失败降级为 None（详情页相应不展示摘要，仍可更新）。

## Step 8 主流程 UI：首启向导 + 技能库/工具页 + TanStack Query

状态：完成

做了：
- **后端小幅扩展**（为 UI 提供数据，不新增能力面）：
  - `ScanProgress` 扩展（engine/types.rs）：新增 `found`（已确认 skill 总数）、`currentTool`、`foundByTool`（按工具累计发现数，中央库命中不计入）；ScanCtx 增 `found: AtomicU32` + `found_by_tool: Mutex<BTreeMap>`，`record_skill` 登记 + 计数，`note_scanned(&ScanJob)` 快照进进度事件 → 首启向导"发现 Claude Code ×12…"实时数据源。
  - 第 32 个（= 上限）IPC 命令 `library_targets`：返回 `Vec<TargetRow>`（skill→tool 部署意图全集），`TargetRow` 补 `Serialize(camelCase)`（store 增 serde 依赖）；技能卡 agent 角标"已部署"状态数据源。
- **前端数据层**（新增依赖均在 §1 白名单）：
  - `@tanstack/react-query` + `lucide-react` 落地；`api/ipc.ts`（32 命令中 UI 用到的 18 个的类型化封装 + `scan://progress|done|error` 事件订阅 + `ipcError` 错误规范化）；`api/queries.ts`（查询键 + hooks；**所有 mutation 成功后失效 library/targets/tools 相关查询**，零 useState+useEffect 手动刷新；首启开关 `first_run.done` 走 settings_get/set，staleTime Infinity）。
- **首启向导**（`first-run/FirstRun.tsx`，§10 五步）：
  - 欢迎（两句话说明 + 明示"不移动/不删除任何文件"）→ 扫描（事件流实时：已扫描 N 目录 · 已发现 M 个 · "发现 Claude Code ×12"人类语言行，轮询 scan_status 兜底事件丢失窗口，可取消）→ 选择（按工具分组、逐项勾选默认全选、只读目录标"只读"；**默认原地登记**（keepInPlace=true 零写盘）；「统一到中央库」可选 checkbox 默认关，勾选后明示"原文件先移入回收站可恢复"）→ 执行（逐项串行，进度条 + 逐条结果流，单项失败不中断）→ 完成（成功/失败统计，失败原因列出）→ 写 `first_run.done` 进主框架。
  - 统一策略：install_local(keepInPlace=false) 复制入中央 → deploy_apply(overwrite=true) 以链接替换原目录（engine 部署纪律自动把原目录移入 trash = 快照/可恢复）；read_only 与无 tool_id 目录退化为原地登记。
- **主框架**（`shell/`）：Sidebar（§11.3 IA：概览/技能库/工具/体检/工具箱/设置 + 底部开发者入口与"打开中央库"；选中项 3px 蓝左条 + primary-soft 底）+ MainShell（useState 视图切换，桌面应用不引入路由）。
- **技能库页**（`pages/LibraryPage.tsx`，§11.3）：
  - 顶栏：搜索（名称/描述/来源）· 来源过滤（全部/本地/Git/压缩包）· 标签（弹层诚实空态——v1 skill_tags 无写入来源）· 网格/列表视图切换 · 扫描按钮。
  - 卡片：名称 · 描述（2 行截断）· 来源徽章 · 风险徽章（诚实占位"未体检"，Phase 4 实装）· **agent 角标常驻可见**（所有已启用+已安装工具；已部署=蓝底白字，点击弹一级确认「取消部署」；未部署=描边灰，点击即 deploy_apply；无右键菜单）。
  - **两级删除语义**：一级「取消部署」（单工具或全部，确认框明示"正本仍在中央库"）= deploy_revoke；二级「删除 Skill」独立按钮 + 确认框明示"撤销 N 个部署 + 正本移入回收站可恢复" = library_remove(mode="delete")。**无任何直接删文件的不合格"删除"按钮**。
  - 重新扫描：信息性（不自动入库）——扫描中弹层进度，完成后弹结果："发现 N 个 Skill 目录，其中 M 个已入库"，未入库目录逐行「登记」按钮（原地登记，不移动文件）。
  - 列表 >300 行走自建 `VirtualList`（~60 行，absolute 定位 + 可见窗口渲染，零第三方库）；加载骨架屏 / 错误重试 / 空态齐备；mutation 错误走自建 `Toast`（右下角堆叠，~90 行）。
- **工具页**（`pages/ToolsPage.tsx`）：按 form_factor 5 组分组（命令行/IDE/编辑器插件/终端 TUI/桌面助手）；每工具：图标 + 显示名 + 已安装徽章 + deployDir（等宽截断）+ 「目录」按钮（system_open_path `tool:<id>`）+ 启用 Toggle（tools_set_enabled 实时失效）。
- **概览页**：技能总数/已部署目标/已启用工具三统计卡（TanStack 派生）+ 快捷操作；**体检/工具箱/设置三页为诚实占位**（EmptyState + Phase 4 说明，不造假数据）；main.tsx 挂 QueryClientProvider + ToastProvider；App.tsx 按 `first_run.done` 分流向导/主框架（加载期骨架屏）。

没做：
- 体检/工具箱/设置实装、标签编辑、skill 详情/文档编辑器（Phase 4，占位诚实）。
- react-router-dom 未引入（桌面单窗视图切换 useState 足够，白名单依赖按需原则）。

自检输出（真实结果）：
1. `npm run build`（tsc --noEmit + vite）→ 通过（首跑 5 处 `ipcError` 未导出 TS2305，补导出后通过）
2. `cargo clippy --workspace --all-targets -- -D warnings` → `Finished dev profile in 1.37s`，EXIT=0，零警告
3. `cargo test --workspace` → EXIT=0；166 passed / 0 failed（adapters 23、domain 6、engine 117、ports 8、store 12）
4. 核心五 crate `tauri::` grep → 0；前端运行时依赖 = react / react-dom / @tauri-apps/api / @tanstack/react-query / lucide-react（全部 §1 白名单内；无动画库/图表库）
5. 全部 .rs ≤ 600 行（未改大文件；fix.rs 546 最大；scan.rs 407 + types.rs 小幅扩展）
6. IPC 命令 32 个 = §8 上限（新增 library_targets 占满，无超）
7. `cargo build --release -p skillbox-app`（touch main.rs 强制嵌入新 dist）→ **7.47 MB ≤ 12 MB**
8. release exe 启动存活验证 → ALIVE pid=28980（7 秒）后正常关闭

遗留风险 / 备案：
- 扫描事件经 Tauri event，StrictMode 双订阅/重放窗口可能丢事件 → 已用 1.2s 轮询 `scan_status` 兜底终态（向导与库页重扫均带）。
- 统一到中央库时若 frontmatter name 与原目录名不一致，deploy_apply 会部署到 `deployDir/<name>` 新路径而非替换原目录（原目录保持不动，产生中央正本 + 原目录并存）；v1 接受此行为（不移动未知文件优先于完全统一）。
- 重新扫描为信息性：不自动登记、不自动部署（防惊喜写盘）；登记需用户逐项点击。
- skill_tags v1 无写入来源：标签过滤入口保留但为诚实空态；风险徽章同理为"未体检"占位。

### 新手视角自测描述（首次运行，模拟机：3 个 agent 目录）

> 模拟机环境：`~/.claude/skills/`（3 个 Skill）、`~/.codex/skills/`（2 个）、`~/.gemini/skills/`（1 个），其中一个 Skill 的 frontmatter 缺失 description。以下为第一次接触 SkillBox 的非专业用户的第一人称走查。

我双击打开了 SkillBox。屏幕最上方有一条流动的蓝色高光条，中间是一张卡片："欢迎使用 SkillBox"，两句话说清了它是干什么的，下面还有一行小字明确写着"首次运行只做两件事：扫描和登记。不会移动、修改或删除你的任何文件"——这让我放心。我点了「开始扫描」。

页面立刻变成"正在扫描…"。没有干等，数字在跳："已扫描 47 个目录 · 已发现 5 个 Skill"，下面还有一行行人类话术："发现 Claude Code ×3""发现 Codex ×2""发现 Gemini CLI ×1"。底部有当前正在查看的目录路径，还有一个不显眼的「取消扫描」按钮。几秒后进度页自动翻页。

现在是一个总结页：标题"扫描完成，发现 6 个 Skill"。三个工具分组，每组标着"×3 / ×2 / ×1"，每个 Skill 一行，名字、描述（有的没描述）、勾选框默认全选，组头还有"全选 / 全不选"。页面底部有一个开关区：默认是"原地登记"的说明——"只记录位置，不移动、不修改任何文件"；「统一到中央库」的开关是关着的，我没开（我是新手，不想让它动我的文件）。我点了「原地登记 6 个 Skill」。

执行页有进度条和逐行结果："✓ xxx — 已原地登记"一行行往下走，全部变绿后到了完成页："全部完成！成功 6 个。已原地登记，文件位置保持不变。"我点了「进入 SkillBox」。

现在我看到了主界面：左边侧栏有"概览、技能库、工具、体检、工具箱、设置"，技能库带星标。我点进「技能库」：6 张卡片，每张有名字、描述、灰色"本地"徽章、"未体检"徽章，还有一排 agent 角标——"CC""CX""GC"三个小方块常驻可见，没部署的是描边灰色。我在一张卡片上点了"CC"，它弹出一个确认框："已部署到 Claude Code"的提示出现了，角标变成了蓝底白字。我又点这个蓝色角标，弹出「取消部署」确认框，明确写着"正本仍在中央库，随时可重新部署"——我确认后角标变回灰色，提示"已取消部署到 Claude Code"。

我看到卡片右下角有两个明确分开的按钮：「取消部署」和「删除 Skill」。点「删除 Skill」弹出的框写的是"正本将移入回收站，可在后续版本的工具箱中恢复，此操作不会直接抹除文件"——不是直接删掉，这让我敢用。我没真的删。

我又点了顶栏的「扫描」，弹层里数字跳动，完成后告诉我"发现 6 个 Skill 目录，其中 6 个已入库，已入库的目录未做任何改动"。然后我去「工具」页看了眼：三个工具按"命令行"分组排着，各有开关和部署目录，还有一个「目录」按钮能直接打开文件夹。整个过程没有出现一个让我看不懂的术语，也没有任何一步在没问我的情况下动了我的文件。

**走查结论（对照 DoD）**：非新手不可完成的步骤：无；所有删除操作均为两级确认（取消部署→正本不动；删除 Skill→入回收站）；首启全程零文件移动（默认原地登记）；扫描过程流式可见（计数 + 按工具人类语言），无裸 spinner。

## Step 7 设计系统：§11.1/§11.2 视觉基础（纯前端，mock）

状态：完成

做了：
- **设计令牌双轨落地**：`index.css` `:root` CSS 变量为唯一正本（§11.1 全部 13 色 + 3 圆角 + 2 时长 + 缓动函数 + 卡片影），`tailwind.config.cjs` 全部经 `var()` 引用（primary 四阶 / highlight 青 / page / surface / line / ink 三级 / danger / success）；圆角 card 10px / btn 8px / tag 999px；duration 120/200ms；ease `cubic-bezier(.2,.8,.2,1)`；body 正文 14px + §11.1 字体栈。
- **顶部 2px 高光条**（`HighlightBar` + `.sb-highlight-track`）：蓝→青→蓝 `linear-gradient`，200% 宽内层 `translateX(-50%)` 6s 线性无缝循环，纯 CSS。
- **11 个基础组件**（`src/components/ui/` + barrel `index.ts`）：Card / Button / Badge / Tag / Toggle / Modal / Drawer / Tooltip / EmptyState / ProgressBar / Skeleton，全部受控 TSProps。
- **卡片 hover**（`.sb-card--interactive`）：`translateY(-2px)` + 深影伪元素 + `conic-gradient` 蓝色描边伪元素（mask-composite 环形），三处全走 transform/opacity；`sb-card--lifted` 变体静态强制展示悬浮态（演示/测试用）。
- **弹层/抽屉玻璃拟态**：backdrop `rgba(16,40,80,.32)` + `backdrop-filter: blur(8px)`；Modal pop（translateY+scale）/ Drawer slide-right 均 200ms 进出（keyframes `reverse forwards` 退出 + 200ms 延迟卸载 hook）。
- **§11.2 动画纪律**：全部动画只动 transform/opacity（ProgressBar 用 scaleX 表达进度不动 width、Toggle 滑块 translateX、Tooltip group-hover opacity）；错峰 fade-up（`--sb-i` × 20ms，`min(·,10)` 封顶 10 项）；`prefers-reduced-motion` 全局 0.01ms 降级。
- **DesignSystemPage.tsx**：13 色色板（色块 + hex + 变量名）、排版示范、高光条、卡片两种状态并排（默认态 vs 悬浮强制态——§11.1 未定义暗色主题，按默认/升起两态展示）、11 组件全变体、错峰 fade-up 10 项演示；App.tsx 改三视图切换（首页 / 设计系统 / IPC Tester）。

没做：
- 未接任何后端数据（全 mock，按步骤边界）；TanStack Query / lucide-react / react-router 未引入（Phase 3 按需加，均在 §1 白名单）。
- 侧栏与 §11.3 信息架构布局（Phase 3 主流程步骤）。

自检输出（真实结果）：
1. `npm run build`（tsc --noEmit + vite）→ 通过；46 modules，CSS 19.45KB / JS 242.98KB（gzip 4.71/75.87KB）
2. `cargo clippy --workspace -- -D warnings` → `Finished dev profile in 0.41s`，EXIT=0，零警告
3. `cargo test --workspace` → EXIT=0；166 passed / 0 failed（adapters 23、domain 6、engine 117、ports 8、store 12）
4. 核心五 crate `tauri::` grep → 0；前端运行时依赖仍为 react / react-dom / @tauri-apps/api（§1 白名单内，零新增；无动画库/图表库/粒子库）
5. 全部 .rs ≤ 600 行（未改 Rust；新增前端文件最大 DesignSystemPage.tsx ~300 行）
6. `cargo build --release -p skillbox-app` → 7.45 MB ≤ 12 MB
7. release exe 启动存活 8 秒后正常关闭（DoD"能跑"）

遗留风险 / 备案：
- cargo 不追踪 `frontend/dist` 内容变化（本步 touch main.rs 强制重编嵌入）；日常开发走 `tauri dev`（vite 5173 热更）不受影响，发布走 `tauri build` 亦不受影响，仅"手跑 cargo build --release 验证前端"场景需先 npm run build + 触发重编。
- Tailwind 颜色经 `var()` 引用后不支持 `bg-primary/10` 透明度修饰符（需透明度时用任意值 `bg-[rgba(...)]`）。
- Modal/Drawer 退出动画用 keyframes `reverse forwards` 近似（视觉等价 200ms 退出），未引入 JS 过渡框架（遵守"禁动画库"约束）。
- conic-gradient 描边为静态角度（0deg 起点淡蓝渐变环）；未做描边旋转动画（旋转需 @property 注册自定义属性，WebView2 支持但收益低，v1 不做）。

## Step 6 IPC 层：31 命令 + app 接线 + IPCTester

状态：完成

做了：
- **engine 全量补齐**（Phase 2 前置，本步一并交付；engine 59 → 117 测试）：
  - env.rs（新增 318 行）：engine 侧环境组装（adapters 消费面）——`tools()` 工具快照、`tool_deploy_dir()`、`scan_roots()` / `doctor_targets()`（`disabled: &[ToolId]` 过滤禁用工具）、`health_context(HealthContextInputs)`；9 测试（含 `scan_roots_exclude_disabled_tools`，禁用 → 空目标集）。
  - install.rs（新增 504 行）：`install(InstallRequest, dry_run, ctx)` —— `InstallSource::Local{path}`（keep_in_place 收编/复制）与 `Git{url, subpath, revision}`（经 GitClient 端口）两来源，InstallOutcome；12 测试。
  - ops.rs + ops/tests.rs（478 + 393 行）：`deploy_to_tools` / `revoke` / `remove_skill`（RemoveMode::Undeploy|Delete）/ `deploy_set_mode` —— deploy/undeploy 的索引编排层（IndexView 读写 + skill_targets 记录维护，全带 dry_run）；13 测试。
  - system.rs（新增 404 行）：`cache_inspect` / `cache_clear` / `trash_list` / `trash_restore`；12 测试。
  - doc.rs（新增 327 行）：`doc_read` / `doc_write`（DocWriteOutcome）/ `validate_meta`（DocCheckReport）；11 测试。
  - fix.rs：`FixRequest` 新增 `pub dry_run: bool`，`dry_run_outcome()` 按问题类型返回影响面预览（NeedsConfirmation{impact}），零写盘零索引写入；新增测试 `dry_run_previews_impact_with_zero_writes`。
  - lib.rs：新增 `pub use skillbox_adapters::{PathScope, ToolOverrides};` 与 `OpsContext` 再导出；`audit_skill` / `context_cost` 仍为 todo!()（占位命令不触达）。
- **ports / store 生产接线**：
  - ports：新增 `OriginInfo`（skills 表 origin_* 列写入面，install 与 adopt 共用，收编固定 local）；`IndexStore` trait 扩展 `delete_target` / `delete_skill`（revoke/remove 用）；3 新测试（总 8）。
  - store/index.rs（新增 330 行）：`DbIndexStore`（SQLite IndexStore 生产实现）+ `Store` 索取面（`index_store()` / `list_skills` / `get_skill` / `list_targets` / `get_setting` / `set_setting`）；5 新测试（store 总 12）。
- **ipc crate 全量落地**（§8 全部 31 命令 ≤ 32 上限，11 个源文件，单文件最大 state.rs 142 行 ≤ 400 红线）：`AppState` 集中持有 store / index_store / central·cache·trash·logs 四根 / fs·clock·linker·hasher·git 生产实现 Arc / `scan: Arc<Mutex<ScanSession>>`，`init(AppStateInputs)` 一次构建由 app `.manage()`；AppError 为 Serialize + std::error::Error → Tauri v2 合法命令错误类型（序列化给 JS）。
  - **scan 单会话模型**：running 时二次 scan_start → InvalidInput；scan_cancel 空闲 → NotFound；后台线程执行，`scan://progress` / `scan://done` / `scan://error` 三事件广播（tauri::Emitter）；`scan_start` 泛型 `<R: tauri::Runtime>` 支撑泛型 `register<R>`。
  - **写命令 12 个全部带 dry_run**（§8 硬约束）：install_local / install_git / library_remove / doc_write / deploy_apply / deploy_revoke / deploy_set_mode / tools_set_enabled / doctor_fix / cache_clear / trash_restore / settings_set，全部下传 engine 既有 dry_run 通道（install/ops/doc/fix/system）。
  - **工具开关**：settings 键 `tools.disabled`（JSON id 数组，`DISABLED_TOOLS_KEY` 常量）；tools_list 返回 `ToolDto { #[serde(flatten)] info: ToolInfo, enabled: bool }`；tools_set_enabled 对照 engine env 快照校验 id（未知 → NotFound），disabled 集经 HealthContextInputs.scan_roots 生效。
  - **doctor_fix**：items: Vec<String>（→IssueId）空 → InvalidInput；confirmed 映射 ReplacePolicy（UserConfirmed / NoClobber）；复用 Step 5 确认门语义。
  - **deploy**：apply 空 tools → InvalidInput，目标目录经 engine `env::tool_deploy_dir`（ipc 不碰 adapters）；revoke 空工具 = 撤销全部记录（engine 语义）。
  - **system_open_path**：仅接受后端生成 id（central / trash / cache / logs / skill:<id> / tool:<id>，防路径穿越），路径须存在（NotFound），explorer / open / xdg-open 按平台分发。
  - **system_trash_restore**：dest 仅 "central"（v1），其余 InvalidInput。
  - **占位命令**（Phase 4 实装）：security_scan / security_report / cost_report / system_logs_export / updates_check / updates_apply —— 返回真实类型（SecurityReport / CostReport 等），实现为结构化 `AppError::internal("尚未实现：…（Phase 4…）")`，前端拿到可分组错误而非 panic。
- **app 接线**：run() 组合根 = data_root()（`~/.skillbox`）+ create_dir_all skills/cache/trash/logs + `AppState::init` + `.manage(state)` + `ipc::register`；app 依赖仅 skillbox-ipc（+tauri），PathScope 经 ipc 再导出满足依赖方向。
- **前端 IPCTester**：`crates/app/frontend/src/dev/IPCTester.tsx` —— 31 命令按 scan/library/deploy/tools/doctor/panels/system/settings/updates 9 组分列，CmdRow 支持编辑 JSON 参数 + Run + 输出 pre；App.tsx 提供 home/dev 切换；`npm run build`（tsc --noEmit + vite）通过。

没做：
- 6 个占位命令的真实现（security/cost 面板、日志导出 zip、更新检查/应用 —— Phase 4 边界）。
- engine `audit_skill` / `context_cost` 仍为 todo!()（占位命令不触达）。
- 生产 UI 页面（Phase 3；本步只交付 dev IPCTester 验证层）。
- WatchMute 真实 watcher（端口预留，ipc 经 `None` 传参不触达）。

自检输出（真实结果）：
1. `cargo clippy --workspace -- -D warnings` → `Finished ... in 3.44s`，EXIT=0，零警告
2. `cargo test --workspace` → EXIT=0；166 passed / 0 failed（adapters 23、domain 6、engine 117、ports 8、store 12、ipc/app 0）
3. `grep -rn "tauri::" crates/{domain,ports,adapters,engine,store}/` → 0 匹配
4. 全部源文件 ≤ 600 行（最大 engine/fix.rs 546）；ipc 单文件最大 state.rs 142 ≤ 400
5. `cargo build --release -p skillbox-app` → `target/release/skillbox-app.exe` 7.45 MB ≤ 12 MB（ipc+接线增量 +1.58 MB）
6. 前端 `npm run build`（tsc --noEmit + vite）→ 通过，零 TS 错误

遗留风险 / 备案：
- scan 后台线程与 scan_status/scan_cancel 经 `Arc<Mutex<ScanSession>>` 串行化；单进程 IPC 串行调用下无并发写风险，多窗口场景未定义（v1 单窗口）。
- system_open_path 的 skill:<id> / tool:<id> 依赖 store / engine env 解析成功；skill 未入库或工具未安装时 NotFound（不猜测路径）。
- trash_restore dest="central" 之外的目标（含原位恢复）推迟 v2；中央库内重名由 trash slot 命名空间规避（system.rs 复用 engine trash_restore 的 slot 语义）。
- settings 为通用 key/value 透传（含 tools.disabled）；settings_set 的 key 白名单校验推迟 Phase 3 UI 表单层。
- AppError 序列化为 Tauri 命令错误后前端拿到 `string`（serde 结构体字符串）；IPCTester 直接展示，Phase 3 若需结构化错误码再评估 serde 错误通道。

## Step 5 磁盘优先：reconcile / doctor / fix（K4）

状态：完成

做了：
- `engine::reconcile` / `engine::doctor` / `engine::fix` 全量实现，§6 硬约束落地（磁盘才是真相；UI 状态由磁盘推算，DB 索引只作"上次意图"比对）。文件拆分：`reconcile.rs`（256 行）+ `reconcile/tests.rs`（301）、`doctor.rs`（253）+ `doctor/tests.rs`（211）、`fix.rs`（551）+ `fix/tests.rs`（309）、共享测试设施 `test_env.rs`（230）。
- **reconcile（§6.2 纯计算）**：覆盖全部 7 类差异，输出按 `(path, kind)` 排序确定性。BrokenLink 重建匹配三级（v3）：① 原始目标 == 索引正本路径；② 目标父目录 == 正本父目录且条目名 == skill 名（中央库名唯一，至多命中一个）；③ 均未命中 → RemoveLink（不猜测意图）。**DuplicateName 内容身份模型**：同一 skill 的部署（链接或 copy 副本）视为同一身份 `skill:{id}`（副本漂移由 CopyDrift 报告，不构成重名）；索引未知的条目回退内容比对——真实目录 → 递归内容哈希、链接 → 原始目标；同路径同时报 UnmanagedRealDir + DuplicateName（一个是"收编建议"、一个是"挑正本提示"，语义不冲突）。
- **doctor（§6.3 只读体检）**：`inspect_disk`（pub(crate)，fix 复用）观察各工具 skills 根——真实目录算指纹 + 递归内容哈希；链接读原始目标 + 悬空标记，非悬空按解析目标归类（正本 → LinkToSource + skill id / 中央库内 → LinkToSource / 外部 → ForeignLink；悬空一律 ForeignLink + link_broken 交 reconcile 判定）。`hash_dir` 内容驱动（与 mtime 无关——`fs::copy` 不保留 mtime，副本与正本必须可比）：文件 1MB 上限近似哈希、根级 `.skillbox-source.json` 跳过、行排序后整体哈希。`tools` 过滤检查范围（未列出工具不检查、记录过滤，避免误报 MissingTarget；skills 永远保留），`min_severity` 过滤严重度；不存在的工具根跳过；`checked_tools` = 实际检查集。
- **fix（§6.3 逐项修复）**：问题 id 稳定为 `kind:tool:path`；执行前重新 inspect + reconcile 按当前磁盘匹配（**不信任 doctor 快照**，K4）。【硬约束】无"一键修复全部"；`DuplicateName` 任何策略恒 `Skipped`（人工挑正本）。危险项（UnmanagedRealDir 收编 / ForeignLink·悬空链接移除 / CopyDrift 更新）逐个确认门：非 `UserConfirmed` → `NeedsConfirmation{reason: 影响面预览}`（即 dry-run），确认后放行。执行表：adopt（收编进中央库 + 原位放链接；frontmatter name 校验 `^[a-z0-9][a-z0-9-]{1,63}$`；失败回滚）→ rebuild_link（悬空无数据，先移 trash 再 NoClobber 提交，任何策略放行）→ redeploy（MissingTarget，无需确认——部署无破坏面）→ register_record（补登记）→ update_copy（重部署 copy 刷新 source_hash）。全部删除/替换先进 trash；执行期 TOCTOU 重分类（现状变化 → `Skipped{state_changed}`）；copy 相关动作无 IndexStore → `Skipped`（先写库后写盘的顺序要求）。
- **端口扩展**：ports 新增 `IndexStore` trait（`insert_skill` / `upsert_target`，fix 补登记用）+ `MemoryIndexStore`（内存实现，单测用；`canonical_path` 唯一冲突 → `AppError::database`）。
- **测试 34 个新增**（总 100）：reconcile 15（七类差异每类 ≥1 + 健康态零差异 + 确定性排序 + 身份规则）；doctor 6（**CountingFs 证明零写盘**、七类差异端到端一屏分类、min_severity / tools 过滤、幽灵工具根跳过、hash_dir 内容驱动与根标记跳过）；fix 13（**DuplicateName 确认后也跳过**、收编先确认后执行含 NoClobber dry-run 零写入与回滚断言、悬空重建任何策略、悬空/外部链接移除确认门、重部署免确认、copy 无库跳过、补登记、copy 漂移确认后更新且 doctor 复检干净、未知 id / 重复 id / **doctor 快照过期**（fix 用实时磁盘状态））。

没做：
- install / audit_skill / context_cost 仍为 todo!()（按步骤边界）。
- **SQLite IndexStore 实现（store crate）**：端口与内存实现已就位，生产接线推迟到 Phase 2（ipc 层组装 `HealthContext` 时一并落地）；当前 fix 的 `index_store: None` 路径会 Skipped 相关动作（已被测试钉死，非静默错误）。
- doctor/fix 与 adapters（目标路径解析）、store（索引快照来源）的生产接线：`HealthContext` 由调用方组装（ipc 层后续步骤）。
- fix 执行后的"重扫校验"闭环（§6.1 链尾）：engine 侧由 doctor 复检测试覆盖，UI 侧闭环待 Phase 3。

自检输出（真实结果）：
1. `cargo clippy --workspace --all-targets -- -D warnings` → `Finished dev profile in 2.00s`，EXIT=0，零警告
2. `cargo test --workspace` → EXIT=0；100 passed / 0 failed（adapters 23、domain 6、engine 59=scan 11+deploy 14+reconcile 15+doctor 6+fix 13、ports 5、store 7）
3. `grep "tauri::"`（domain/ports/adapters/engine/store 全部源文件）→ 0 匹配
4. `grep "skillbox-store" crates/ports/Cargo.toml` → 无匹配（§2.5 同层边守卫）
5. 全部源文件 ≤ 600 行（新增最大 fix.rs 551；reconcile 256、doctor 253、test_env 230、各 tests ≤ 309）
6. `cargo build --release -p skillbox-app` → `target/release/skillbox-app.exe` 5.87 MB ≤ 12 MB（与 Step 1-4 持平）

遗留风险 / 备案：
- broken_link 规则 ②（父目录 + basename 回退）依赖中央库名字唯一；正本同目录同名的极端冲突下取字典序第一（仅影响"建议"，不自动执行）。
- `hash_dir` 对 >1MB 文件近似哈希（标记 approximated）：极端构造下 CopyDrift 理论上可漏报（近似前缀相同）；v1 取舍，接受。
- doctor 对真实目录全量递归哈希，无指纹缓存对比：大 agent 目录（数千 skill）体检耗时未优化；v1 可接受（只读安全），后续可复用 ScanCache 指纹做增量。
- `NeedsConfirmation.reason` 为影响面预览文本（人读），非结构化数据；UI 直接展示即可，不做解析依赖。
- adopt 失败回滚链：收编 rename 成功但索引登记失败时目录留在中央库（doctor 会提示补登记，不丢数据）；回滚遇原位被占则保留中央库副本并 Failed 告知。
- Windows 测试环境无开发者模式：链接走 junction 全覆盖（与 symlink 分类等价，Step 4 已验证）；悬空链接构造采用"先建目标再删除"避免 junction 指向不存在路径的平台差异。

## Step 4 链接部署与显式覆盖授权（v2 修订版）

状态：完成（含 review 修订：undeploy 执行期重分类 / 标记副本移除 / Skipped 语义）

做了：
- `engine::deploy` / `engine::undeploy` 全量实现，§5 全部硬约束落地。deploy 模块按 600 行红线拆分：`deploy.rs`（类型与出口，150 行）+ `deploy/classify.rs`（来源标记与目标分类）+ `deploy/policy.rs`（授权决策表）+ `deploy/write.rs`（写入纪律原语）+ `deploy/ops.rs`（deploy/undeploy 执行）+ 单测 `deploy/tests.rs`（deploy 侧与共享设施）+ `deploy/tests_undeploy.rs`（undeploy 侧）。
- **目标分类（§5.1）**：`classify_target` 基于 `symlink_metadata()`（lstat 语义），禁止 `is_dir()`；链接经 `read_link` + 双边 `canonicalize` 判定 `LinkToSource` / `ForeignLink`（悬空链接 canonicalize 失败 → ForeignLink，正确落表）；lstat Err 且 `exists()==false` → `Absent`（AppError 未保留 io ErrorKind，用 exists 兜底）。
- **授权决策表（§5.2）**：`overwrite_authorized` 显式 16 臂 match（`RealDir × Recorded{mode:"symlink"}` 单列一臂）：Absent/LinkToSource 全策略放行；ForeignLink/RealDir/RealFile 仅 `UserConfirmed` 放行；`RealDir × Recorded{..}` 一律拒绝（"从中央库更新副本"属 UserConfirmed 领域，§5.2 括注）。附 `#[allow(clippy::match_same_arms)]`（重复 true/false 体是决策表本意）。
- **写入纪律（§5.3）**：单目标三阶段——① 全部可失败重活先落在目标同父目录的 tmp（`.skillbox-deploy-<now_ms>-<SEQ>`，同父=同卷）上，旧目标零触动；② 旧目标整体 rename 进 `trash/<now_ms>/<basename>`（`-N` 防冲突，v1 无硬删除，跨卷直接报错且原路径不动）；③ 同卷原子 rename 提交；失败路径 tmp 清理 + trash 内容尽力回滚（双保险）。写盘前 `MuteGuard` RAII mute 文件监听，提前 return/出错保证成对。
- **回退链（§5.2）**：symlink → junction（Windows）→ tracked copy；copy 必写 `.skillbox-source.json`（`SourceMarker { source, writtenAtMs }`）；junction 与 symlink 在 DB 语义上同记 Symlink 模式。
- **全有或全无**：授权拒绝发生在任何写盘之前，全部冲突收集进一次 `AppError::target_conflict`（携带逐路径 `details.conflicts`，UI"指着目录说话"）；逐工具执行失败只影响该工具（`Failed{reason}`），报告仍 Ok 返回。
- **执行期重分类（TOCTOU 防护，v2）**：deploy 与 undeploy 的执行阶段对每个目标重新 `classify_target`，绝不按 precheck 快照删除。deploy：现状超出授权 → `Failed{reason: state_changed ...}` 零写入（重试走完整 precheck 得到正确 TargetConflict），幂等判定（AlreadyUpToDate）也基于现状。undeploy：现状 ≠ precheck 预期 → `Skipped{reason:"state_changed"}`；目标已消失 → `Skipped{reason:"already_absent"}`（幂等终态，非失败）。
- **undeploy 移除范围（v2）**：只移除指向本源的链接与 `.skillbox-source.json` 标记一致的 tracked-copy 副本（`is_our_copy` 只认磁盘标记，K4；用户在副本里的改动随副本整体进 trash 可找回）；`ForeignLink` / `RealFile` / 无标记 `RealDir` 一律 `TargetConflict` 拒绝（取消部署 ≠ 删除 Skill，§10）。`ToolUndeployOutcome` 改为 `Removed / Skipped{reason} / Failed{reason}`（reason 为稳定 token，IPC/前端可分组）。
- **端口扩展**：ports 新增 `WatchMute` trait（mute/unmute 成对契约；v1 watcher 未实现，engine 经端口预留接入点）。
- 单测 14 个（真实文件系统 tempfile + 固定时钟 + `FailWriteFs` 注入 write 失败 + `RecordingWatch` 计数 + `SwapAfterPrecheckFs` 竞态注入）：**关键测试 `recorded_symlink_never_deletes_real_dir`**（DB 记 mode=symlink、磁盘是 RealDir → 断言 TargetConflict 且用户文件原样保留）、决策表 20 格逐格钉死、Absent 双模式部署、LinkToSource 幂等、ForeignLink 双策略、copy marker 内容、失败部署无部分写入且 tmp 清理、缺源 NotFound、mute/unmute 四路径（成功/幂等/失败/冲突）成对；undeploy 侧：移除我方链接保留正本、三态拒绝/幂等、**TOCTOU 竞态**（`SwapAfterPrecheckFs` 在第 2 次分类时把链接偷换成带用户数据的真实目录 → 断言 `Skipped{state_changed}`、目录原样、trash 空）、**带标记副本含用户改动整体进 trash 可找回**、**无标记 / 伪造标记（source 指向别的 skill）真实目录拒绝且原样保留**。
- 外源链接测试构造：Windows 用 junction（无权限要求），Unix 用 symlink——测试不依赖开发者模式，链接断言全部走 canonicalize 等价，平台无关。

没做：
- reconcile / doctor / fix / install / audit_skill / context_cost 仍为 todo!()（按步骤边界）。
- deploy/undeploy 与 DB（skill_targets 读写）、adapters（目标路径解析）的生产接线：请求由调用方组装（ipc 层后续步骤）；engine 本步只做纯部署语义。
- WatchMute 的真实 watcher 实现（端口已预留，接入不改 engine）。
- `ToolDeployOutcome::NeedsConfirmation` 变体保留给 IPC 层转换（deploy 内部以 `TargetConflict` 错误整体表达拒绝，未构造该变体）。

自检输出（真实结果）：
1. `cargo clippy --workspace --all-targets -- -D warnings` → `Finished dev profile in 3.46s`，EXIT=0，零警告
2. `cargo test --workspace` → EXIT=0；66 passed / 0 failed（adapters 23、domain 6、engine 25=scan 11+deploy 14、ports 5、store 7）
3. `git grep "tauri::" -- "*.rs" (排除 ipc/app)` → 0 匹配
4. `git grep "skillbox-store" -- crates/ports/Cargo.toml` → 无匹配（§2.5 同层边守卫）
5. 全部源文件 ≤ 600 行（deploy 拆分后：deploy.rs 150 / classify 71 / policy 43 / write 130 / ops 268 / tests 405 / tests_undeploy 209）
6. `cargo build --release --workspace` → `target/release/skillbox-app.exe` 5.87 MB ≤ 12 MB（与 Step 1-3 持平）

遗留风险 / 备案：
- trash 同卷 rename：跨卷 v1 直接报错（原路径不动，不做删除式搬运）。生产 trash 根 `~/.skillbox/trash` 与 agent 目录几乎必然同卷，风险低；真跨卷时用户会看到明确错误。
- undeploy 对悬空的"我方历史链接"（正本已删）同样拒绝（canonicalize 失败 → ForeignLink）：保守取向，此类清理走后续 doctor/fix 的 RemoveLink + 确认流。
- Windows 无开发者模式环境（本机即此）回退 junction 已被全部测试真实覆盖；有开发者模式时走 symlink，两者分类与回退结果一致（测试断言平台无关）。
- `cleanup_tmp` 的 `remove_dir_all` 只作用于我们自己生成的 `.skillbox-deploy-` 前缀 tmp 路径，不触及用户数据；链接 tmp 走 `remove_link`。
- 多目标重复路径、并发 deploy 同目标：v1 未定义（单进程 IPC 串行调用，先不设防）。
- TOCTOU 重分类把窗口收窄到"单目标分类→同卷原子 rename"毫秒级，但不承诺原子性；竞态残留由 reconcile/doctor 兜底（K4）。

## Step 3 有界增量扫描引擎

状态：完成

做了：
- `engine::scan` 全量实现（scan.rs 501 行 + scan/tests.rs + types.rs 重写 + benches/scan_bench.rs），§7 全部 11 条硬约束逐条落位，scan.rs 顶部清单标注精确行号。
- ScanRequest/ScanProgress/ScanReport 等类型按用户指定结构重写（types.rs）；ScanRequest 携带注入的 `fs: Arc<dyn FileSystem>` / `clock: Arc<dyn Clock>` / `cache: Option<Arc<Mutex<dyn ScanCache>>>`，不做序列化（IPC 层另设 DTO），手写 Debug。
- 手工工作池：`Mutex<PoolState>` + Condvar `wait_timeout(50ms)`，固定 worker `min(num_cpus, 4)`，全部线程在 `std::thread::scope` 内；取消后 ≤ 200ms 返回部分结果（50ms 唤醒粒度 + 目录收尾）。
- 深度硬上限 6（请求 >6 钳制）；内置 12 项黑名单（Windows 大小写不敏感 / Unix 精确）+ 用户追加；符号链接按 lstat 记 Symlink 跳过不入队；卷检查仅在 depth==0 根（Windows `GetDriveTypeW==DRIVE_FIXED`，UNC 视为非本地，Unix 跳 proc/sys/dev/mnt）。
- 增量指纹（mtime_ms, size, child_count）：命中复用缓存 name/description 免重读 SKILL.md，未命中截断读前 8KB 解析 frontmatter（BOM/CRLF/引号/截断容错）后 upsert。缓存失败降级全量读并记 warning，不中断扫描。
- 端口扩展：ports 新增 `ScanCache` trait / `CachedScanEntry` / `MemoryScanCache` / `FileSystem::is_local_volume`（windows-sys 0.59：GetDriveTypeW 在 Win32_Storage_FileSystem，DRIVE_FIXED 在 Win32_System_WindowsProgramming）；store 新增 `DbScanCache`（SQLite upsert，指纹列存完整条目 JSON，schema 兼容；`prepare_cached` 复用 statement；显式 `PRAGMA journal_mode=WAL; synchronous=NORMAL` 消除默认 DELETE+FULL 下 fsync 瓶颈）。
- 报告确定性：并发遍历顺序不稳定，skills/skipped 按路径排序、warnings 字典序，保证同磁盘状态同报告（增量 UI diff 前提）。
- FakeFs 内存文件系统（含 read_head/read_dir 计数器、last_head_limit 断言 8KB 截断、local 卷开关）+ FakeClock；12 个测试覆盖任务要求的 5 个场景（深度 8 层级/符号链接自环/黑名单/增量缓存命中读计数/取消部分返回）+ 卷跳过/8KB 截断/进度节流/不可读根降级等。
- 基准：双后端同进程对比 bench（db_cache_bench.rs），消除系统负载差异；20000 目录 DbCache 冷 710ms / 暖 518ms，MemCache 冷 537ms / 暖 495ms。RSS 增量由 scan_bench（FakeFs 内存树）单独采样：20000 目录峰值 5.7MB。5000 目录冷扫描 85ms ≤ 60s 预算。

没做：
- engine 其余 8 个函数仍为 todo!()（按步骤边界，Phase 1 后续步骤）。
- scan 与 adapters 的 roots 组装、与 store DbScanCache 的生产接线（后续步骤）。
- docs/adapters.md 交付物（Step 2 遗留，待【待复核】路径核验）。

自检输出（真实结果）：
1. `cargo clippy --workspace -D warnings` → EXIT=0，零警告
2. `cargo test --workspace` → EXIT=0；52 passed / 0 failed（adapters 23、domain 6、engine 11、ports 5、store 7）
3. `grep -rn "tauri::" crates/{domain,ports,adapters,engine,store}/ | wc -l` → 0
4. `grep "skillbox-store" crates/ports/Cargo.toml` → 无匹配（ports 不反向依赖 store，§2.5 同层边守卫）
5. 最大源文件 scan.rs 501 行 ≤ 600；tests.rs、types.rs 均在红线内
6. `cargo bench -p skillbox-store --bench db_cache_bench`（同进程对比）→ 20000 目录 DbCache 冷 710ms / 暖 518ms；MemCache 冷 537ms / 暖 495ms
7. `cargo bench -p skillbox-engine --bench scan_bench` → 20000 目录 RSS 峰值增量 5736KB ≤ 80MB
8. 基准预算复核：5000 目录 DbCache 冷 85ms ≤ 60s；余量 700 倍

遗留风险 / 备案：
- **store→ports 同层依赖**（store 的 DbScanCache 实现 ports 的 ScanCache trait）：全 workspace 唯一同层边，无环无逆向，已裁决维持方案 A，§2.5 grep 守卫已配。
- 19 vs 16 工具数目差异 **已裁决：按 19 统一，TASKBOOK 已更新**。
- 基准环境为 NVMe + NTFS，fsync 非零但偏低；机械盘 / 冷盘需重测。WAL + synchronous=NORMAL 已设，丢指纹仅重算。
- Windows 卷检查依赖 canonicalize 前缀解析，subst/网络驱动器映射盘符边界行为未专项测试。

## Step 2 Adapters 声明表

状态：完成

做了：
- `ToolAdapter` 声明结构（§4.1 字段一个不少）落在 lib.rs；表与解析拆为 `table.rs`（396 行）+ `resolve.rs`（376 行），均 ≤ 600 行红线。
- 填写 §4.2 全部内置工具声明，按形态分组排列：**CLI 6 / IDE·桌面端 5 / 编辑器插件 4 / 终端 TUI 2 / 桌面助手 2，共 19 个**（TASKBOOK §4.2 表格实际列举 19 个，已按"以表格为准"统一为 19）。
- 每个 adapter 三段式注释：路径来源依据（官方文档/仓库）/ 全局与项目级是否不对称 / 平台差异。无官方依据的 skills 子目录一律标注【待复核】（交付 docs/adapters.md 时核验）：grok / windsurf / antigravity / crush / amp / hermes 的 skills 子目录；zed 在 macOS/Windows 的目录大小写。
- 路径解析（resolve.rs，纯函数、零 IO）：
  - `PathScope { home, config }` 可注入；生产入口 `home()` → `dirs::home_dir()`，`PathScope::current()` → `dirs::config_dir()`。
  - `.config/` 前缀自动映射配置根（Linux XDG）：默认 `~/.config` 与 `XDG_CONFIG_HOME` 覆写两种情形均有测试。
  - `ToolOverrides`：覆写优先于默认表路径；覆写即视为已安装（§4.2）。
  - 项目级目录相对**项目根**解析（OpenCode 全局 `.config/opencode/skills` 与项目 `.opencode/skills` 不对称，有专项测试）。
- `is_installed()`：探测路径任一存在或被覆写；存在性检查以闭包注入（本层零 IO，engine 传 ports::FileSystem 包装）。
- `platform_matches()` / `available_on()`（纯函数可断言任意平台掩码）；`link_capability()`（Unix→Symlink / Windows→Junction / 其他→CopyOnly）。
- extra_scan_dirs 类型级只读：`ResolvedScanDir { kind: ReadOnly }`（§4.3 只发现登记、不部署，serde 输出 `"readOnly"`）。claude_code（`.claude/plugins`）与 cursor（`.cursor/extensions`）带真实 extra 目录。
- 附带改动：`PlatformMask::contains/union` 改为 `const fn`（const 声明表中 `WINDOWS.union(MACOS)` 需要）；workspace 新增 `dirs = "6"`。
- 单测 23 个（adapters），覆盖任务要求的全部场景：XDG 两种情形映射、覆写优先、平台掩码过滤（Trae 无 Linux / Amp 无原生 Windows，全表过滤也断言）、19 个 id 无重复且全部解析出非空 skills 路径、分组计数 6/5/4/2/2、Hermes 唯一 recursive_scan、只读目录 camelCase 序列化。

没做：
- engine 对 adapters 的消费（scan roots 组装等，Phase 1 后续步骤，engine 的 todo!() 未动）。
- docs/adapters.md（交付物，待【待复核】路径经官方来源核验后汇入）。

自检输出（真实结果）：
1. `cargo clippy --workspace -- -D warnings` → `Finished dev profile in 8.04s`，EXIT=0，零警告
2. `cargo test --workspace` → EXIT=0；39 passed / 0 failed（adapters 23、domain 6、ports 5、store 5）
3. `grep -rn "tauri::" crates/{domain,ports,adapters,engine,store}/ | wc -l` → 0
4. `grep "skillbox-store" crates/ports/Cargo.toml` → 无匹配（ports 不反向依赖 store）
5. 最大源文件 ports/src/prod.rs 404 行 ≤ 600；新增 table.rs 396、resolve.rs 376
6. `target/release/skillbox-app.exe` → 5.87 MB（预算 ≤ 12 MB，与 Step 1 持平）

遗留风险：
- 19 vs 16 数目差异已裁决：统一按 19，TASKBOOK 已更新。
- Trae / Trae CN 共用一条声明（两个候选目录）；若需拆成两个 adapter 改动很小。
- 【待复核】路径清单见上；Phase 3 前 UI 不应假设这些路径必然存在（engine 探测失败会优雅降级）。
- Hermes docs_url 暂为空字符串（官方文档未定），UI 需容忍空值。

## Step 1 骨架与契约

状态：完成

做了：
- 建立 7-crate workspace（domain / ports / adapters / engine / store / ipc / app），依赖方向按 §2.1 固化在各自 Cargo.toml：domain 零本地依赖；engine 只依赖 domain/ports/adapters/store；ipc 依赖 engine + tauri；app 依赖 ipc + tauri。
- domain：§3.3 全部类型（FormFactor / PlatformMask / LinkCapability / SyncMode / TargetState / ReplacePolicy / OriginKind / SkillMeta / DiscoveredSkill / ScanFingerprint / AppError / ErrorKind，另加 ToolId / SkillId / AppResult），全部 Serialize + Deserialize + Debug + Clone，全量 camelCase 序列化；含 TargetState.overwritable() 决策表、PlatformMask 位运算、AppError 的 denied/io_denied 区分（§2.3"禁止模糊 Io"落地为结构化字段）。
- ports：FileSystem / Clock / Hasher / Linker / GitClient 五个 trait（签名足以支撑 tmpfs + 假时钟单测）+ ProdFileSystem / ProdClock / ProdHasher / ProdLinker / ProdGitClient 生产实现。link_kind 先查 junction 重解析标签再查 symlink（Windows 上 std 的 is_symlink 对 junction 也返回 true 的坑已规避）。
- engine：§2.2 全部 9 个函数签名（scan / reconcile / doctor / fix / install / deploy / undeploy / audit_skill / context_cost）+ 完整契约文档注释，实现体一律 todo!()；types.rs 定义全部请求/报告类型与 CancellationToken。
- store：migration 框架（schema_migrations 版本表 + run_migrations + current_version）+ migrate_v0_to_v1（§3.2 全部 6 张表，skills.canonical_path UNIQUE，skill_targets UNIQUE(skill_id, tool_id)）。
- app：Tauri 2（v2.12.4）1440×860 窗口，前端 Vite 7 + React 19 + TS 5.9 + Tailwind 3.4 最小骨架（运行时依赖仅 3 个：react / react-dom / @tauri-apps/api）；图标经 tools/gen-icon.cjs（零依赖生成 PNG）+ tauri icon 生成全套。
- 附带：为满足构建在本机安装了 VS Build Tools 2022（MSVC 14.44）+ Windows SDK 26100（之前机器无任何可用链接器）。

没做：
- adapters 的 19 工具声明表与路径解析（Phase 1，已完成）。
- ipc 的 31 个命令实现（Phase 2；当前 register() 为直通透壳，命令清单写在文档注释里）。
- engine 全部函数实现体（按"先签约后实现"留 todo!()）。
- 前端只渲染 "SkillBox" 标题卡，未搭路由/状态管理/设计系统组件（Phase 3）。
- tauri.conf.json 的 CSP 保持 null（Phase 3 接入 IPC 时按 §8 收紧）。

自检输出（真实结果）：
1. `cargo clippy --workspace -- -D warnings` → `Finished dev profile ... in 2.00s`，EXIT=0，零警告
2. `cargo build --workspace` → `Finished dev profile ... in 8.27s`，EXIT=0
3. `cargo test --workspace` → EXIT=0；16 passed / 0 failed（domain 6、ports 5、store 5，其余 crate 暂无测试）
4. `grep -rn "tauri::" crates/domain crates/ports crates/adapters crates/engine crates/store | wc -l` → 0（注：文档注释里连字面量都清掉了，否则 grep 会计数）
5. `target/release/skillbox-app.exe` → 5.87 MB（预算 ≤ 12 MB；profile: opt-level=s + thin LTO + codegen-units=1 + strip）

DoD 补充验证：debug 版窗口进程启动后存活 8 秒正常关闭（WebView2 144.0.3719.104，渲染 "SkillBox"）。

遗留风险：
- 本机 git 状态特殊：D:\ 盘根有一个误建的 git 仓库（会拖慢全盘 git 操作），项目目录内已用独立 .git 隔离，建议用户择机删除 D:\.git。
- rusqlite 锁 0.32（bundled SQLite 3.46），后续升级需迁移测试覆盖；libsqlite3-sys 编译依赖 cc 找到 MSVC（已装好，但换机器要注意）。
- ReplacePolicy<'a> 带借用期，Phase 2 反序列化 IPC 参数时需从 &'de str 借用或提供 owned 镜像，留意 serde 借用约束。
- tauri 2.x 解析为当前最新 2.12.4；后续 minor 升级可能带来行为变化（锁文件已提交可复现）。
- 前端 build.target 用 Vite 默认（WebView2 Chromium 无兼容问题），若未来要兼容旧 WebView2 再收紧。
