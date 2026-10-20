# PERFORMANCE —— 扫描引擎基准基线

> 数据来源：`crates/engine/benches/scan_bench.rs`（`cargo bench -p skillbox-engine --bench scan_bench`）。
> 预算依据：TASKBOOK §7（5000 目录全量 ≤ 60s；峰值 RSS 增量 ≤ 80MB）。

## 测量环境

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-20 |
| OS | Windows（本机实测） |
| Rust | stable（clippy 1.99 工具链） |
| profile | bench（optimized） |
| 文件系统 | FakeFs 内存树（排除物理盘噪声，衡量引擎自身开销） |
| worker 数 | min(num_cpus, 4)（§7 约束 7） |

## 基准数据（每 10 个目录含 1 个 SKILL.md）

| 规模（目录数） | skills | 冷扫描（缓存空） | 暖扫描（全命中） | 峰值 RSS 增量 | 预算 | 结论 |
| ---: | ---: | ---: | ---: | ---: | --- | --- |
| 1 000 | 100 | 42 ms | 27 ms | 804 KB | — | 远低于预算 |
| 5 000 | 500 | 219 ms | 170 ms | 1 540 KB | ≤ 60 s / ≤ 80 MB | **通过**（耗时为预算的 0.37%） |
| 20 000 | 2 000 | 1 236 ms | 1 010 ms | 5 736 KB | ≤ 80 MB（外推预算） | 通过（RSS 为预算的 7.2%） |

- 冷扫描：缓存空，逐 skill 截断读 SKILL.md 前 8KB 并写入缓存（miss + write 路径）。
- 暖扫描：共享同一 `MemoryScanCache` 二次扫描，指纹全命中（hit 路径，零 SKILL.md 读取）。
- 峰值 RSS 增量：`on_progress` 回调内采样进程 RSS 相对基准的增量（Windows `GetProcessMemoryInfo`）。
- 外推：RSS 增量随目录数近似线性（约 0.28 KB/目录），80MB 预算对应 ~28 万目录，安全余量充足。

## 复现

```powershell
cargo bench -p skillbox-engine --bench scan_bench
```
