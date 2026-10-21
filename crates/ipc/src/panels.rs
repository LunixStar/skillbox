//! security_scan / security_report / cost_report（§8；Phase 4 实装，先占位）。
//!
//! 契约（§9.1 / §9.2）：纯静态规则匹配，不执行任何 skill 脚本；
//! token 折算 ~3.5 字符/token。占位期返回结构化"尚未实现"错误。

use skillbox_domain::{AppError, AppResult, SkillId};
use skillbox_engine::{CostReport, SecurityReport};

#[tauri::command]
pub fn security_scan(skill_id: SkillId) -> AppResult<SecurityReport> {
    Err(AppError::internal(format!(
        "尚未实现：安全扫描（Phase 4，§9.1），请求 skill #{skill_id}"
    )))
}

#[tauri::command]
pub fn security_report() -> AppResult<SecurityReport> {
    Err(AppError::internal("尚未实现：安全报告（Phase 4，§9.1）"))
}

#[tauri::command]
pub fn cost_report() -> AppResult<CostReport> {
    Err(AppError::internal("尚未实现：上下文成本报告（Phase 4，§9.2）"))
}
