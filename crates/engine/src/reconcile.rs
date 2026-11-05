//! reconcile —— 磁盘真相对账（TASKBOOK §6.1/§6.2，K4）。
//!
//! 【契约】纯计算：输入磁盘快照与 DB 索引快照，输出差异清单；
//! 不写盘、不改 DB、不读任何端口。任何 UI 的"同步异常"必须由本函数
//! （或同源磁盘推算）得出，禁止直接读 DB 字段展示（§6.1 硬约束）。
//!
//! DB 索引只作为"上次意图"参与比对；分类判定一律以磁盘条目现状为准。
//! 覆盖 §6.2 全部 7 类差异，判定规则：
//!
//! | 差异 | 判定 | severity | 建议修复 |
//! |---|---|---|---|
//! | `UnmanagedRealDir` | 真实目录且无记录，或记录为链接模式 | Info | Adopt（收编注册） |
//! | `BrokenLink` | 悬空链接（目标不存在） | Warning | 匹配到正本→RebuildLink，否则 RemoveLink |
//! | `ForeignLink` | 链接指向中央库之外且未悬空 | Info | RemoveLink（仅提示） |
//! | `MissingTarget` | 记录已部署但磁盘没有（含现状为真实文件） | Warning | Redeploy |
//! | `UnrecordedDeployment` | 指向中央库的链接但无记录 | Info | RegisterRecord |
//! | `CopyDrift` | copy 副本 content_hash ≠ 记录 source_hash | Warning | UpdateCopy |
//! | `DuplicateName` | 多个 agent 目录同名且内容身份 ≥ 2（同一 skill 的部署视为同一身份） | Info | ChooseCanonical |
//!
//! 输出确定性：同一（磁盘快照, 索引快照）输入恒产生同一清单
//! （按 `(path, kind)` 排序，`related` 亦排序）。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use skillbox_domain::{SkillId, SyncMode, TargetState, ToolId};

use crate::types::{
    Discrepancy, DiscrepancyKind, DiskEntry, DiskView, IndexView, Severity, SuggestedFix,
    TargetRecord,
};

/// 以磁盘为真相，将磁盘现状与 DB 索引比对，产出差异清单（§6.2）。
///
/// 纯计算，不写盘、不修改 DB。`index` 应只含本次实际检查过的工具的
/// 记录（doctor 侧负责按同一工具集过滤，避免未检查工具被误报
/// MissingTarget）。
pub fn reconcile(disk: &DiskView, index: &IndexView) -> Vec<Discrepancy> {
    let records = index_by_target(&index.records);
    let mut seen: HashSet<(ToolId, PathBuf)> = HashSet::new();
    let mut out = Vec::new();

    for entry in &disk.entries {
        let record = records.get(&(entry.tool_id.clone(), entry.path.clone())).copied();
        seen.insert((entry.tool_id.clone(), entry.path.clone()));
        match entry.state {
            // Absent 条目按"磁盘上没有"处理：有记录 → MissingTarget。
            TargetState::Absent => {
                if let Some(record) = record {
                    out.push(missing_target(record));
                }
            }
            TargetState::RealDir => match record {
                // 记录为 copy 模式：仅当两侧哈希齐备且不一致时报漂移。
                Some(r) if r.mode == SyncMode::Copy => {
                    let drift = match (&r.source_hash, &entry.content_hash) {
                        (Some(source_hash), Some(disk_hash)) => source_hash != disk_hash,
                        _ => false,
                    };
                    if drift {
                        out.push(copy_drift(entry, r));
                    }
                }
                // 无记录，或记录为链接模式而磁盘是真实目录。
                _ => out.push(unmanaged_real_dir(entry)),
            },
            TargetState::ForeignLink => {
                if entry.link_broken {
                    out.push(broken_link(entry, index));
                } else {
                    out.push(foreign_link(entry));
                }
            }
            TargetState::LinkToSource => {
                if record.is_none() {
                    out.push(unrecorded_deployment(entry));
                }
            }
            TargetState::RealFile => {
                if let Some(record) = record {
                    out.push(missing_target(record));
                }
            }
        }
    }

    // 记录已存在但磁盘上未见该路径 → MissingTarget。
    for record in &index.records {
        if seen.contains(&(record.tool_id.clone(), record.path.clone())) {
            continue;
        }
        out.push(missing_target(record));
    }

    // 多个 agent 目录同名但内容身份不同（§6.2 DuplicateName）。
    out.extend(duplicate_names(&disk.entries, index, &records));

    out.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.kind.as_str().cmp(b.kind.as_str())));
    out
}

/// 按 `(tool_id, path)` 建记录索引（同键多条时后者覆盖，v1 容忍）。
fn index_by_target(records: &[TargetRecord]) -> HashMap<(ToolId, PathBuf), &TargetRecord> {
    records.iter().map(|r| ((r.tool_id.clone(), r.path.clone()), r)).collect()
}

fn unmanaged_real_dir(entry: &DiskEntry) -> Discrepancy {
    Discrepancy {
        kind: DiscrepancyKind::UnmanagedRealDir,
        // 【修复】未纳管技能是"可收编的机会"而非风险：降为 Info 提示，
        // 一键 Adopt（搬入中央库 + 原位链接）完成注册。
        severity: Severity::Info,
        skill_id: None,
        tool_id: Some(entry.tool_id.clone()),
        path: entry.path.clone(),
        related: Vec::new(),
        detail: "发现未纳管技能（真实目录，无部署记录），可收编进中央库".to_string(),
        suggested_fix: SuggestedFix::Adopt,
    }
}

fn broken_link(entry: &DiskEntry, index: &IndexView) -> Discrepancy {
    // 悬空链接不含数据。可重建匹配（从严到宽，命中即止）：
    // 1) 原始链接目标与索引正本路径一致；
    // 2) 链接目标与正本同目录（目标曾指过的旧正本路径已迁移/更名）且
    //    条目名与 skill 名一致——中央库名字唯一，至多命中一个。
    // 3) 均未命中 → 只建议移除（不猜测意图）。
    let matched = entry
        .link_target
        .as_ref()
        .and_then(|t| index.skills.iter().find(|s| &s.canonical_path == t))
        .or_else(|| {
            let name = entry.path.file_name()?.to_string_lossy().into_owned();
            let parent = entry.link_target.as_ref()?.parent()?;
            index
                .skills
                .iter()
                .find(|s| s.name == name && s.canonical_path.parent() == Some(parent))
        });
    let related = entry.link_target.clone().into_iter().collect();
    match matched {
        Some(skill) => Discrepancy {
            kind: DiscrepancyKind::BrokenLink,
            severity: Severity::Warning,
            skill_id: Some(skill.id),
            tool_id: Some(entry.tool_id.clone()),
            path: entry.path.clone(),
            related,
            detail: format!(
                "链接悬空（目标不存在），可从中央库正本重建: {}",
                skill.canonical_path.display()
            ),
            suggested_fix: SuggestedFix::RebuildLink,
        },
        None => Discrepancy {
            kind: DiscrepancyKind::BrokenLink,
            severity: Severity::Warning,
            skill_id: None,
            tool_id: Some(entry.tool_id.clone()),
            path: entry.path.clone(),
            related,
            detail: "链接悬空（目标不存在），且无法对应到中央库正本".to_string(),
            suggested_fix: SuggestedFix::RemoveLink,
        },
    }
}

fn foreign_link(entry: &DiskEntry) -> Discrepancy {
    Discrepancy {
        kind: DiscrepancyKind::ForeignLink,
        severity: Severity::Info,
        skill_id: None,
        tool_id: Some(entry.tool_id.clone()),
        path: entry.path.clone(),
        related: entry.link_target.clone().into_iter().collect(),
        detail: "链接指向中央库之外（用户自有配置，仅提示，不自动改动）".to_string(),
        suggested_fix: SuggestedFix::RemoveLink,
    }
}

fn missing_target(record: &TargetRecord) -> Discrepancy {
    Discrepancy {
        kind: DiscrepancyKind::MissingTarget,
        severity: Severity::Warning,
        skill_id: Some(record.skill_id),
        tool_id: Some(record.tool_id.clone()),
        path: record.path.clone(),
        related: Vec::new(),
        detail: "DB 记录已部署，但磁盘上没有该部署".to_string(),
        suggested_fix: SuggestedFix::Redeploy,
    }
}

fn unrecorded_deployment(entry: &DiskEntry) -> Discrepancy {
    Discrepancy {
        kind: DiscrepancyKind::UnrecordedDeployment,
        severity: Severity::Info,
        skill_id: entry.source_skill_id,
        tool_id: Some(entry.tool_id.clone()),
        path: entry.path.clone(),
        related: Vec::new(),
        detail: "磁盘上有指向中央库的链接，DB 无部署记录".to_string(),
        suggested_fix: SuggestedFix::RegisterRecord,
    }
}

fn copy_drift(entry: &DiskEntry, record: &TargetRecord) -> Discrepancy {
    Discrepancy {
        kind: DiscrepancyKind::CopyDrift,
        severity: Severity::Warning,
        skill_id: Some(record.skill_id),
        tool_id: Some(entry.tool_id.clone()),
        path: entry.path.clone(),
        related: Vec::new(),
        detail: "copy 模式副本内容哈希与部署时记录的 source_hash 不一致（正本已更新或副本被改动）"
            .to_string(),
        suggested_fix: SuggestedFix::UpdateCopy,
    }
}

/// 同名分组：≥ 2 个条目且内容身份 ≥ 2 种 → DuplicateName。
/// `path` 取字典序最小者，`related` 为其余（排序），供 UI 高亮选择正本。
fn duplicate_names(
    entries: &[DiskEntry],
    index: &IndexView,
    records: &HashMap<(ToolId, PathBuf), &TargetRecord>,
) -> Vec<Discrepancy> {
    let known_skills: HashSet<SkillId> = index.skills.iter().map(|s| s.id).collect();
    let mut groups: HashMap<String, Vec<&DiskEntry>> = HashMap::new();
    for entry in entries {
        let Some(name) = entry.path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        groups.entry(name).or_default().push(entry);
    }
    let mut out = Vec::new();
    for (name, mut group) in groups {
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|a, b| a.path.cmp(&b.path));
        let mut identities: HashSet<String> = HashSet::new();
        for entry in &group {
            if let Some(identity) = content_identity(entry, records, &known_skills) {
                identities.insert(identity);
            }
        }
        if identities.len() < 2 {
            continue;
        }
        let related = group[1..].iter().map(|e| e.path.clone()).collect::<Vec<_>>();
        out.push(Discrepancy {
            kind: DiscrepancyKind::DuplicateName,
            severity: Severity::Info,
            skill_id: None,
            tool_id: None,
            path: group[0].path.clone(),
            related,
            detail: format!("多个 agent 目录存在同名「{name}」但内容不同，需人工挑选正本（不会自动选择）"),
            suggested_fix: SuggestedFix::ChooseCanonical,
        });
    }
    out
}

/// 条目的内容身份：同一 skill 的部署（链接或 copy 副本）视为同一身份——
/// 副本漂移由 `CopyDrift` 报告，不构成"重复名"。索引未知的条目回退到
/// 内容比对：真实目录 → 递归内容哈希；链接 → 原始链接目标。
/// None = 不参与同名比对（真实文件、缺哈希目录等）。
fn content_identity(
    entry: &DiskEntry,
    records: &HashMap<(ToolId, PathBuf), &TargetRecord>,
    known_skills: &HashSet<SkillId>,
) -> Option<String> {
    match entry.state {
        TargetState::LinkToSource => Some(match entry.source_skill_id {
            Some(id) => format!("skill:{}", id.0),
            None => entry.link_target.as_ref()?.display().to_string(),
        }),
        TargetState::RealDir => {
            if let Some(r) = records.get(&(entry.tool_id.clone(), entry.path.clone())) {
                if r.mode == SyncMode::Copy && known_skills.contains(&r.skill_id) {
                    return Some(format!("skill:{}", r.skill_id.0));
                }
            }
            entry.content_hash.clone()
        }
        TargetState::ForeignLink => {
            entry.link_target.as_ref().map(|t| t.display().to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
