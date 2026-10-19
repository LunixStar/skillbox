# SkillBox 开发进度

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
- adapters 的 16 工具声明表与路径解析（Phase 1）。
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
