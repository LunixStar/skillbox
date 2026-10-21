//! §7.1「单次 IPC 往返 list 500 条 ≤ 50ms」的 store 层实测：
//! `Store::list_skills`（500 行）+ serde JSON 序列化（IPC 载荷形态）。
//! 口径：命令体 + 序列化耗时；Tauri invoke 信封（WebView 桥）不计入，
//! 见 docs/PERFORMANCE.md 备注。

use std::path::Path;
use std::time::Instant;

use skillbox_domain::SkillMeta;
use skillbox_ports::{IndexStore, OriginInfo};
use skillbox_store::Store;

fn main() {
    let tmp = std::env::temp_dir().join(format!("skillbox-list-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db_path = tmp.join("bench.skillbox.db");

    let store = Store::open(&db_path).unwrap();
    let index = store.index_store();

    // 灌 500 条 skill 行。
    let t0 = Instant::now();
    for i in 0..500 {
        index
            .insert_skill(
                &format!("bench-skill-{i:03}"),
                &format!("bench skill number {i} with a moderately long description"),
                &OriginInfo::local(),
                Path::new(&format!(r"C:\Users\t\.skillbox\skills\bench-skill-{i:03}")),
                Some(&format!("hash-{i:03}")),
            )
            .unwrap();
    }
    let seed_ms = t0.elapsed().as_millis();

    // list 500 条 + 序列化，取 20 轮 min/avg。
    let mut min_list = 0u64;
    let mut sum_list = 0u64;
    let mut min_ser = 0u64;
    let mut sum_ser = 0u64;
    let mut bytes = 0usize;
    for round in 0..20u64 {
        let t1 = Instant::now();
        let skills: Vec<SkillMeta> = store.list_skills().unwrap();
        let list_us = t1.elapsed().as_micros() as u64;
        let t2 = Instant::now();
        let json = serde_json::to_string(&skills).unwrap();
        let ser_us = t2.elapsed().as_micros() as u64;
        bytes = json.len();
        if round == 0 || list_us < min_list {
            min_list = list_us;
        }
        if round == 0 || ser_us < min_ser {
            min_ser = ser_us;
        }
        sum_list += list_us;
        sum_ser += ser_us;
    }
    let count = store.list_skills().unwrap().len();
    println!("skills={count} seed={seed_ms}ms payload={bytes}B");
    println!(
        "list500  : min={:.2}ms avg={:.2}ms",
        min_list as f64 / 1000.0,
        sum_list as f64 / 20_000.0,
    );
    println!(
        "serde500 : min={:.2}ms avg={:.2}ms",
        min_ser as f64 / 1000.0,
        sum_ser as f64 / 20_000.0,
    );
    println!(
        "total500 : min={:.2}ms avg={:.2}ms (list + serde, 20 轮)",
        (min_list + min_ser) as f64 / 1000.0,
        (sum_list + sum_ser) as f64 / 20_000.0,
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
