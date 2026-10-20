# SkillBox 开发进度

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
