//! # skillbox-app
//!
//! Tauri 入口：窗口与进程生命周期。命令注册经 skillbox-ipc（Phase 2 接入）。

pub fn run() {
    let builder = tauri::Builder::default();
    let builder = skillbox_ipc::register(builder);
    builder
        .run(tauri::generate_context!())
        .expect("SkillBox 启动失败");
}
