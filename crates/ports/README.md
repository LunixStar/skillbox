# skillbox-ports

trait 抽象：FileSystem / Clock / Hasher / Linker / GitClient——唯一目的是让 engine
可在 tmpfs + 假时钟下完整单测（不碰真实磁盘）。
允许依赖：skillbox-domain。附生产实现（prod.rs）。
【硬约束】本 crate 禁止出现 `tauri::`。
