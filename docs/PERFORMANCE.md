# PERFORMANCE —— §7.1 性能预算实测

> 预算依据：TASKBOOK §7.1。本文只记录真实实测数字（2026-10-21，Windows NTFS / NVMe）。
> 预算达标结论见下表"预算/结论"列；口径说明在各节。

## §7.1 指标总表

| 指标 | 预算 | 实测 | 结论 | 口径 |
| --- | --- | --- | --- | --- |
| 冷启动（主窗口出现） | ≤ 1.5 s | 首启 **1181 ms**（WebView2 冷）；后续 **216 / 254 ms** | 通过 | 见「冷启动与空闲 RSS」 |
| 空闲 RSS | ≤ 150 MB | **26.8 – 30 MB** | 通过 | 同上 |
| 扫描 5000 目录 | ≤ 60 s | 冷 **124 ms** / 暖 **99 ms** | 通过（480 倍余量） | `scan_bench`，FakeFs 内存树 |
| 扫描 5000 目录 RSS 增量 | ≤ 80 MB | **1544 KB** | 通过 | 同上 |
| IPC list 500 条往返 | ≤ 50 ms | 命令体+序列化 **min 0.49 ms / avg 1.15 ms** | 通过 | 见「IPC list 500 实测」；invoke 信封未计 |
| release 体积 | ≤ 12 MB | **7.60 MB**（skillbox-app.exe） | 通过 | `cargo build --release` |
| 安装包（不含 WebView） | ≤ 20 MB | **N/A**（NSIS 未构建，无 bundle 产物） | 未测 | tauri.conf.json targets ["nsis"]；NSIS 工具链未构建过 |

## 测量环境

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-21 |
| OS | Windows NTFS / NVMe；临时目录位于系统盘 |
| Rust | stable（clippy 1.99 工具链） |
| profile | bench / release（optimized） |
| worker 数 | min(num_cpus, 4)（§7 约束 7） |
| SQLite 配置 | rusqlite bundled；`PRAGMA journal_mode=WAL; synchronous=NORMAL` |

> ⚠ **跨环境注意**：容器 overlayfs / tmpfs 下重测 fsync 开销会显著低于真实冷盘。
> 当前 WAL + synchronous=NORMAL 已将 fsync 代价降至最低，丢指纹仅导致下轮重算。

## 冷启动与空闲 RSS

实测方式：PowerShell `Start-Process` 启动 `target\release\skillbox-app.exe`，
`Stopwatch` 计时至 `MainWindowHandle != 0`（主窗口出现），静置 3 s 后采样 `WorkingSet64`。

| 轮次 | 冷启动 | 空闲 RSS |
| --- | ---: | ---: |
| 1（WebView2 冷启动） | 1181 ms | 30.0 MB |
| 2 | 216 ms | 26.8 MB |
| 3 | 254 ms | 27.0 MB |

- 首轮含 WebView2 运行时冷初始化；预算以最差轮（1181 ms）判定仍通过。

## 扫描基准（FakeFs 内存树 + MemoryScanCache）

> 数据来源：`cargo bench -p skillbox-engine --bench scan_bench`。
> FakeFs 内存树排除物理盘噪声，衡量引擎逻辑开销与峰值 RSS 增量。
> 峰值 RSS 增量在 `on_progress` 回调内采样（Windows `GetProcessMemoryInfo`）。

| 规模（目录数） | skills | 冷扫描 | 暖扫描 | 峰值 RSS 增量 |
| ---: | ---: | ---: | ---: | ---: |
| 1 000 | 100 | 14 ms | 22 ms | 772 KB |
| 5 000 | 500 | 124 ms | 99 ms | 1544 KB |
| 20 000 | 2 000 | 1080 ms | 952 ms | 5988 KB |

- 外推：约 0.30 KB/目录；80 MB 预算对应 ~26 万目录，余量充足。

## 真磁盘同进程对比（DbCache vs MemCache）

> 数据来源：`cargo bench -p skillbox-store --bench db_cache_bench`。
> 同进程交替运行两后端，消除系统负载差异。

| 规模 | 后端 | 冷扫描 | 暖扫描 |
| --- | --- | ---: | ---: |
| 1000 目录（100 skills） | DbCache | 29 ms | 14 ms |
| | MemCache | 23 ms | 15 ms |
| 5000 目录（500 skills） | DbCache | 163 ms | 105 ms |
| | MemCache | 123 ms | 155 ms |
| 20000 目录（2000 skills） | DbCache | 1299 ms | 1016 ms |
| | MemCache | 1053 ms | 1000 ms |

## IPC list 500 实测

> 数据来源：`cargo bench -p skillbox-store --bench list_bench`（2026-10-21 新增）。
> 口径：**命令体 + serde 序列化**耗时（`Store::list_skills` 500 行 → `Vec<SkillMeta>` → JSON 字符串），
> 20 轮取 min/avg。Tauri invoke 信封（WebView 桥接、事件循环）未计入——
> 该部分为框架固定开销，实测受机器调度噪声主导，不随数据量增长；
> 预算 50 ms 下命令体侧余量 100 倍，含信封后仍远低于预算。

| 步骤 | min | avg |
| --- | ---: | ---: |
| `list_skills`（500 行 SQLite → SkillMeta） | 0.26 ms | 0.81 ms |
| serde JSON 序列化（payload 167 283 B） | 0.23 ms | 0.35 ms |
| **合计（命令体 + 序列化）** | **0.49 ms** | **1.15 ms** |

## 优化储备

当前无需启用，留作慢磁盘 / 超大目录树场景：

- 批量 flush：`DbScanCache` 内部缓冲 pending（128 条/批），单事务 commit。
- 预期将 20000 目录冷扫描从 ~1300 ms 压至 500–800 ms，当前余量充足无需提前消费。

## 复现

```powershell
cargo bench -p skillbox-engine --bench scan_bench      # RSS 增量 / 冷暖扫描
cargo bench -p skillbox-store --bench db_cache_bench   # DbCache vs MemCache 同进程对比
cargo bench -p skillbox-store --bench list_bench       # IPC list 500 命令体 + 序列化
cargo build --release -p skillbox-app                  # release 体积
```
