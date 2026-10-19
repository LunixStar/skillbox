# skillbox-domain

纯数据 + 纯逻辑：领域类型与错误模型。零 IO，唯一外部依赖 serde（序列化派生）。
允许依赖：无本地 crate（依赖方向的最底层）。
【硬约束】本 crate 禁止出现 `tauri::`。
