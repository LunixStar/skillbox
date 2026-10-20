//! scan_bench：1000 / 5000 / 20000 候选目录三档基准（§7.1 验收硬指标）。
//!
//! `harness = false`（简单计时，不引 criterion——控制依赖面与 release 体积）。
//! 运行：`cargo bench -p skillbox-engine --bench scan_bench`
//! （三档真实目录树建在系统临时目录，跑完即删；结果记录到
//! docs/PERFORMANCE.md）。

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use skillbox_domain::ToolId;
use skillbox_engine::scan;
use skillbox_engine::types::{CancellationToken, ScanRequest, ScanRoot};
use skillbox_ports::{MemoryScanCache, ProdClock, ProdFileSystem, ScanCache};

fn main() {
    for &tier in &[1000usize, 5000, 20000] {
        run_tier(tier);
    }
}

/// 建目录树：`root/g<i>/d<j>`，每 10 个目录一个 SKILL.md（带 frontmatter），
/// 每 5 个（非 skill）目录一个普通文件；候选目录数 ≈ tier。
fn build_tree(root: &Path, total: usize) {
    let per_group = 200;
    let groups = total.div_ceil(per_group);
    for g in 0..groups {
        let group = root.join(format!("g{g}"));
        fs::create_dir_all(&group).unwrap();
        let count = per_group.min(total - g * per_group);
        for j in 0..count {
            let dir = group.join(format!("d{j}"));
            fs::create_dir_all(&dir).unwrap();
            if j % 10 == 0 {
                fs::write(
                    dir.join("SKILL.md"),
                    format!(
                        "---\nname: bench-{g}-{j}\ndescription: bench skill {g}/{j}\n---\n正文。\n"
                    ),
                )
                .unwrap();
            } else if j % 5 == 0 {
                fs::write(dir.join("notes.md"), "filler content\n").unwrap();
            }
        }
    }
}

/// 当前进程 RSS（字节）。Windows：GetProcessMemoryInfo；Unix：/proc/self/status。
fn rss_bytes() -> u64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut pmc: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) };
        if ok != 0 {
            pmc.WorkingSetSize as u64
        } else {
            0
        }
    }
    #[cfg(not(windows))]
    {
        let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                return rest
                    .trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse::<u64>()
                    .unwrap_or(0)
                    * 1024;
            }
        }
        0
    }
}

fn run_tier(target: usize) {
    let base = std::env::temp_dir().join(format!(
        "skillbox-scan-bench-{target}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    let t0 = Instant::now();
    build_tree(&base, target);
    let build_secs = t0.elapsed().as_secs_f64();

    let cache: Arc<Mutex<dyn ScanCache>> = Arc::new(Mutex::new(MemoryScanCache::new()));
    let make_req = || ScanRequest {
        roots: vec![
            ScanRoot::new(base.clone()).for_tool(ToolId("claude_code".into())),
        ],
        max_depth: 6,
        blacklist: Vec::new(),
        incremental: true,
        fs: Arc::new(ProdFileSystem),
        clock: Arc::new(ProdClock),
        cache: Some(cache.clone()),
    };

    // 冷扫描（缓存全 miss + 写入）。
    let rss_before = rss_bytes();
    let peak = Arc::new(AtomicU64::new(rss_before));
    let cold_peak = peak.clone();
    let t1 = Instant::now();
    let cold = scan(make_req(), CancellationToken::new(), move |_p| {
        // §7 约束 10：进度回调里采样 RSS 峰值。
        let cur = rss_bytes();
        cold_peak.fetch_max(cur, Ordering::Relaxed);
    })
    .unwrap();
    let cold_ms = t1.elapsed().as_millis();
    let peak_delta_kb = (peak.load(Ordering::Relaxed).saturating_sub(rss_before)) / 1024;

    // 增量扫描（缓存全命中：不重读 SKILL.md）。
    let t2 = Instant::now();
    let warm = scan(make_req(), CancellationToken::new(), |_| {}).unwrap();
    let warm_ms = t2.elapsed().as_millis();

    println!(
        "tier {target:>5}: skills={:>4} skipped={:>4} warnings={} | cold={cold_ms:>5}ms warm={warm_ms:>5}ms | peakRSS+={peak_delta_kb:>5}KB (budget 80MB) | tree built in {build_secs:.1}s",
        cold.skills.len(),
        cold.skipped.len(),
        cold.warnings.len(),
    );
    assert_eq!(cold.skills.len(), warm.skills.len(), "增量结果必须与全量一致");
    let _ = fs::remove_dir_all(&base);
}
