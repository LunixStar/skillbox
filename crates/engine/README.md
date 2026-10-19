# skillbox-engine

内核：scan / reconcile / install / deploy / doctor / audit / cost。
必须能在 `cargo test` 下脱离 GUI 完整运行（磁盘真相由 ports 注入）。
允许依赖：skillbox-domain、skillbox-ports、skillbox-adapters、skillbox-store。
【硬约束】本 crate 禁止出现 `tauri::`，禁止依赖 ipc / app。
