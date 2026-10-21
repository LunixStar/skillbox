//! doc —— SKILL.md 文档读写与元数据校验（TASKBOOK §8 `doc` 资源组）。
//!
//! - `doc_read`：读正本全文（SKILL.md）。
//! - `doc_write`：**先校验后写入**——错误（缺 name / 非法 name / 缺
//!   description）一律拒绝写入；写入前把旧文复制为 `SKILL.md.bak`
//!   （keep 1，随写随覆盖）；dry_run 只校验零写盘。
//! - `validate_meta`：静态检查盘上内容；`validate_text` 检查编辑器中的
//!   任意文本（实时校验，零写盘）——错误 + 告警两级，均携带 1 起始行号
//!   （0 = 无确切行），供前端行内标记。
//! - `rescan_after_doc_save`：编辑保存后的**单 skill 增量重扫**——只刷新
//!   该 skill 的索引登记（description / content_hash），不做全量扫描。
//!
//! 告警规则（不阻断写入）：
//! - frontmatter name 与登记名不一致（索引与目录名不随编辑改名）；
//! - description 少于 20 或超过 200 字符（过短触发不了 / 过长污染上下文）；
//! - 正文 ``` 围栏未闭合（定位到未闭合围栏的起始行）；
//! - 正文 markdown 链接引用的相对文件缺失（定位到引用行）。

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{AppError, AppResult, SkillId};
use skillbox_ports::{
    FileSystem, Hasher, IndexStore, Linker, OriginInfo, WatchMute,
};

use crate::deploy::write::MuteGuard;
use crate::doctor::hash_dir;
use crate::fix::valid_skill_name;
use crate::install::{read_frontmatter, validate_source_dir};
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
    /// 规则 id：`missing_name` / `invalid_name` / `name_mismatch` /
    /// `missing_description` / `short_description` / `long_description` /
    /// `unclosed_fence` / `missing_file`。
    pub rule: String,
    pub message: String,
    /// 1 起始行号（0 = 无确切行）。
    pub line: u32,
}

/// doc_write 结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocWriteOutcome {
    pub dry_run: bool,
    /// 写入前留下的备份路径（旧文不存在则 None；dry_run 为"将备份"预览）。
    pub backup: Option<std::path::PathBuf>,
    /// 校验告警（不阻断；错误已在入口整体拒绝）。
    pub warnings: Vec<DocIssue>,
    /// 保存后单 skill 增量重扫是否已刷新索引（dry_run 恒 false）。
    pub index_refreshed: bool,
    /// 增量重扫失败说明（保存已成功，仅索引刷新失败）。
    pub index_error: Option<String>,
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

/// 元数据校验（只读，盘上内容）。
/// `expected_name`：登记名（name_mismatch 告警用），编辑器外场景可传 None。
pub fn validate_meta(
    fs: &dyn FileSystem,
    skill_dir: &Path,
    expected_name: Option<&str>,
) -> AppResult<DocCheckReport> {
    let text = doc_read(fs, skill_dir)?;
    Ok(validate_text(fs, skill_dir, &text, expected_name))
}

/// 校验编辑器中的任意文本（实时校验，零写盘）。
pub fn validate_text(
    fs: &dyn FileSystem,
    skill_dir: &Path,
    text: &str,
    expected_name: Option<&str>,
) -> DocCheckReport {
    check_text(fs, skill_dir, text, expected_name)
}

/// 写正本：先校验（错误拒绝写入）→ 备份旧文（keep 1）→ 写入。
/// dry_run 只校验，零写盘。
#[allow(clippy::too_many_arguments)]
pub fn doc_write(
    fs: &dyn FileSystem,
    watcher: Option<&Arc<dyn WatchMute>>,
    skill_dir: &Path,
    contents: &str,
    expected_name: Option<&str>,
    dry_run: bool,
) -> AppResult<DocWriteOutcome> {
    validate_source_dir(fs, skill_dir, "正本目录")?;
    if contents.trim().is_empty() {
        return Err(AppError::invalid_input("文档内容不能为空"));
    }
    let report = check_text(fs, skill_dir, contents, expected_name);
    if !report.is_valid() {
        let mut message = String::from("文档校验未通过，已拒绝写入:");
        for issue in &report.errors {
            if issue.line > 0 {
                message.push_str(&format!("\n- [{}] {}（第 {} 行）", issue.rule, issue.message, issue.line));
            } else {
                message.push_str(&format!("\n- [{}] {}", issue.rule, issue.message));
            }
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
            index_refreshed: false,
            index_error: None,
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
    Ok(DocWriteOutcome {
        dry_run: false,
        backup,
        warnings: report.warnings,
        index_refreshed: false,
        index_error: None,
    })
}

/// 编辑保存后的**增量重扫**：只重算该 skill 的 frontmatter description 与
/// 内容哈希并刷新索引行（§9.3「编辑保存后触发一次增量重扫，不要全量」）。
/// name / canonical_path / origin 均不动（身份与位置不变）。
pub fn rescan_after_doc_save(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    hasher: &dyn Hasher,
    index: &dyn IndexStore,
    skill_id: SkillId,
    skill_dir: &Path,
    origin: &OriginInfo,
) -> AppResult<()> {
    let (_, description) = read_frontmatter(fs, skill_dir);
    let content_hash = hash_dir(fs, linker, hasher, skill_dir);
    index.refresh_skill(skill_id, &description, origin, content_hash.as_deref())
}

// ---------------------------------------------------------------------------
// 校验规则
// ---------------------------------------------------------------------------

/// 对给定文本执行全部规则（`doc_write` 校验新内容 / `validate_text` 实时校验）。
pub(crate) fn check_text(
    fs: &dyn FileSystem,
    skill_dir: &Path,
    text: &str,
    expected_name: Option<&str>,
) -> DocCheckReport {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.lines().collect();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let fm_end = frontmatter_end(&lines);
    let name_line = key_line(&lines, fm_end, "name");
    let desc_line = key_line(&lines, fm_end, "description");
    let (fm_name, description) = parse_frontmatter_head(text.as_bytes());

    match &fm_name {
        None => errors.push(DocIssue {
            rule: "missing_name".into(),
            message: "frontmatter 缺少 name 字段".into(),
            line: frontmatter_anchor(fm_end),
        }),
        Some(name) => {
            let line = name_line.map_or_else(|| frontmatter_anchor(fm_end), |i| i as u32 + 1);
            if !valid_skill_name(name) {
                errors.push(DocIssue {
                    rule: "invalid_name".into(),
                    message: format!("name 不符合 ^[a-z0-9][a-z0-9-]{{1,63}}$: {name}"),
                    line,
                });
            }
            if let Some(expected) = expected_name {
                if name != expected {
                    warnings.push(DocIssue {
                        rule: "name_mismatch".into(),
                        message: format!(
                            "frontmatter name（{name}）与登记名（{expected}）不一致；索引与目录名不会随编辑改名"
                        ),
                        line,
                    });
                }
            }
        }
    }

    match &description {
        None => errors.push(DocIssue {
            rule: "missing_description".into(),
            message: "frontmatter 缺少 description 字段".into(),
            line: frontmatter_anchor(fm_end),
        }),
        Some(desc) => {
            let len = desc.chars().count();
            let line = desc_line.map_or_else(|| frontmatter_anchor(fm_end), |i| i as u32 + 1);
            if len < 20 {
                warnings.push(DocIssue {
                    rule: "short_description".into(),
                    message: format!("description 仅 {len} 字符（建议 ≥ 20，便于 agent 触发）"),
                    line,
                });
            } else if len > 200 {
                warnings.push(DocIssue {
                    rule: "long_description".into(),
                    message: format!("description 达 {len} 字符（建议 ≤ 200，避免污染上下文）"),
                    line,
                });
            }
        }
    }

    // 正文 = frontmatter 之后（无 frontmatter 时全文）。
    let body_start = fm_end.map_or(0, |i| i + 1);

    // 未闭合围栏：行级开合配对（行首 ``` 切换开/合），定位未闭合的起始行。
    let mut fence_open_line: Option<u32> = None;
    for (i, line) in lines.iter().enumerate().skip(body_start) {
        if line.trim_start().starts_with("```") {
            if fence_open_line.is_some() {
                fence_open_line = None;
            } else {
                fence_open_line = Some(i as u32 + 1);
            }
        }
    }
    if let Some(open) = fence_open_line {
        warnings.push(DocIssue {
            rule: "unclosed_fence".into(),
            message: "正文存在未闭合的 ``` 围栏".into(),
            line: open,
        });
    }

    // 引用文件缺失：逐行提取 markdown 链接目标，定位到引用行（同一目标只报首次）。
    let mut checked: HashSet<String> = HashSet::new();
    for (i, line) in lines.iter().enumerate().skip(body_start) {
        for target in markdown_targets(line) {
            if !checked.insert(target.clone()) {
                continue;
            }
            if !fs.exists(&skill_dir.join(&target)) {
                warnings.push(DocIssue {
                    rule: "missing_file".into(),
                    message: format!("正文引用的文件不存在: {target}"),
                    line: i as u32 + 1,
                });
            }
        }
    }
    DocCheckReport { errors, warnings }
}

/// frontmatter 闭合 `---` 的行索引（0 起始）；无 frontmatter 返回 None。
/// 首行必须是 `---`（与 [`parse_frontmatter_head`] 同规则）。
fn frontmatter_end(lines: &[&str]) -> Option<usize> {
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return None;
    }
    lines
        .iter()
        .skip(1)
        .position(|l| l.trim_end() == "---")
        .map(|i| i + 1)
}

/// frontmatter 内 `key:` 所在行索引（0 起始）；无 frontmatter / 未找到返回 None。
fn key_line(lines: &[&str], fm_end: Option<usize>, key: &str) -> Option<usize> {
    let end = fm_end?;
    lines
        .iter()
        .take(end)
        .skip(1)
        .position(|l| l.split_once(':').is_some_and(|(k, _)| k.trim() == key))
        .map(|i| i + 1)
}

/// frontmatter 存在时的提示锚点行（`---` 之后第 1 行，1 起始）；否则第 1 行。
fn frontmatter_anchor(fm_end: Option<usize>) -> u32 {
    if fm_end.is_some() {
        2
    } else {
        1
    }
}

/// 提取一行内 markdown 链接目标 `](target)` 中需要检查存在性的相对路径。
/// 跳过：外链（`scheme://`）、锚点（`#`）、`mailto:`、空目标、含空格目标。
fn markdown_targets(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("](") {
        rest = &rest[start + 2..];
        let end = rest.find([')', ' ', '\t']).unwrap_or(rest.len());
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

#[cfg(test)]
mod tests;
