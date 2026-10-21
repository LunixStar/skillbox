//! doc —— SKILL.md 文档读写与元数据校验（TASKBOOK §8 `doc` 资源组）。
//!
//! - `doc_read`：读正本全文（SKILL.md）。
//! - `doc_write`：**先校验后写入**——错误（缺 name / 非法 name / 缺
//!   description）一律拒绝写入；写入前把旧文复制为 `SKILL.md.bak`
//!   （keep 1，随写随覆盖）；dry_run 只校验零写盘。
//! - `validate_meta`：静态检查，错误 + 告警两级（见 [`DocCheckReport`]）。
//!
//! 告警规则（不阻断写入）：
//! - description 少于 20 或超过 200 字符（过短触发不了 / 过长污染上下文）；
//! - 正文 ``` 围栏未闭合；
//! - 正文 markdown 链接引用的相对文件缺失。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{AppError, AppResult};
use skillbox_ports::{FileSystem, WatchMute};

use crate::deploy::write::MuteGuard;
use crate::fix::valid_skill_name;
use crate::install::validate_source_dir;
use crate::scan::{parse_frontmatter_head, SKILL_FILE};

/// 备份文件名（keep 1，随写随覆盖）。
const SKILL_BAK_FILE: &str = "SKILL.md.bak";

// ---------------------------------------------------------------------------
// 结果类型
// ---------------------------------------------------------------------------

/// 元数据校验结果：错误（阻断 doc_write）与告警（只提示）两级。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocCheckReport {
    pub errors: Vec<DocIssue>,
    pub warnings: Vec<DocIssue>,
}

impl DocCheckReport {
    /// 无错误（告警不影响合法性）。
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// 单条校验发现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocIssue {
    /// 规则 id：`missing_name` / `invalid_name` / `missing_description` /
    /// `short_description` / `long_description` / `unclosed_fence` /
    /// `missing_file` / `missing_skill_file`。
    pub rule: String,
    pub message: String,
}

/// doc_write 结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocWriteOutcome {
    pub dry_run: bool,
    /// 写入前留下的备份路径（旧文不存在则 None；dry_run 为"将备份"预览）。
    pub backup: Option<PathBuf>,
    /// 校验告警（不阻断；错误已在入口整体拒绝）。
    pub warnings: Vec<DocIssue>,
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 读正本全文。
pub fn doc_read(fs: &dyn FileSystem, skill_dir: &Path) -> AppResult<String> {
    let path = skill_dir.join(SKILL_FILE);
    if !fs.exists(&path) {
        return Err(AppError::not_found(format!("SKILL.md 不存在: {}", path.display())));
    }
    fs.read_to_string(&path)
}

/// 元数据校验（只读）。
pub fn validate_meta(fs: &dyn FileSystem, skill_dir: &Path) -> AppResult<DocCheckReport> {
    let text = doc_read(fs, skill_dir)?;
    Ok(check_text(fs, skill_dir, &text))
}

/// 写正本：先校验（错误拒绝写入）→ 备份旧文（keep 1）→ 写入。
/// dry_run 只校验，零写盘。
#[allow(clippy::too_many_arguments)]
pub fn doc_write(
    fs: &dyn FileSystem,
    watcher: Option<&Arc<dyn WatchMute>>,
    skill_dir: &Path,
    contents: &str,
    dry_run: bool,
) -> AppResult<DocWriteOutcome> {
    validate_source_dir(fs, skill_dir, "正本目录")?;
    if contents.trim().is_empty() {
        return Err(AppError::invalid_input("文档内容不能为空"));
    }
    let report = check_text(fs, skill_dir, contents);
    if !report.is_valid() {
        let mut message = String::from("文档校验未通过，已拒绝写入:");
        for issue in &report.errors {
            message.push_str(&format!("\n- [{}] {}", issue.rule, issue.message));
        }
        return Err(AppError::invalid_input(message));
    }
    let target = skill_dir.join(SKILL_FILE);
    let bak = skill_dir.join(SKILL_BAK_FILE);
    let had_old = fs.exists(&target);
    if dry_run {
        return Ok(DocWriteOutcome {
            dry_run: true,
            backup: had_old.then_some(bak),
            warnings: report.warnings,
        });
    }

    let _mute = MuteGuard::new(watcher);
    let backup = if had_old {
        fs.copy_file(&target, &bak)?;
        Some(bak)
    } else {
        None
    };
    fs.write(&target, contents.as_bytes())?;
    Ok(DocWriteOutcome { dry_run: false, backup, warnings: report.warnings })
}

// ---------------------------------------------------------------------------
// 校验规则
// ---------------------------------------------------------------------------

/// 对给定文本执行全部规则（`doc_write` 校验新内容 / `validate_meta` 校验盘上内容）。
pub(crate) fn check_text(fs: &dyn FileSystem, skill_dir: &Path, text: &str) -> DocCheckReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let (fm_name, description) = parse_frontmatter_head(text.as_bytes());
    match &fm_name {
        None => errors.push(DocIssue {
            rule: "missing_name".into(),
            message: "frontmatter 缺少 name 字段".into(),
        }),
        Some(name) => {
            if !valid_skill_name(name) {
                errors.push(DocIssue {
                    rule: "invalid_name".into(),
                    message: format!(
                        "name 不符合 ^[a-z0-9][a-z0-9-]{{1,63}}$: {name}"
                    ),
                });
            }
        }
    }
    match &description {
        None => errors.push(DocIssue {
            rule: "missing_description".into(),
            message: "frontmatter 缺少 description 字段".into(),
        }),
        Some(desc) => {
            let len = desc.chars().count();
            if len < 20 {
                warnings.push(DocIssue {
                    rule: "short_description".into(),
                    message: format!("description 仅 {len} 字符（建议 ≥ 20，便于 agent 触发）"),
                });
            } else if len > 200 {
                warnings.push(DocIssue {
                    rule: "long_description".into(),
                    message: format!("description 达 {len} 字符（建议 ≤ 200，避免污染上下文）"),
                });
            }
        }
    }

    let body = body_of(text);
    if body.matches("```").count() % 2 == 1 {
        warnings.push(DocIssue {
            rule: "unclosed_fence".into(),
            message: "正文存在未闭合的 ``` 围栏".into(),
        });
    }
    for target in markdown_targets(&body) {
        let resolved = skill_dir.join(&target);
        if !fs.exists(&resolved) {
            warnings.push(DocIssue {
                rule: "missing_file".into(),
                message: format!("正文引用的文件不存在: {target}"),
            });
        }
    }
    DocCheckReport { errors, warnings }
}

/// 取 frontmatter 之后的正文（无 frontmatter 时为全文）。
/// 逐行重排而非字节偏移：对 CRLF 行尾安全。
fn body_of(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return text.to_string();
    }
    let mut body: Vec<&str> = Vec::new();
    let mut in_body = false;
    for line in lines {
        if in_body {
            body.push(line);
        } else if line.trim_end() == "---" {
            in_body = true;
        }
    }
    body.join("\n")
}

/// 提取正文 markdown 链接目标 `](target)` 中需要检查存在性的相对路径。
/// 跳过：外链（`scheme://`）、锚点（`#`）、`mailto:`、空目标、含空格目标。
fn markdown_targets(body: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("](") {
        rest = &rest[start + 2..];
        let end = rest.find([')', ' ', '\n', '\t']).unwrap_or(rest.len());
        let target = &rest[..end];
        let candidate = target.trim();
        if candidate.is_empty()
            || candidate.starts_with('#')
            || candidate.starts_with("mailto:")
            || candidate.contains("://")
            || candidate.contains(char::is_whitespace)
        {
            continue;
        }
        targets.push(candidate.to_string());
    }
    targets
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::CountingFs;
    use skillbox_ports::ProdFileSystem;

    fn make_skill(root: &Path, name: &str, contents: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SKILL_FILE), contents).unwrap();
        dir
    }

    const VALID_DOC: &str = "---\nname: demo-skill\ndescription: a demo skill for testing docs\n---\n# Demo\n\nbody text\n";

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
        let report = validate_meta(&ProdFileSystem, &dir).unwrap();
        assert!(report.is_valid());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn validate_meta_collects_all_errors() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_skill(root.path(), "bad-skill", "no frontmatter at all");
        let report = validate_meta(&ProdFileSystem, &dir).unwrap();
        assert!(!report.is_valid());
        let rules: Vec<&str> = report.errors.iter().map(|e| e.rule.as_str()).collect();
        assert_eq!(rules, ["missing_name", "missing_description"]);
    }

    #[test]
    fn validate_meta_flags_invalid_name() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_skill(
            root.path(),
            "demo-skill",
            "---\nname: Bad Name\ndescription: a demo skill for testing docs\n---\n",
        );
        let report = validate_meta(&ProdFileSystem, &dir).unwrap();
        let rules: Vec<&str> = report.errors.iter().map(|e| e.rule.as_str()).collect();
        assert_eq!(rules, ["invalid_name"]);
    }

    #[test]
    fn validate_meta_warns_length_fence_and_missing_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_skill(
            root.path(),
            "demo-skill",
            "---\nname: demo-skill\ndescription: too short\n---\nsee [guide](guide.md) and [site](https://x.com) and [anchor](#top)\n```rust\nunclosed",
        );
        let report = validate_meta(&ProdFileSystem, &dir).unwrap();
        assert!(report.is_valid(), "告警不阻断");
        let rules: Vec<&str> = report.warnings.iter().map(|w| w.rule.as_str()).collect();
        assert!(rules.contains(&"short_description"));
        assert!(rules.contains(&"unclosed_fence"));
        assert!(rules.contains(&"missing_file"), "guide.md 不存在应告警: {rules:?}");
        // 外链与锚点不检查存在性。
        assert_eq!(report.warnings.iter().filter(|w| w.rule == "missing_file").count(), 1);
    }

    #[test]
    fn doc_write_rotates_single_backup() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_skill(root.path(), "demo-skill", VALID_DOC);

        let v2 = VALID_DOC.replace("body text", "body v2");
        doc_write(&ProdFileSystem, None, &dir, &v2, false).unwrap();
        let bak = dir.join(SKILL_BAK_FILE);
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), VALID_DOC, "首次写入备份旧文");

        let v3 = VALID_DOC.replace("body text", "body v3");
        doc_write(&ProdFileSystem, None, &dir, &v3, false).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), v2, "keep 1：只保留上一版");
        assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), v3);
    }

    #[test]
    fn doc_write_blocks_on_validation_errors() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_skill(root.path(), "demo-skill", VALID_DOC);

        let err = doc_write(&ProdFileSystem, None, &dir, "no frontmatter", false).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
        assert!(err.message.contains("missing_name"));
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
        let outcome = doc_write(counting.as_ref(), None, &dir, &v2, true).unwrap();
        assert!(outcome.dry_run);
        assert_eq!(outcome.backup, Some(dir.join(SKILL_BAK_FILE)), "dry_run 预告将备份");
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0, "dry_run 零写盘");
        assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), VALID_DOC);
    }

    #[test]
    fn doc_write_new_file_has_no_backup() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("brand-new");
        std::fs::create_dir_all(&dir).unwrap();
        let outcome = doc_write(&ProdFileSystem, None, &dir, VALID_DOC, false).unwrap();
        assert_eq!(outcome.backup, None);
        assert!(dir.join(SKILL_FILE).is_file());
    }

    #[test]
    fn doc_write_rejects_empty_and_missing_dir() {
        let root = tempfile::tempdir().unwrap();
        let dir = make_skill(root.path(), "demo-skill", VALID_DOC);
        let err = doc_write(&ProdFileSystem, None, &dir, "   \n", false).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);

        let missing = root.path().join("nope");
        let err = doc_write(&ProdFileSystem, None, &missing, VALID_DOC, false).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);
    }
}
