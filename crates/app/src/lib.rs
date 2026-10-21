//! # skillbox-app
//!
//! Tauri 入口：窗口与进程生命周期。命令注册经 skillbox-ipc；
//! AppState 在此组装（组合根：目录创建 + managed state）。

use skillbox_ipc::{AppState, AppStateInputs, PathScope};

/// `~/.skillbox` 数据根布局（§3.2）。
fn data_root() -> std::path::PathBuf {
    PathScope::current().home().join(".skillbox")
}

pub fn run() {
    let base = data_root();
    // 组合根职责：启动期确保目录结构存在（ipc / engine 不做目录创建）。
    for dir in ["skills", "cache", "trash", "logs"] {
        std::fs::create_dir_all(base.join(dir))
            .unwrap_or_else(|e| panic!("创建数据目录失败 {}: {e}", base.join(dir).display()));
    }
    let state = AppState::init(AppStateInputs {
        db_path: base.join("skillbox.db"),
        central_root: base.join("skills"),
        cache_root: base.join("cache"),
        trash_root: base.join("trash"),
        logs_dir: base.join("logs"),
    })
    .expect("SkillBox 状态初始化失败");

    let builder = tauri::Builder::default().manage(state);
    let builder = skillbox_ipc::register(builder);
    builder
        .run(tauri::generate_context!())
        .expect("SkillBox 启动失败");
}
