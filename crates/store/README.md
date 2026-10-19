# skillbox-store

SQLite 索引读写 + migration。只存索引与摘要（name/description 摘要），
禁止存放 skill 正文、frontmatter 全文、任何密钥（§3.1 硬约束）。
允许依赖：skillbox-domain。
【硬约束】本 crate 禁止出现 `tauri::`。
