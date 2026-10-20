//! scan 引擎单测：FakeFileSystem（内存树）+ FakeClock，全部 IO 经端口
//! 注入，不碰真实磁盘（§2.1 要求 engine 在 tmpfs / 假实现下完整可测）。
//!
//! 必测场景（任务要求）：深度上限（8 层嵌套，断言第 7 层未扫）、符号链接
//! 自环不死循环、黑名单、增量缓存命中（FakeFS 读计数证明）、取消返回
//! 部分结果。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use skillbox_domain::{AppError, AppResult, ToolId};
use skillbox_ports::{
    Clock, EntryKind, EntryMeta, FileSystem, MemoryScanCache, ScanCache,
};

use super::parse_frontmatter_head;
use crate::types::{CancellationToken, ScanRequest, ScanRoot, SkipReason};
use crate::scan::scan;

// ---------------------------------------------------------------------------
// FakeFileSystem / FakeClock
// ---------------------------------------------------------------------------

/// 内存文件系统：只实现扫描所需的读路径（lstat / read_dir / read_head /
/// 卷判定），写方法一律返回错误。计数器记录 `read_head` / `read_dir`
/// 调用次数——增量缓存命中验证的观测点。
struct FakeFs {
    /// 目录 → 直接子项 lstat 元数据。
    dirs: HashMap<PathBuf, Vec<EntryMeta>>,
    /// 路径 → 自身 lstat 元数据。
    metas: HashMap<PathBuf, EntryMeta>,
    /// 文件路径 → 内容（read_head / read_to_string 数据源）。
    files: HashMap<PathBuf, Vec<u8>>,
    /// `is_local_volume` 的返回值（默认 true）。
    local: bool,
    read_head_calls: AtomicUsize,
    read_dir_calls: AtomicUsize,
    last_head_limit: AtomicU64,
}

impl FakeFs {
    fn new() -> Self {
        FakeFs {
            dirs: HashMap::new(),
            metas: HashMap::new(),
            files: HashMap::new(),
            local: true,
            read_head_calls: AtomicUsize::new(0),
            read_dir_calls: AtomicUsize::new(0),
            last_head_limit: AtomicU64::new(0),
        }
    }

    /// 登记到父目录的子项列表（父未登记则跳过——根没有父）。
    fn register_child(&mut self, path: &Path, meta: EntryMeta) {
        if let Some(parent) = path.parent() {
            if let Some(children) = self.dirs.get_mut(parent) {
                children.push(meta);
            }
        }
    }

    fn dir(&mut self, path: impl AsRef<Path>) {
        let path = PathBuf::from(path.as_ref());
        let meta = EntryMeta {
            name: basename(&path),
            kind: EntryKind::Dir,
            len: 4096,
            modified_ms: 1000,
        };
        self.metas.insert(path.clone(), meta.clone());
        self.register_child(&path, meta);
        self.dirs.entry(path).or_default();
    }

    fn file(&mut self, path: impl AsRef<Path>, contents: impl Into<Vec<u8>>) {
        let path = PathBuf::from(path.as_ref());
        let contents = contents.into();
        let meta = EntryMeta {
            name: basename(&path),
            kind: EntryKind::File,
            len: contents.len() as u64,
            modified_ms: 1000,
        };
        self.metas.insert(path.clone(), meta.clone());
        self.register_child(&path, meta);
        self.files.insert(path, contents);
    }

    /// 符号链接条目（lstat 语义；扫描器不得跟随——目标是/父级自环）。
    fn symlink(&mut self, path: impl AsRef<Path>) {
        let path = PathBuf::from(path.as_ref());
        let meta = EntryMeta {
            name: basename(&path),
            kind: EntryKind::Symlink,
            len: 32,
            modified_ms: 1000,
        };
        self.metas.insert(path.clone(), meta.clone());
        self.register_child(&path, meta);
    }

    fn read_head_calls(&self) -> usize {
        self.read_head_calls.load(Ordering::Relaxed)
    }
}

fn basename(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn not_found(path: &Path) -> AppError {
    AppError::not_found(format!("FakeFs 无此路径: {}", path.display()))
}

impl FileSystem for FakeFs {
    fn exists(&self, path: &Path) -> bool {
        self.metas.contains_key(path) || self.dirs.contains_key(path)
    }

    fn symlink_metadata(&self, path: &Path) -> AppResult<EntryMeta> {
        self.metas.get(path).cloned().ok_or_else(|| not_found(path))
    }

    fn read_dir(&self, path: &Path) -> AppResult<Vec<EntryMeta>> {
        self.read_dir_calls.fetch_add(1, Ordering::Relaxed);
        self.dirs.get(path).cloned().ok_or_else(|| not_found(path))
    }

    fn read_head(&self, path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
        self.read_head_calls.fetch_add(1, Ordering::Relaxed);
        self.last_head_limit.store(max_bytes, Ordering::Relaxed);
        let data = self.files.get(path).ok_or_else(|| not_found(path))?;
        Ok(data[..(data.len().min(max_bytes as usize))].to_vec())
    }

    fn read_to_string(&self, path: &Path) -> AppResult<String> {
        let data = self.files.get(path).ok_or_else(|| not_found(path))?;
        Ok(String::from_utf8_lossy(data).into_owned())
    }

    fn is_local_volume(&self, _path: &Path) -> bool {
        self.local
    }

    // 写路径：扫描测试只读，写方法一律拒绝。
    fn write(&self, _path: &Path, _contents: &[u8]) -> AppResult<()> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn create_dir_all(&self, _path: &Path) -> AppResult<()> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn rename(&self, _from: &Path, _to: &Path) -> AppResult<()> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn copy_file(&self, _from: &Path, _to: &Path) -> AppResult<u64> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn copy_dir_all(&self, _from: &Path, _to: &Path) -> AppResult<()> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn remove_dir(&self, _path: &Path) -> AppResult<()> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn remove_dir_all(&self, _path: &Path) -> AppResult<()> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn remove_file(&self, _path: &Path) -> AppResult<()> {
        Err(AppError::internal("FakeFs 只读"))
    }

    fn canonicalize(&self, path: &Path) -> AppResult<PathBuf> {
        Ok(path.to_path_buf())
    }
}

/// 假时钟：固定时间，扫描 duration 可确定性断言。
struct FakeClock(AtomicI64);

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// 组装辅助
// ---------------------------------------------------------------------------

/// 基准请求：max_depth 6、无缓存、无用户黑名单、固定时钟 1000ms。
fn base_req(fs: Arc<dyn FileSystem>) -> ScanRequest {
    ScanRequest {
        roots: Vec::new(),
        max_depth: 6,
        blacklist: Vec::new(),
        incremental: false,
        fs,
        clock: Arc::new(FakeClock(AtomicI64::new(1000))),
        cache: None,
    }
}

fn cache_handle() -> Arc<Mutex<dyn ScanCache>> {
    Arc::new(Mutex::new(MemoryScanCache::new()))
}

fn skill_md(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n正文内容。\n")
}

// ---------------------------------------------------------------------------
// 必测场景
// ---------------------------------------------------------------------------

/// 必测 1：深度上限——8 层嵌套（level 0..7），断言第 7 层未扫。
/// max_depth 请求 > 6 时被钳制到 6（§7 约束 2）。
#[test]
fn depth_limit_blocks_level_seven() {
    let mut fake = FakeFs::new();
    let mut path = PathBuf::from("/t/r");
    fake.dir(&path);
    // l1..l7 逐层嵌套，每层带 SKILL.md（8 层 = 根 + 7 层）。
    for level in 1..=7 {
        path = path.join(format!("l{level}"));
        fake.dir(&path);
        fake.file(path.join("SKILL.md"), skill_md(&format!("skill-l{level}"), "d"));
    }
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    // 故意请求超限深度：必须被钳制到 6。
    req.max_depth = 10;
    req.roots = vec![ScanRoot::new("/t/r")];

    let report = scan(req, CancellationToken::new(), |_| {}).unwrap();

    let nested = |level: u32| -> PathBuf {
        let mut p = PathBuf::from("/t/r");
        for i in 1..=level {
            p = p.join(format!("l{i}"));
        }
        p
    };
    // level 1..=6 的 skill 全部发现。
    for level in 1..=6 {
        assert!(
            report.skills.iter().any(|s| s.path == nested(level)),
            "level {level} 必须被发现"
        );
    }
    // 第 7 层不扫：不出现在 skills，且以 TooDeep 记录在 skipped。
    assert!(
        !report.skills.iter().any(|s| s.path == nested(7)),
        "第 7 层必须被深度上限拦截"
    );
    assert!(report
        .skipped
        .iter()
        .any(|s| s.path == nested(7) && s.reason == SkipReason::TooDeep));
}

/// 必测 2：符号链接自环不死循环；链接条目以 Symlink 记录跳过（§7 约束 4）。
#[test]
fn symlink_self_cycle_terminates() {
    let mut fake = FakeFs::new();
    fake.dir("/t/r");
    fake.dir("/t/r/a");
    fake.file("/t/r/a/SKILL.md", skill_md("a", "含自环链接的 skill"));
    // a/loop 指向 a 自身（真实文件系统上跟随它 = 无限循环）。
    fake.symlink("/t/r/a/loop");
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    req.roots = vec![ScanRoot::new("/t/r")];

    let report = scan(req, CancellationToken::new(), |_| {}).unwrap();

    assert!(report
        .skipped
        .iter()
        .any(|s| s.path == Path::new("/t/r/a/loop") && s.reason == SkipReason::Symlink));
    assert!(report.skills.iter().any(|s| s.dir_name == "a"));
}

/// 必测 3：黑名单——内置项与用户追加项都跳过且记录（§7 约束 3）。
#[test]
fn blacklist_skips_builtin_and_user_entries() {
    let mut fake = FakeFs::new();
    fake.dir("/t/r");
    for name in ["node_modules", ".git", "mine", "keep"] {
        fake.dir(format!("/t/r/{name}"));
        fake.file(format!("/t/r/{name}/SKILL.md"), skill_md(name, "d"));
    }
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    req.roots = vec![ScanRoot::new("/t/r")];
    req.blacklist = vec!["mine".into()];

    let report = scan(req, CancellationToken::new(), |_| {}).unwrap();

    // 黑名单目录里的 SKILL.md 一个都不读（只发现 keep）。
    assert_eq!(report.skills.len(), 1);
    assert_eq!(report.skills[0].dir_name, "keep");
    for name in ["node_modules", ".git", "mine"] {
        assert!(report
            .skipped
            .iter()
            .any(|s| s.path == Path::new(format!("/t/r/{name}").as_str())
                && s.reason == SkipReason::Blacklisted),);
    }
}

/// 必测 4：增量缓存命中——第二次扫描不重读 SKILL.md（FakeFS read_head
/// 计数不变），指纹变化（子项 size 变）后正确失效重读。
#[test]
fn incremental_cache_hit_skips_skill_md_reads() {
    let mut fake = FakeFs::new();
    fake.dir("/t/r");
    for name in ["a", "b", "c"] {
        fake.dir(format!("/t/r/{name}"));
        fake.file(format!("/t/r/{name}/SKILL.md"), skill_md(name, "增量测试"));
    }
    let mut fake = Arc::new(fake);
    let cache = cache_handle();
    let mut req = base_req(fake.clone());
    req.roots = vec![ScanRoot::new("/t/r")];
    req.incremental = true;
    req.cache = Some(cache.clone());

    // 第一次：缓存空，每个 SKILL.md 各读一次。
    let first = scan(req, CancellationToken::new(), |_| {}).unwrap();
    assert_eq!(first.skills.len(), 3);
    assert_eq!(fake.read_head_calls(), 3, "冷扫描每 skill 恰好读一次");
    for skill in &first.skills {
        assert!(skill.name.is_some(), "frontmatter name 必须解析出");
    }

    // 第二次：指纹未变 → 全部命中，零新增 SKILL.md 读取。
    let mut req = base_req(fake.clone());
    req.roots = vec![ScanRoot::new("/t/r")];
    req.incremental = true;
    req.cache = Some(cache.clone());
    let second = scan(req, CancellationToken::new(), |_| {}).unwrap();
    assert_eq!(
        fake.read_head_calls(),
        3,
        "增量命中不得重读 SKILL.md（缓存命中需读计数证明）"
    );
    assert_eq!(second.skills.len(), 3);
    assert_eq!(second.skills, first.skills, "命中结果与全量解析一致");

    // 指纹变化（skill-b 内容变长 → 子项 size 变）→ 仅 b 失效重读。
    Arc::get_mut(&mut fake)
        .expect("scan 请求已释放，FakeFs 应唯一持有")
        .file("/t/r/b/SKILL.md", skill_md("b-renamed", "内容已变化，长度不同"));
    let mut req3 = base_req(fake.clone());
    req3.roots = vec![ScanRoot::new("/t/r")];
    req3.incremental = true;
    req3.cache = Some(cache);
    let third = scan(req3, CancellationToken::new(), |_| {}).unwrap();
    assert_eq!(fake.read_head_calls(), 4, "仅指纹变化的 skill-b 重读");
    let b = third.skills.iter().find(|s| s.dir_name == "b").unwrap();
    assert_eq!(b.name.as_deref(), Some("b-renamed"));
}

/// 必测 5：取消返回部分结果——首个进度回调后取消，发现数远小于全量；
/// 预取消令牌立即返回空报告。
#[test]
fn cancel_returns_partial_report() {
    let mut fake = FakeFs::new();
    fake.dir("/t/r");
    for i in 0..300 {
        let d = format!("/t/r/s{i}");
        fake.dir(d.clone());
        fake.file(format!("{d}/SKILL.md"), skill_md(&format!("s{i}"), "取消测试"));
    }
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    req.roots = vec![ScanRoot::new("/t/r")];

    // 首次进度事件（scanned = 50）触发取消。
    let token = CancellationToken::new();
    let trigger = token.clone();
    let report = scan(req, token, move |p| {
        if p.scanned >= 50 {
            trigger.cancel();
        }
    })
    .unwrap();
    assert!(!report.skills.is_empty(), "取消前已完成的部分必须返回");
    assert!(report.skills.len() < 300, "取消后不得继续扫完全部 300 个");

    // 预取消：一个目录都不处理。
    let mut fake2 = FakeFs::new();
    fake2.dir("/t/r2");
    fake2.file("/t/r2/SKILL.md", skill_md("x", "d"));
    let mut req2 = base_req(Arc::new(fake2));
    req2.roots = vec![ScanRoot::new("/t/r2")];
    let empty = scan(req2, CancellationToken::cancelled(), |_| {}).unwrap();
    assert!(empty.skills.is_empty());
    assert!(empty.skipped.is_empty());
    assert!(empty.warnings.is_empty());
    assert_eq!(empty.duration_ms, 0, "假时钟下 duration 为 0");
}

// ---------------------------------------------------------------------------
// 补充场景
// ---------------------------------------------------------------------------

/// 非本卷根被跳过（§7 约束 5）。
#[test]
fn non_local_volume_root_is_skipped() {
    let mut fake = FakeFs::new();
    fake.dir("/t/r");
    fake.file("/t/r/SKILL.md", skill_md("x", "d"));
    fake.local = false;
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    req.roots = vec![ScanRoot::new("/t/r")];

    let report = scan(req, CancellationToken::new(), |_| {}).unwrap();

    assert!(report.skills.is_empty(), "非本卷根内的 skill 不得发现");
    assert!(report
        .skipped
        .iter()
        .any(|s| s.path == Path::new("/t/r") && s.reason == SkipReason::NonLocalVolume));
}

/// 进度节流：恰好每 50 个目录 emit 一次，scanned 序列单调完整（§7 约束 9）。
#[test]
fn progress_emits_every_fifty_dirs() {
    let mut fake = FakeFs::new();
    fake.dir("/t/r");
    for i in 0..200 {
        let d = format!("/t/r/d{i}");
        fake.dir(d.clone());
        fake.file(format!("{d}/SKILL.md"), skill_md(&format!("d{i}"), "进度"));
    }
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    req.roots = vec![ScanRoot::new("/t/r")];

    let seen: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let report = scan(req, CancellationToken::new(), move |p| {
        sink.lock().unwrap().push(p.scanned);
    })
    .unwrap();

    assert_eq!(report.skills.len(), 200);
    assert_eq!(*seen.lock().unwrap(), vec![50, 100, 150, 200]);
}

/// 超限 SKILL.md 截断到 8 KB 仍解析出 frontmatter；read_head 以 8192 上限调用。
#[test]
fn oversized_skill_md_is_truncated_to_8kb() {
    let mut fake = FakeFs::new();
    fake.dir("/t/r");
    fake.dir("/t/r/big");
    // frontmatter 在头部，后接 20 KB 填充——只有前 8 KB 会被读。
    let body = "x".repeat(20 * 1024);
    fake.file("/t/r/big/SKILL.md", format!("---\nname: big\n description: d\n---\n{body}"));
    let fake = Arc::new(fake);
    let mut req = base_req(fake.clone());
    req.roots = vec![ScanRoot::new("/t/r")];

    let report = scan(req, CancellationToken::new(), |_| {}).unwrap();

    assert_eq!(fake.last_head_limit.load(Ordering::Relaxed), 8192, "read_head 上限必须是 8KB");
    let big = report.skills.iter().find(|s| s.dir_name == "big").unwrap();
    assert_eq!(big.name.as_deref(), Some("big"));
}

/// 发现结果携带 tool_id / read_only 标记（后续部署授权的依据）。
#[test]
fn discovery_carries_tool_and_read_only_flags() {
    let mut fake = FakeFs::new();
    fake.dir("/t/claude");
    fake.dir("/t/claude/a");
    fake.file("/t/claude/a/SKILL.md", skill_md("a", "工具根"));
    fake.dir("/t/cache");
    fake.dir("/t/cache/b");
    fake.file("/t/cache/b/SKILL.md", skill_md("b", "只读根"));
    fake.dir("/t/lib");
    fake.dir("/t/lib/c");
    fake.file("/t/lib/c/SKILL.md", skill_md("c", "中央库"));
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    req.roots = vec![
        ScanRoot::new("/t/claude").for_tool(ToolId("claude_code".into())),
        ScanRoot::new("/t/cache").for_tool(ToolId("claude_code".into())).read_only(),
        ScanRoot::new("/t/lib"),
    ];

    let report = scan(req, CancellationToken::new(), |_| {}).unwrap();

    let a = report.skills.iter().find(|s| s.dir_name == "a").unwrap();
    assert_eq!(a.tool_id.as_ref().map(|t| t.0.as_str()), Some("claude_code"));
    assert!(!a.read_only);
    let b = report.skills.iter().find(|s| s.dir_name == "b").unwrap();
    assert!(b.read_only, "extra_scan_dirs 根必须是只读");
    let c = report.skills.iter().find(|s| s.dir_name == "c").unwrap();
    assert!(c.tool_id.is_none(), "中央库根无 tool_id");
}

/// 不可读根降级为 warning，不中断其余扫描。
#[test]
fn unreadable_root_degrades_to_warning() {
    let mut fake = FakeFs::new();
    fake.dir("/t/ok");
    fake.file("/t/ok/SKILL.md", skill_md("ok", "正常"));
    // /t/broken 只在 metas 里伪造一个"存在但不可枚举"的目录：不登记 dirs。
    fake.metas.insert(
        PathBuf::from("/t/broken"),
        EntryMeta { name: "broken".into(), kind: EntryKind::Dir, len: 0, modified_ms: 0 },
    );
    let fake = Arc::new(fake);
    let mut req = base_req(fake);
    req.roots = vec![ScanRoot::new("/t/ok"), ScanRoot::new("/t/broken")];

    let report = scan(req, CancellationToken::new(), |_| {}).unwrap();

    assert_eq!(report.skills.len(), 1);
    assert!(
        report.warnings.iter().any(|w| w.contains("broken")),
        "不可读根必须有 warning: {warnings:?}",
        warnings = report.warnings
    );
}

// ---------------------------------------------------------------------------
// parse_frontmatter_head 纯函数
// ---------------------------------------------------------------------------

#[test]
fn frontmatter_parsing_variants() {
    // 标准块。
    assert_eq!(
        parse_frontmatter_head(b"---\nname: foo\ndescription: bar baz\n---\nbody"),
        (Some("foo".into()), Some("bar baz".into()))
    );
    // 引号包裹与首现优先。
    assert_eq!(
        parse_frontmatter_head(b"---\nname: \"quoted\"\nname: second\ndescription: 'sq'\n---\n"),
        (Some("quoted".into()), Some("sq".into()))
    );
    // CRLF + BOM。
    assert_eq!(
        parse_frontmatter_head("\u{feff}---\r\nname: crlf\r\ndescription: d\r\n---\r\n".as_bytes()),
        (Some("crlf".into()), Some("d".into()))
    );
    // 无 frontmatter / 畸形。
    assert_eq!(parse_frontmatter_head(b"# just markdown"), (None, None));
    assert_eq!(parse_frontmatter_head(b"---\nname: unclosed"), (Some("unclosed".into()), None));
    // 缺 description。
    assert_eq!(
        parse_frontmatter_head(b"---\nname: only\n---\n"),
        (Some("only".into()), None)
    );
}
