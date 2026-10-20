use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use skillbox_domain::ToolId;
use skillbox_engine::scan;
use skillbox_engine::types::{CancellationToken, ScanRequest, ScanRoot};
use skillbox_ports::{MemoryScanCache, ProdClock, ProdFileSystem, ScanCache};
use skillbox_store::DbScanCache;

fn main() {
    let tmp = std::env::temp_dir().join(format!("skillbox-db-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    for &tier in &[1000usize, 5000, 20000] {
        println!("--- tier {tier} ---");
        run_tier_both(tier, &tmp);
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

fn build_tree(root: &Path, total: usize) {
    let per_group = 200;
    let groups = total.div_ceil(per_group);
    for g in 0..groups {
        let group = root.join(format!("g{g}"));
        std::fs::create_dir_all(&group).unwrap();
        let count = per_group.min(total - g * per_group);
        for j in 0..count {
            let dir = group.join(format!("d{j}"));
            std::fs::create_dir_all(&dir).unwrap();
            if j % 10 == 0 {
                std::fs::write(
                    dir.join("SKILL.md"),
                    format!("---\nname: bench-{g}-{j}\ndescription: bench skill {g}/{j}\n---\n正文。\n"),
                ).unwrap();
            } else if j % 5 == 0 {
                std::fs::write(dir.join("notes.md"), "filler\n").unwrap();
            }
        }
    }
}

fn run_tier_both(target: usize, base: &Path) {
    let tree = base.join(format!("tree-{target}"));
    let _ = std::fs::remove_dir_all(&tree);
    let t0 = Instant::now();
    build_tree(&tree, target);
    let build_secs = t0.elapsed().as_secs_f64();

    let make_req = |cache: Arc<Mutex<dyn ScanCache>>| ScanRequest {
        roots: vec![ScanRoot::new(tree.clone()).for_tool(ToolId("claude_code".into()))],
        max_depth: 6,
        blacklist: Vec::new(),
        incremental: true,
        fs: Arc::new(ProdFileSystem),
        clock: Arc::new(ProdClock),
        cache: Some(cache),
    };

    // --- DbScanCache ---
    let db_path = base.join(format!("db-{target}.skillbox.db"));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    let conn = skillbox_store::open(&db_path).unwrap();
    let db_cache: Arc<Mutex<dyn ScanCache>> = Arc::new(Mutex::new(DbScanCache::new(conn).unwrap()));

    let t1 = Instant::now();
    let cold_db = scan(make_req(db_cache.clone()), CancellationToken::new(), |_| {}).unwrap();
    let cold_db_ms = t1.elapsed().as_millis();

    let t2 = Instant::now();
    let warm_db = scan(make_req(db_cache.clone()), CancellationToken::new(), |_| {}).unwrap();
    let warm_db_ms = t2.elapsed().as_millis();

    println!(
        "  DbCache  : skills={:>4} | cold={cold_db_ms:>5}ms warm={warm_db_ms:>5}ms",
        cold_db.skills.len(),
    );

    // --- MemoryScanCache ---
    let mem_cache: Arc<Mutex<dyn ScanCache>> = Arc::new(Mutex::new(MemoryScanCache::new()));

    let t3 = Instant::now();
    let cold_mem = scan(make_req(mem_cache.clone()), CancellationToken::new(), |_| {}).unwrap();
    let cold_mem_ms = t3.elapsed().as_millis();

    let t4 = Instant::now();
    let warm_mem = scan(make_req(mem_cache.clone()), CancellationToken::new(), |_| {}).unwrap();
    let warm_mem_ms = t4.elapsed().as_millis();

    println!(
        "  MemCache : skills={:>4} | cold={cold_mem_ms:>5}ms warm={warm_mem_ms:>5}ms | tree={build_secs:.1}s",
        cold_mem.skills.len(),
    );

    assert_eq!(cold_db.skills.len(), warm_db.skills.len());
    assert_eq!(cold_mem.skills.len(), warm_mem.skills.len());

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    let _ = std::fs::remove_dir_all(&tree);
}
