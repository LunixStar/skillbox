//! doc 单测：读写 / 校验规则（含行号）/.bak 轮换 / 增量重扫。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skillbox_ports::{
    IndexStore, MemoryIndexStore, OriginInfo, ProdFileSystem, ProdHasher, ProdLinker,
};
use skillbox_domain::SkillId;

use super::{
    doc_read, doc_write, rescan_after_doc_save, validate_meta, validate_text, DocIssue, SKILL_FILE,
};
use crate::doctor::hash_dir;
use crate::test_env::CountingFs;

const SKILL_BAK_FILE: &str = "SKILL.md.bak";

fn make_skill(root: &Path, name: &str, contents: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(SKILL_FILE), contents).unwrap();
    dir
}

const VALID_DOC: &str = "---\nname: demo-skill\ndescription: a demo skill for testing docs\n---\n# Demo\n\nbody text\n";

fn rules(issues: &[DocIssue]) -> Vec<&str> {
    issues.iter().map(|i| i.rule.as_str()).collect()
}

#[test]
fn doc_read_returns_full_text() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);
    let text = doc_read(&ProdFileSystem, &dir).unwrap();
    assert!(text.contains("# Demo"));
}

#[test]
fn doc_read_missing_rejected() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("empty");
    std::fs::create_dir_all(&dir).unwrap();
    let err = doc_read(&ProdFileSystem, &dir).unwrap_err();
    assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);
}

#[test]
fn validate_meta_passes_valid_doc() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);
    let report = validate_meta(&ProdFileSystem, &dir, Some("demo-skill")).unwrap();
    assert!(report.is_valid());
    assert!(report.warnings.is_empty());
}

#[test]
fn validate_meta_collects_all_errors() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "bad-skill", "no frontmatter at all");
    let report = validate_meta(&ProdFileSystem, &dir, None).unwrap();
    assert!(!report.is_valid());
    assert_eq!(rules(&report.errors), ["missing_name", "missing_description"]);
    // 无 frontmatter：锚点第 1 行。
    assert!(report.errors.iter().all(|e| e.line == 1));
}

#[test]
fn validate_meta_flags_invalid_name() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(
        root.path(),
        "demo-skill",
        "---\nname: Bad Name\ndescription: a demo skill for testing docs\n---\n",
    );
    let report = validate_meta(&ProdFileSystem, &dir, None).unwrap();
    assert_eq!(rules(&report.errors), ["invalid_name"]);
    assert_eq!(report.errors[0].line, 2, "name 键在第 2 行");
}

#[test]
fn validate_meta_warns_length_fence_and_missing_files_with_lines() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(
        root.path(),
        "demo-skill",
        "---\nname: demo-skill\ndescription: too short\n---\nsee [guide](guide.md) and [site](https://x.com) and [anchor](#top)\n```rust\nunclosed",
    );
    let report = validate_meta(&ProdFileSystem, &dir, Some("demo-skill")).unwrap();
    assert!(report.is_valid(), "告警不阻断");
    assert!(rules(&report.warnings).contains(&"short_description"));
    assert!(rules(&report.warnings).contains(&"unclosed_fence"));
    assert!(rules(&report.warnings).contains(&"missing_file"), "guide.md 不存在应告警");
    // 行号：description 第 3 行、缺失引用第 5 行、未闭合围栏起始第 6 行。
    let line_of = |rule: &str| {
        report.warnings.iter().find(|w| w.rule == rule).unwrap().line
    };
    assert_eq!(line_of("short_description"), 3);
    assert_eq!(line_of("missing_file"), 5);
    assert_eq!(line_of("unclosed_fence"), 6);
    // 外链与锚点不检查存在性。
    assert_eq!(report.warnings.iter().filter(|w| w.rule == "missing_file").count(), 1);
}

#[test]
fn validate_text_flags_unclosed_fence_at_opening_line() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);
    let text = "---\nname: demo-skill\ndescription: a demo skill for testing docs\n---\ntext\n```js\ncode\nmore\n";
    let report = validate_text(&ProdFileSystem, &dir, text, None);
    let fence = report.warnings.iter().find(|w| w.rule == "unclosed_fence").unwrap();
    assert_eq!(fence.line, 6, "未闭合围栏定位到起始行");
}

#[test]
fn validate_text_warns_name_mismatch() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);
    let report = validate_text(&ProdFileSystem, &dir, VALID_DOC, Some("other-name"));
    assert!(report.is_valid(), "不一致是告警不是错误");
    let mismatch = report.warnings.iter().find(|w| w.rule == "name_mismatch").unwrap();
    assert_eq!(mismatch.line, 2);
    // 一致时无告警。
    let ok = validate_text(&ProdFileSystem, &dir, VALID_DOC, Some("demo-skill"));
    assert!(ok.warnings.is_empty());
}

#[test]
fn doc_write_rotates_single_backup() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);

    let v2 = VALID_DOC.replace("body text", "body v2");
    doc_write(&ProdFileSystem, None, &dir, &v2, None, false).unwrap();
    let bak = dir.join(SKILL_BAK_FILE);
    assert_eq!(std::fs::read_to_string(&bak).unwrap(), VALID_DOC, "首次写入备份旧文");

    let v3 = VALID_DOC.replace("body text", "body v3");
    doc_write(&ProdFileSystem, None, &dir, &v3, None, false).unwrap();
    assert_eq!(std::fs::read_to_string(&bak).unwrap(), v2, "keep 1：只保留上一版");
    assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), v3);
}

#[test]
fn doc_write_blocks_on_validation_errors() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);

    let err = doc_write(&ProdFileSystem, None, &dir, "no frontmatter", None, false).unwrap_err();
    assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
    assert!(err.message.contains("missing_name"));
    assert!(err.message.contains("第 1 行"), "拒绝信息带行号");
    // 盘上内容与备份均未变化。
    assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), VALID_DOC);
    assert!(!dir.join(SKILL_BAK_FILE).exists());
}

#[test]
fn doc_write_dry_run_zero_writes() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);
    let (counting, writes) = CountingFs::wrap(Arc::new(ProdFileSystem));

    let v2 = VALID_DOC.replace("body text", "body v2");
    let outcome = doc_write(counting.as_ref(), None, &dir, &v2, None, true).unwrap();
    assert!(outcome.dry_run);
    assert_eq!(outcome.backup, Some(dir.join(SKILL_BAK_FILE)), "dry_run 预告将备份");
    assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0, "dry_run 零写盘");
    assert!(!outcome.index_refreshed);
    assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), VALID_DOC);
}

#[test]
fn doc_write_new_file_has_no_backup() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("brand-new");
    std::fs::create_dir_all(&dir).unwrap();
    let outcome = doc_write(&ProdFileSystem, None, &dir, VALID_DOC, None, false).unwrap();
    assert_eq!(outcome.backup, None);
    assert!(dir.join(SKILL_FILE).is_file());
}

#[test]
fn doc_write_rejects_empty_and_missing_dir() {
    let root = tempfile::tempdir().unwrap();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);
    let err = doc_write(&ProdFileSystem, None, &dir, "   \n", None, false).unwrap_err();
    assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);

    let missing = root.path().join("nope");
    let err = doc_write(&ProdFileSystem, None, &missing, VALID_DOC, None, false).unwrap_err();
    assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);
}

#[test]
fn rescan_after_doc_save_refreshes_description_and_hash_only() {
    let root = tempfile::tempdir().unwrap();
    let store = MemoryIndexStore::new();
    let dir = make_skill(root.path(), "demo-skill", VALID_DOC);

    let old_hash = hash_dir(&ProdFileSystem, &ProdLinker, &ProdHasher, &dir);
    let id = store
        .insert_skill("demo-skill", "old description", &OriginInfo::local(), &dir, old_hash.as_deref())
        .unwrap();

    // 编辑保存：换 description 正文。
    let v2 = VALID_DOC.replace(
        "a demo skill for testing docs",
        "an edited demo skill description for docs",
    );
    doc_write(&ProdFileSystem, None, &dir, &v2, Some("demo-skill"), false).unwrap();

    rescan_after_doc_save(
        &ProdFileSystem,
        &ProdLinker,
        &ProdHasher,
        &store,
        SkillId(id.0),
        &dir,
        &OriginInfo::local(),
    )
    .unwrap();

    let skills = store.skills();
    assert_eq!(skills.len(), 1);
    let row = &skills[0];
    assert_eq!(row.description, "an edited demo skill description for docs");
    assert_ne!(row.content_hash, old_hash, "内容变了哈希必须变");
    assert_eq!(row.name, "demo-skill", "名字不动");
    assert_eq!(row.id, SkillId(id.0));

    // 内容未变时再次重扫：哈希稳定（幂等）。
    rescan_after_doc_save(
        &ProdFileSystem,
        &ProdLinker,
        &ProdHasher,
        &store,
        SkillId(id.0),
        &dir,
        &OriginInfo::local(),
    )
    .unwrap();
    assert_eq!(store.skills()[0].content_hash, row.content_hash);
}
