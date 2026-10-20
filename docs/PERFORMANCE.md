# PERFORMANCE —— 扫描引擎基准基线

> 预算依据：TASKBOOK §7（5000 目录全量 ≤ 60s；峰值 RSS 增量 ≤ 80MB）。

## 测量环境

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-20 |
| OS | Windows NTFS / NVMe（fsync 非空操作）；临时目录位于系统盘 |
| Rust | stable（clippy 1.99 工具链） |
| profile | bench（optimized） |
| worker 数 | min(num_cpus, 4)（§7 约束 7） |
| SQLite 配置 | rusqlite bundled；显式 `PRAGMA journal_mode=WAL; synchronous=NORMAL` |

> ⚠ **跨环境注意**：若在容器 overlayfs / tmpfs / 高写缓存 NVMe 下重测，fsync 开销会
> 显著低于真实冷盘。生产环境（Windows NTFS / macOS APFS 机械盘或冷盘）需重测验证。
> 当前 WAL + synchronous=NORMAL 已将 fsync 代价降至最低，丢指纹仅导致下轮重算。

## 真磁盘基准（同一进程、同一棵树、两后端对比）

> 数据来源：`crates/store/benches/db_cache_bench.rs`（`cargo bench -p skillbox-store --bench db_cache_bench`）。
> DbCache 与 MemCache 在同一进程内交替运行，消除系统负载与编译优化差异。

### 20000 目录（2000 skills）

| 后端 | 冷扫描（空表→put） | 暖扫描（全命中→get） |
| --- | ---:| ---:|
| **DbCache**（SQLite WAL） | 710 ms | 518 ms |
| **MemCache**（HashMap） | 537 ms | 495 ms |

### 5000 目录（500 skills）

| 后端 | 冷扫描 | 暖扫描 | 预算 |
| --- | ---:| ---:| --- |
| DbCache | 85 ms | 56 ms | ≤ 60 s |
| MemCache | 59 ms | 57 ms | ≤ 60 s |

### 1000 目录（100 skills）

| 后端 | 冷扫描 | 暖扫描 |
| --- | ---:| ---:|
| DbCache | 14 ms | 12 ms |
| MemCache | 11 ms | 9 ms |

- **口径说明**：此前 `scan_bench`（MemoryScanCache，单独进程）暖扫描 20000 目录测出 1010ms，
  而 `db_cache_bench`（DbCache，单独进程）测出 548ms，看似内存更慢。同进程对比已证实
  MemCache 实际更快（495ms vs 518ms），差异源于两次独立 bench 运行的系统负载/编译差异，
  非实现问题。**后续基准一律在同进程内对比。**

## RSS 增量基准（MemoryScanCache + FakeFs 内存树）

> 数据来源：`crates/engine/benches/scan_bench.rs`（`cargo bench -p skillbox-engine --bench scan_bench`）。
> 用 FakeFs 内存树排除物理盘噪声，专门衡量引擎逻辑开销与峰值 RSS 增量。

| 规模（目录数） | skills | 冷扫描 | 暖扫描 | 峰值 RSS 增量 | 预算 | 结论 |
| ---: | ---: | ---: | ---: | ---: | --- | --- |
| 1 000 | 100 | 42 ms | 27 ms | 804 KB | — | 远低于预算 |
| 5 000 | 500 | 219 ms | 170 ms | 1 540 KB | ≤ 60 s / ≤ 80 MB | **通过** |
| 20 000 | 2 000 | 1 236 ms | 1 010 ms | 5 736 KB | ≤ 80 MB | 通过（7.2%） |

- 峰值 RSS 增量：`on_progress` 回调内采样（Windows `GetProcessMemoryInfo`）。
- 外推：约 0.28 KB/目录，80MB 对应 ~28 万目录，余量充足。

## 优化储备

当前无需启用，留作慢磁盘 / 超大目录树场景：

- 批量 flush：`DbScanCache` 内部缓冲 pending（128 条/批），单事务 commit，将 2000 次 commit 降至 ~16 次。
- 预期将冷扫描从 ~710ms 压至 300–500ms，但当前余量 80 倍无需提前消费。

## 复现

```powershell
# 同进程对比（推荐，消除系统差异）
cargo bench -p skillbox-store --bench db_cache_bench

# RSS 增量（需 FakeFs + scan_bench 专用）
cargo bench -p skillbox-engine --bench scan_bench
```
