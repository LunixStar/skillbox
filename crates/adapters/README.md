# skillbox-adapters

agent 声明表（ToolAdapter）+ 路径解析，纯函数：输入 home/config_dir，输出路径，
不做任何文件系统访问。16 个工具声明表与 is_installed 探测在 Phase 1 填充。
允许依赖：skillbox-domain。
【硬约束】本 crate 禁止出现 `tauri::`。
