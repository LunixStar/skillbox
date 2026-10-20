//! tracked-copy 来源标记与目标分类（§5.1 / §5.2）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use skillbox_domain::{AppResult, TargetState};
use skillbox_ports::{EntryKind, FileSystem, Linker};

/// tracked-copy 部署的来源标记（写入目标内 `.skillbox-source.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceMarker {
    /// 中央库正本路径。
    pub(crate) source: PathBuf,
    /// 写入时间（epoch 毫秒）。
    pub(crate) written_at_ms: i64,
}

/// 来源标记文件名（§5.2）。
pub(crate) const SOURCE_MARKER_FILE: &str = ".skillbox-source.json";

/// 目标路径现状分类（lstat 语义）。
///
/// 【硬约束 §5.1】判定基于 `symlink_metadata()`，禁止 `is_dir()`。
pub(crate) fn classify_target(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    source: &Path,
    path: &Path,
) -> AppResult<TargetState> {
    match fs.symlink_metadata(path) {
        Ok(meta) => match meta.kind {
            EntryKind::Symlink => {
                if resolves_to_source(fs, linker, source, path) {
                    Ok(TargetState::LinkToSource)
                } else {
                    // 指向别处或悬空（canonicalize 失败）：用户配置。
                    Ok(TargetState::ForeignLink)
                }
            }
            EntryKind::Dir => Ok(TargetState::RealDir),
            EntryKind::File | EntryKind::Other => Ok(TargetState::RealFile),
        },
        Err(e) => {
            // AppError 未保留 io ErrorKind，用 exists 兜底区分 NotFound；
            // 悬空链接 lstat 成功，不会走到这里。
            if !fs.exists(path) {
                Ok(TargetState::Absent)
            } else {
                Err(e)
            }
        }
    }
}

/// 链接是否解析到 `source`（两边 canonicalize 后比较；任何一步失败即否）。
fn resolves_to_source(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    source: &Path,
    link: &Path,
) -> bool {
    let Ok(raw_target) = linker.read_link(link) else { return false };
    let Ok(source_canon) = fs.canonicalize(source) else { return false };
    let Ok(target_canon) = fs.canonicalize(&raw_target) else { return false };
    source_canon == target_canon
}

/// RealDir 是否为我们的 tracked-copy 副本：目标内存在
/// `.skillbox-source.json` 且记录的 source 与本次请求一致。
///
/// 判定只认磁盘标记（K4：DB 只是上次意图，本层也不读 DB）。
/// 无标记 / 解析失败 / 源不一致一律视为用户目录。
pub(crate) fn is_our_copy(fs: &dyn FileSystem, source: &Path, path: &Path) -> bool {
    let Ok(raw) = fs.read_to_string(&path.join(SOURCE_MARKER_FILE)) else { return false };
    let Ok(marker) = serde_json::from_str::<SourceMarker>(&raw) else { return false };
    marker.source == source
}
