# PERFORMANCE —— 扫描引擎基准基线

> 预算依据：TASKBOOK §7（5000 目录全量 ≤ 60s；峰值 RSS 增量 ≤ 80MB）。

## 测量环境

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-20 |
| OS | Windows（本机实测） |
| Rust | stable（clippy 1.99 工具链） |
| profile | bench（optimized） |
| worker 数 | min(num_cpus, 4)（§7 约束 7） |

## 真磁盘基准（DbScanCache + 真实文件系统）

> 数据来源：`crates/store/benches/db_cache_bench.rs`（`cargo bench -p skillbox-store --bench db_cache_bench`）。

| 规模（目录数） | skills | 冷扫描（空表→put） | 暖扫描（全命中→get） | 预算 | 结论 |
| ---: | ---: | ---: | ---: | --- | --- |
| 1 000 | 100 | 36 ms | 10 ms | — | — |
| 5 000 | 500 | 186 ms | 56 ms | ≤ 60 s | **通过**（耗时为预算的 0.31%） |
| 20 000 | 2 000 | 1 241 ms | 548 ms | ≤ 60 s | 通过（耗时为预算的 2.1%） |

- SQLite 配置：默认（rusqlite bundled，synchronous=NORMAL，无显式 WAL）。
- `prepare_cached` 复用 statement（2 条 SQL：get / upsert），避免每次调用 compile。
- 暖扫描 SQLite get 反而快于内存 HashMap（548ms vs 1010ms），因 `prepare_cached` 命中 + 无 JSON 反序列化开销。
- 冷扫描与内存基准持平（1.24s vs 1.24s），表明 auto-commit + page cache 足以消化 2000 次 put。

## 内存基准（MemoryScanCache + FakeFs）

> 数据来源：`crates/engine/benches/scan_bench.rs`（`cargo bench -p skillbox-engine --bench scan_bench`）。
> 排除物理盘噪声，衡量引擎自身逻辑开销 + RSS 增量。

| 规模（目录数） | skills | 冷扫描（缓存空） | 暖扫描（全命中） | 峰值 RSS 增量 | 预算 | 结论 |
| ---: | ---: | ---: | ---: | ---: | --- | --- |
| 1 000 | 100 | 42 ms | 27 ms | 804 KB | — | 远低于预算 |
| 5 000 | 500 | 219 ms | 170 ms | 1 540 KB | ≤ 60 s / ≤ 80 MB | **通过**（耗时为预算的 0.37%） |
| 20 000 | 2 000 | 1 236 ms | 1 010 ms | 5 736 KB | ≤ 80 MB（外推预算） | 通过（RSS 为预算的 7.2%） |

- 峰值 RSS 增量：`on_progress` 回调内采样进程 RSS 相对基准的增量（Windows `GetProcessMemoryInfo`）。
- 外推：RSS 增量随目录数近似线性（约 0.28 KB/目录），80MB 预算对应 ~28 万目录，安全余量充足。

## 优化储备

当前无需启用，留作 Step 8 前慢磁盘 / 超大目录树场景的优化手段：

- WAL + `synchronous=NORMAL`：丢指纹仅导致下轮重算，无正确性损失。
- 批量 flush：`DbScanCache` 内部缓冲 pending 条目（128 条/批），单事务 commit，将 2000 次 commit 降至 ~16 次。
- 两者组合预期将冷扫描从 1.2s 压至 0.3–0.5s（估算），但当前余量 50 倍无需提前消费。

## 复现

```powershell
# 真磁盘基准（推荐，测量完整 IO 路径）
cargo bench -p skillbox-store --bench db_cache_bench

# 内存基准（RSS 增量 + 逻辑开销）
cargo bench -p skillbox-engine --bench scan_bench
```
