//! 覆盖授权决策表（§5.2）。

use skillbox_domain::{ReplacePolicy, TargetState};

/// §5.1/§5.2 决策表：`(TargetState, ReplacePolicy)` 是否授权覆盖。
///
/// 逐格展开（不用 if 链），让每一格都能被单测与 review 钉死。
/// 【硬约束】`RealDir × Recorded { mode: "symlink" }` 必须为 false：
/// DB 记录只代表历史意图，不代表磁盘现状。
#[allow(clippy::match_same_arms)]
pub(crate) fn overwrite_authorized(state: TargetState, policy: &ReplacePolicy<'_>) -> bool {
    match (state, policy) {
        // Absent：目标不存在，任何策略下写入都不可能损失数据。
        (TargetState::Absent, ReplacePolicy::NoClobber) => true,
        (TargetState::Absent, ReplacePolicy::Recorded { .. }) => true,
        (TargetState::Absent, ReplacePolicy::UserConfirmed) => true,
        // LinkToSource：链接已指向本源，重写等价幂等。
        (TargetState::LinkToSource, ReplacePolicy::NoClobber) => true,
        (TargetState::LinkToSource, ReplacePolicy::Recorded { .. }) => true,
        (TargetState::LinkToSource, ReplacePolicy::UserConfirmed) => true,
        // ForeignLink：用户配置（指向别处/悬空），仅显式确认可覆盖。
        (TargetState::ForeignLink, ReplacePolicy::NoClobber) => false,
        (TargetState::ForeignLink, ReplacePolicy::Recorded { .. }) => false,
        (TargetState::ForeignLink, ReplacePolicy::UserConfirmed) => true,
        // RealDir：真实目录。Recorded 一律不授权——从中央库更新副本
        // 属 UserConfirmed 领域（§5.2 括注）。
        (TargetState::RealDir, ReplacePolicy::NoClobber) => false,
        (TargetState::RealDir, ReplacePolicy::Recorded { mode: "symlink" }) => false,
        (TargetState::RealDir, ReplacePolicy::Recorded { .. }) => false,
        (TargetState::RealDir, ReplacePolicy::UserConfirmed) => true,
        // RealFile：真实文件，仅显式确认可覆盖。
        (TargetState::RealFile, ReplacePolicy::NoClobber) => false,
        (TargetState::RealFile, ReplacePolicy::Recorded { .. }) => false,
        (TargetState::RealFile, ReplacePolicy::UserConfirmed) => true,
    }
}

/// 未授权状态的冲突原因（进入 `details.conflicts`，UI"指着目录说话"）。
pub(crate) fn conflict_reason(state: TargetState) -> String {
    match state {
        TargetState::ForeignLink => "链接指向别处或悬空（用户配置），覆盖需 UserConfirmed".to_string(),
        TargetState::RealDir => "真实目录（可能含用户数据），覆盖需 UserConfirmed".to_string(),
        TargetState::RealFile => "真实文件，覆盖需 UserConfirmed".to_string(),
        TargetState::Absent | TargetState::LinkToSource => "无冲突".to_string(),
    }
}
