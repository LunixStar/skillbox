//! security_scan / security_report / cost_report（§8；§9.1 / §9.2 实装）。
//!
//! 契约（§9.1 / §9.2）：
//! - 安全扫描是纯静态规则匹配，不执行任何 skill 脚本（engine::audit）；
//! - 扫描结果缓存在 AppState（skill → 报告），`security_report` 汇总；
//! - token 折算 ~3.5 字符/token，前端必须标注"估算值"。

use skillbox_domain::{AppError, AppResult, SkillId, ToolId};
use skillbox_engine::{
    audit_skill, context_cost, AuditContext, CostReport, CostSkill, SecurityReport,
};
use tauri::State;

use crate::state::AppState;
use crate::tools::env_snapshot;

/// 对单个 skill 做纯静态安全扫描（八条规则，只提示不阻断安装）。
#[tauri::command]
pub fn security_scan(state: State<'_, AppState>, skill_id: SkillId) -> AppResult<SecurityReport> {
    let meta = state
        .store
        .get_skill(skill_id)?
        .ok_or_else(|| AppError::not_found(format!("skill #{skill_id} 不存在")))?;
    let ctx = AuditContext { fs: state.fs.clone(), linker: state.linker.clone() };
    let report = audit_skill(&ctx, &meta.canonical_path)?;
    state.put_security_report(skill_id, report.clone());
    Ok(report)
}

/// 全库安全概览：合并已扫描 skill 的缓存报告。
#[tauri::command]
pub fn security_report(state: State<'_, AppState>) -> AppResult<SecurityReport> {
    Ok(state.security_overview())
}

/// 上下文成本报告：各已启用工具常驻 skill 的 description 成本
/// （~3.5 字符/token 估算）与触发冲突分组（Jaccard ≥ 0.35）。
#[tauri::command]
pub fn cost_report(state: State<'_, AppState>) -> AppResult<CostReport> {
    let skills = state.store.list_skills()?;
    let targets = state.store.list_targets()?;
    let disabled: Vec<String> = state.disabled_tools()?.into_iter().map(|t| t.0).collect();
    let enabled: Vec<ToolId> = env_snapshot(&state)
        .into_iter()
        .map(|info| info.id)
        .filter(|id| !disabled.contains(&id.0))
        .collect();
    let cost_skills: Vec<CostSkill> = skills
        .into_iter()
        .map(|m| CostSkill { id: m.id, name: m.name, description: m.description })
        .collect();
    let pairs: Vec<(SkillId, ToolId)> = targets.into_iter().map(|t| (t.skill_id, t.tool_id)).collect();
    Ok(context_cost(&cost_skills, &pairs, &enabled))
}
