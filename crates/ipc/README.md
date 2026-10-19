# skillbox-ipc

Tauri 命令层（薄）：校验参数 → 调 engine → 序列化。≤ 32 个命令（§8），
无业务逻辑。32 个命令的实现在 Phase 2 填充。
允许依赖：skillbox-engine、skillbox-domain、tauri。
【约束】ipc 不许直接调 adapters 拼路径，必须走 engine（§2.1）。
