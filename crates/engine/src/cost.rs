//! 上下文成本（§9.2）：各已启用工具常驻 skill 的 description 成本
//! 与触发冲突分组。
//!
//! - token 按 ~3.5 字符/token 折算（[`CHARS_PER_TOKEN`]），是**估算值**，
//!   UI 必须标注。
//! - 冲突检测：description 分词（英文取 ≥2 字符的字母数字段，中文逐字）
//!   后两两 Jaccard ≥ [`CONFLICT_THRESHOLD`]（0.35）的组成风险组；
//!   组内连通（A~B、B~C 即同组），关键词取全员交集（为空时退化为
//!   出现于 ≥2 个成员的高频词），`similarity` 为组内最大两两相似度。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use skillbox_domain::{SkillId, ToolId};

use crate::types::{ConflictGroup, ConflictMember, CostReport, ToolCost};

/// token 折算系数（估算值）。
pub const CHARS_PER_TOKEN: f64 = 3.5;
/// description 两两 Jaccard ≥ 该阈值视为触发冲突风险组。
pub const CONFLICT_THRESHOLD: f64 = 0.35;
/// 冲突组关键词的展示上限。
const MAX_KEYWORDS: usize = 8;

/// 成本计算的 skill 输入行（ipc 由 DB 快照组装）。
pub struct CostSkill {
    pub id: SkillId,
    pub name: String,
    pub description: String,
}

/// 上下文成本报告。
///
/// - `skills`：中央库全部 skill（id → description）。
/// - `targets`：部署意图对（skill, tool）。
/// - `enabled_tools`：已启用工具（顺序即输出顺序）。
pub fn context_cost(
    skills: &[CostSkill],
    targets: &[(SkillId, ToolId)],
    enabled_tools: &[ToolId],
) -> CostReport {
    let by_id: HashMap<SkillId, &CostSkill> = skills.iter().map(|s| (s.id, s)).collect();
    let mut per_tool = Vec::with_capacity(enabled_tools.len());
    let mut conflicts = Vec::new();
    for tool in enabled_tools {
        // 该工具已部署的 skill（保持首次出现顺序去重）。
        let mut seen = HashSet::new();
        let mut ids: Vec<SkillId> = Vec::new();
        for (skill_id, tool_id) in targets {
            if tool_id == tool && seen.insert(*skill_id) {
                ids.push(*skill_id);
            }
        }
        let mut chars: u64 = 0;
        let mut tokens: Vec<(SkillId, BTreeSet<String>)> = Vec::with_capacity(ids.len());
        for id in &ids {
            if let Some(skill) = by_id.get(id) {
                chars += skill.description.chars().count() as u64;
                tokens.push((*id, tokenize(&skill.description)));
            }
        }
        per_tool.push(ToolCost {
            tool_id: tool.clone(),
            skill_count: ids.len() as u64,
            description_chars: chars,
            estimated_tokens: (chars as f64 / CHARS_PER_TOKEN).round() as u64,
        });
        conflicts.extend(conflicts_for_tool(tool, &by_id, &tokens));
    }
    CostReport { per_tool, conflicts }
}

/// 单个工具内的冲突组：两两 Jaccard 建边后取连通分量（≥2 成员）。
fn conflicts_for_tool(
    tool: &ToolId,
    by_id: &HashMap<SkillId, &CostSkill>,
    tokens: &[(SkillId, BTreeSet<String>)],
) -> Vec<ConflictGroup> {
    let n = tokens.len();
    if n < 2 {
        return Vec::new();
    }
    // 邻接表（两两 Jaccard ≥ 阈值）。
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut max_jac: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let sim = jaccard(&tokens[i].1, &tokens[j].1);
            if sim >= CONFLICT_THRESHOLD {
                adj[i].push(j);
                adj[j].push(i);
                max_jac.insert((i, j), sim);
            }
        }
    }
    // 连通分量（访问序即成员序，确定性）。
    let mut visited = vec![false; n];
    let mut groups = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        let mut component: Vec<usize> = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start);
        visited[start] = true;
        while let Some(cur) = queue.pop_front() {
            component.push(cur);
            for &next in &adj[cur] {
                if !visited[next] {
                    visited[next] = true;
                    queue.push_back(next);
                }
            }
        }
        if component.len() < 2 {
            continue;
        }
        component.sort_unstable();
        let members: Vec<ConflictMember> = component
            .iter()
            .map(|&i| ConflictMember {
                skill_id: tokens[i].0,
                name: by_id.get(&tokens[i].0).map(|s| s.name.clone()).unwrap_or_default(),
                tool_id: tool.clone(),
            })
            .collect();
        let similarity = component
            .iter()
            .enumerate()
            .flat_map(|(a, &i)| component.iter().skip(a + 1).map(move |&j| (i, j)))
            .map(|(i, j)| {
                let key = (i.min(j), i.max(j));
                max_jac.get(&key).copied().unwrap_or(0.0)
            })
            .fold(0.0f64, f64::max);
        groups.push(ConflictGroup { members, keywords: keywords_of(&component, tokens), similarity });
    }
    groups
}

/// 组内重叠关键词：全员 token 交集；为空时退化为出现于 ≥2 个成员的
/// 高频词（按出现次数降序、字典序稳定排序），截断到 [`MAX_KEYWORDS`]。
fn keywords_of(component: &[usize], tokens: &[(SkillId, BTreeSet<String>)]) -> Vec<String> {
    let mut sets = component.iter().map(|&i| &tokens[i].1);
    let Some(first) = sets.next() else { return Vec::new() };
    let mut inter: BTreeSet<String> = first.clone();
    for set in sets {
        inter = inter.intersection(set).cloned().collect();
    }
    if !inter.is_empty() {
        return inter.into_iter().take(MAX_KEYWORDS).collect();
    }
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for &i in component {
        for token in &tokens[i].1 {
            *freq.entry(token.clone()).or_insert(0) += 1;
        }
    }
    let mut shared: Vec<(String, usize)> = freq.into_iter().filter(|(_, c)| *c >= 2).collect();
    shared.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    shared.into_iter().take(MAX_KEYWORDS).map(|(t, _)| t).collect()
}

/// 分词：小写 ASCII 字母数字连续段（≥2 字符）+ 中文逐字。
fn tokenize(text: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut BTreeSet<String>| {
        if cur.chars().count() >= 2 {
            out.insert(cur.clone());
        }
        cur.clear();
    };
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch.to_ascii_lowercase());
        } else if ('\u{4E00}'..='\u{9FFF}').contains(&ch) {
            flush(&mut cur, &mut out);
            out.insert(ch.to_string());
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Jaccard 相似度；双空集定义为 0（无内容不构成冲突）。
fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(id: i64, name: &str, description: &str) -> CostSkill {
        CostSkill { id: SkillId(id), name: name.to_string(), description: description.to_string() }
    }

    #[test]
    fn token_estimate_uses_chars_per_token() {
        // 340 字符 / 3.5 = 97.14 → 97（四舍五入，估算值）。
        let a = skill(1, "a", &"x".repeat(100));
        let b = skill(2, "b", &"y".repeat(240));
        let tool = ToolId("claude_code".into());
        let report = context_cost(&[a, b], &[(SkillId(1), tool.clone()), (SkillId(2), tool.clone())], std::slice::from_ref(&tool));
        assert_eq!(report.per_tool.len(), 1);
        let cost = &report.per_tool[0];
        assert_eq!(cost.tool_id, tool);
        assert_eq!(cost.skill_count, 2);
        assert_eq!(cost.description_chars, 340);
        assert_eq!(cost.estimated_tokens, 97);
    }

    #[test]
    fn only_enabled_tools_and_deployed_skills_counted() {
        let a = skill(1, "a", "deploy coding helper for python tests");
        let tool_a = ToolId("tool_a".into());
        let tool_b = ToolId("tool_b".into());
        let report = context_cost(
            &[a],
            &[(SkillId(1), tool_b.clone())],
            std::slice::from_ref(&tool_a), // tool_b 未启用
        );
        assert_eq!(report.per_tool.len(), 1);
        assert_eq!(report.per_tool[0].tool_id, tool_a);
        assert_eq!(report.per_tool[0].skill_count, 0, "未部署到启用工具的不计");
        assert!(report.conflicts.is_empty());
    }

    #[test]
    fn duplicate_targets_counted_once() {
        let a = skill(1, "a", "helper");
        let tool = ToolId("t".into());
        let report = context_cost(&[a], &[(SkillId(1), tool.clone()), (SkillId(1), tool.clone())], &[tool]);
        assert_eq!(report.per_tool[0].skill_count, 1);
    }

    #[test]
    fn conflicting_descriptions_grouped_with_keywords() {
        // 三个 description 高度重叠 → 单组；关键词为全员交集。
        let a = skill(1, "code-review", "Review python code, run unit tests and fix bugs");
        let b = skill(2, "test-runner", "Run unit tests for python code and review the diff");
        let c = skill(3, "py-helper", "Python helper to run unit tests and review code quality");
        let tool = ToolId("t".into());
        let report = context_cost(
            &[a, b, c],
            &[
                (SkillId(1), tool.clone()),
                (SkillId(2), tool.clone()),
                (SkillId(3), tool.clone()),
            ],
            &[tool],
        );
        assert_eq!(report.conflicts.len(), 1);
        let group = &report.conflicts[0];
        assert_eq!(group.members.len(), 3);
        assert!(group.similarity >= CONFLICT_THRESHOLD);
        assert!(group.keywords.contains(&"tests".to_string()));
        assert!(group.keywords.contains(&"run".to_string()));
        assert!(group.members.iter().all(|m| m.tool_id.0 == "t"));
    }

    #[test]
    fn disjoint_descriptions_do_not_conflict() {
        let a = skill(1, "chef", "Cook italian pasta recipes with tomato sauce");
        let b = skill(2, "pilot", "Fly airplane checklist aviation weather procedures");
        let tool = ToolId("t".into());
        let report = context_cost(
            &[a, b],
            &[(SkillId(1), tool.clone()), (SkillId(2), tool.clone())],
            &[tool],
        );
        assert!(report.conflicts.is_empty());
    }

    #[test]
    fn transitive_pairs_form_single_group() {
        // A~B、B~C 相似；A~C 不相似 → 仍是一组（连通）。
        let a = skill(1, "a", "alpha beta gamma delta epsilon zeta");
        let b = skill(2, "b", "alpha beta gamma delta epsilon eta");
        let c = skill(3, "c", "beta gamma delta epsilon eta theta");
        let tool = ToolId("t".into());
        let report = context_cost(
            &[a, b, c],
            &[
                (SkillId(1), tool.clone()),
                (SkillId(2), tool.clone()),
                (SkillId(3), tool.clone()),
            ],
            &[tool],
        );
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].members.len(), 3);
    }

    #[test]
    fn cjk_chars_are_single_char_tokens() {
        // 中文逐字分词：5/9 ≈ 0.56 ≥ 0.35。
        let a = skill(1, "a", "数学建模求解器");
        let b = skill(2, "b", "数学建模推导器");
        let tool = ToolId("t".into());
        let report = context_cost(
            &[a, b],
            &[(SkillId(1), tool.clone()), (SkillId(2), tool.clone())],
            &[tool],
        );
        assert_eq!(report.conflicts.len(), 1);
        assert!(report.conflicts[0].keywords.contains(&"数".to_string()));
        assert!(report.conflicts[0].keywords.contains(&"学".to_string()));
    }

    #[test]
    fn conflicts_are_per_tool() {
        // 同一对 description 部署到两个工具 → 各自成组，成员 tool_id 不同。
        let a = skill(1, "a", "run python unit tests and fix code");
        let b = skill(2, "b", "run unit tests for python and fix bugs");
        let t1 = ToolId("t1".into());
        let t2 = ToolId("t2".into());
        let report = context_cost(
            &[a, b],
            &[
                (SkillId(1), t1.clone()),
                (SkillId(2), t1.clone()),
                (SkillId(1), t2.clone()),
                (SkillId(2), t2.clone()),
            ],
            &[t1, t2],
        );
        assert_eq!(report.conflicts.len(), 2);
        assert!(report.conflicts[0].members.iter().all(|m| m.tool_id.0 == "t1"));
        assert!(report.conflicts[1].members.iter().all(|m| m.tool_id.0 == "t2"));
    }
}
