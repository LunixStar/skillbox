# SkillBox 开发进度

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
