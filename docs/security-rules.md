# SkillBox 安全扫描规则（docs/security-rules.md）

> 来源代码：`crates/engine/src/audit.rs`（`audit_skill` 纯静态审计）。
> 硬约束：审计**绝不执行 skill 内任何脚本**。唯一 IO 端口是 `FileSystem`（读目录/读文件）与 `Linker::read_link`（读链接目标），依赖图中不存在任何可被 skill 触达的执行面（无 `GitClient`、无 spawn 能力端口）。单测 `audit_is_pure_static_zero_writes` 用零写入计数文件系统证明这一点。
> 所有规则**只提示、不阻断安装**（§9.1）；发现按 Critical > Warning > Info 汇总为 risk level（High / Medium / Low）。

## 扫描范围与防御性上限

| 参数 | 值 | 说明 |
|------|----|------|
| 递归深度上限 | 6 | skill 是小包 |
| 最多扫描文件数 | 400 | 超出后剩余文件不读内容（名称/链接规则仍生效） |
| 单文件大小上限 | 512 KB | 超出只按名称/链接规则处理 |
| 命中片段截断 | 120 字符 | 按字符截断，中文不被截半 |
| 隐藏目录 | 不递归 | 标记 `hidden_files` 后不进入内部 |
| `node_modules` | 不递归 | 依赖内容与 skill 本体无关 |
| 非 UTF-8 / 二进制 | 静默跳过 | 扫描不因个别坏文件失败 |

## 八条规则

### 1. shell_exec（Warning）— 执行 shell 命令

**匹配模式**（行内子串，大小写不敏感）：`bash -c`、`sh -c`、`zsh -c`、`dash -c`、`ksh -c`、`powershell -command`、`pwsh -command`、`powershell -enc`、`cmd /c`、`| bash`、`|bash`、`| sh`、`|sh`、`| zsh`、`|zsh`、`| python`、`|python`（管道进 shell / 解释器）。

**误报**：文档中示例命令同样命中（markdown 文件不豁免本规则）；如 `grep "| sh"` 之类的字面字符串也会命中。命中即建议人工审阅该行。

### 2. network_egress（Info）— 网络访问

**匹配模式**：
- 联网命令（任何文件）：`curl `、`wget `、`invoke-webrequest`、`invoke-restmethod`、`iwr `、`irm `、`requests.post/get/put/delete`、`urllib.request`、`http.client`、`net::http`、`fetch(`；
- 裸 URL（**仅非 markdown 文件**）：`http://`、`https://`、`ftp://`。

**误报**：markdown 文档中的 URL 引用不标记（豁免）；非 markdown 文件中的注释/示例 URL 会命中。`curl ` 带尾随空格或制表符，避免 `curly` 之类单词误报。

### 3. credential_access（Critical）— 读取凭据

**匹配模式**（三组，任一命中）：
- 路径/文件名子串：`~/.ssh`、`/.ssh/`、`id_rsa`、`id_ed25519`、`~/.aws`、`.aws/credentials`、`aws_access_key_id`、`aws_secret_access_key`、`.netrc`、`.kube/config`、`keychain`、`credentials.json`；
- `.env` 文件引用，**带词边界**：`.env` 后跟的字符不能是字母/数字（`.environment`、`.envrc` 不算——注意 `.envrc` 因后跟 `r` 被豁免）；
- `process.env` + 敏感词组合：同行出现 `process.env` 且含 `token` / `secret` / `password` / `passwd` / `credential` / `_key` / `apikey` 之一。

**误报**：普通 `process.env.PATH` 之类不带敏感词不命中；`id_rsa` 出现在文档说明中同样命中（ Critical 提示人工确认）。

### 4. path_traversal（Warning）— 路径越界（仅非 markdown 文件）

**匹配模式**：`../../`、`..\..`、`/etc/`、`/usr/`、`/var/`、`/bin/`、`/sbin/`、`/root/`、`c:\windows`、`c:/windows`。

**误报**：markdown 文件完全豁免（文档讨论系统路径不算）；脚本中合法的安装路径（如 `/usr/local/bin` 的安装说明脚本）会命中。

### 5. obfuscation（Critical）— 混淆代码

**匹配模式**（任一命中）：
- 子串：`base64 -d`、`base64 --decode`、`base64 -D`、`openssl base64`、`eval(`、`exec(eval`、`atob(`、`frombase64string`、`certutil -decode`；
- 连续十六进制串 ≥ 32 字符（典型混淆载荷形态）。

**误报**：长 hash（SHA-256 等 64 位十六进制）会命中十六进制规则；`atob(` 在正常 JS 解码场景也命中——均提示人工审阅。

### 6. symlink_escape（Critical）— 符号链接越界

**判定**：skill 包内链接（Windows junction 与 symlink 同样处理，一律不跟随、只读目标）的解析目标位于 skill 根之外；**无法解析的链接同样标记**。

**误报**：极少的相对链接解析假阴性/假阳性可能（相对目标按链接父目录解析）；无法解析的链接统一按 Critical 提示，由用户确认是否环境问题。

### 7. hidden_files（Info）— 隐藏文件

**判定**：名称以 `.` 开头的文件或目录（`.gitignore`、`.git/` 等）。隐藏目录标记后**不递归**（内部文件不参与内容规则）。

**误报**：`.gitignore`、`.DS_Store` 等常见无害文件同样提示（Info 级，仅告知存在）。

### 8. excessive_scope（Warning）— 过宽的权限声明

**判定**：仅 SKILL.md frontmatter（首行 `---` 到下一个 `---` 之间）；键为 `tools` / `allowed-tools` / `allowed_tools` / `permissions` / `scope` 之一，且值含 `*`、或值为 `all` / 以 `all ` 开头。

**误报**：值为 `*` 的合法通配场景（确实想放开全部工具）会命中——规则目的是提示用户确认。

## risk level 汇总

| 条件 | risk level |
|------|-----------|
| 任一 Critical 发现 | High |
| 无 Critical、有 Warning | Medium |
| 仅 Info / 无发现 | Low |
