use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use skillbox_domain::ToolId;
use skillbox_engine::scan;
use skillbox_engine::types::{CancellationToken, ScanRequest, ScanRoot};
use skillbox_ports::{ProdClock, ProdFileSystem, ScanCache};
use skillbox_store::DbScanCache;

fn main() {
    let tmp = std::env::temp_dir().join(format!("skillbox-db-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db_path = tmp.join("skillbox.db");

    for &tier in &[1000usize, 5000, 20000] {
        run_tier(tier, &db_path);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
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

fn run_tier(target: usize, db_path: &Path) {
    let tmp = std::env::temp_dir().join(format!("skillbox-db-bench-tree-{target}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let t0 = Instant::now();
    build_tree(&tmp, target);
    let build_secs = t0.elapsed().as_secs_f64();

    let conn = skillbox_store::open(db_path).unwrap();
    let cache: Arc<Mutex<dyn ScanCache>> = Arc::new(Mutex::new(DbScanCache::new(conn).unwrap()));
    let make_req = || ScanRequest {
        roots: vec![ScanRoot::new(tmp.clone()).for_tool(ToolId("claude_code".into()))],
        max_depth: 6,
        blacklist: Vec::new(),
        incremental: true,
        fs: Arc::new(ProdFileSystem),
        clock: Arc::new(ProdClock),
        cache: Some(cache.clone()),
    };

    let t1 = Instant::now();
    let cold = scan(make_req(), CancellationToken::new(), |_| {}).unwrap();
    let cold_ms = t1.elapsed().as_millis();

    let t2 = Instant::now();
    let warm = scan(make_req(), CancellationToken::new(), |_| {}).unwrap();
    let warm_ms = t2.elapsed().as_millis();

    println!(
        "tier {target:>5}: skills={:>4} | cold(DiskSQLite)={cold_ms:>5}ms warm={warm_ms:>5}ms | tree built in {build_secs:.1}s",
        cold.skills.len(),
    );
    assert_eq!(cold.skills.len(), warm.skills.len());
    let _ = std::fs::remove_dir_all(&tmp);
}
