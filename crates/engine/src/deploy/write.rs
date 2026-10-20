//! 写入纪律原语（§5.3）：mute 守卫、源校验、tmp、trash、staging。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use skillbox_domain::{AppError, AppResult};
use skillbox_ports::{Clock, EntryKind, FileSystem, Linker, WatchMute};

use super::classify::{SourceMarker, SOURCE_MARKER_FILE};

/// 写盘期间的监听静音守卫：构造时 mute，Drop 时 unmute（§5.3）。
/// 提前 return / 出错也保证成对。
pub(crate) struct MuteGuard<'a> {
    watcher: Option<&'a Arc<dyn WatchMute>>,
}

impl<'a> MuteGuard<'a> {
    pub(crate) fn new(watcher: Option<&'a Arc<dyn WatchMute>>) -> Self {
        if let Some(w) = watcher {
            w.mute();
        }
        MuteGuard { watcher }
    }
}

impl Drop for MuteGuard<'_> {
    fn drop(&mut self) {
        if let Some(w) = self.watcher {
            w.unmute();
        }
    }
}

/// 校验中央库正本：必须存在且为真实目录（拒绝链接/文件）。
pub(crate) fn validate_source(fs: &dyn FileSystem, source: &Path) -> AppResult<()> {
    if !fs.exists(source) {
        return Err(AppError::not_found(format!("中央库正本不存在: {}", source.display())));
    }
    match fs.symlink_metadata(source)?.kind {
        EntryKind::Dir => Ok(()),
        EntryKind::Symlink => Err(AppError::invalid_input(format!(
            "中央库正本本身是链接，拒绝部署: {}",
            source.display()
        ))),
        _ => Err(AppError::invalid_input(format!(
            "中央库正本不是目录: {}",
            source.display()
        ))),
    }
}

/// 在目标的父目录下生成未占用的临时路径（同父目录 = 同卷，§5.3）。
pub(crate) fn fresh_tmp(fs: &dyn FileSystem, clock: &dyn Clock, target: &Path) -> AppResult<PathBuf> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let parent = target.parent().ok_or_else(|| {
        AppError::invalid_input(format!("目标路径没有父目录: {}", target.display()))
    })?;
    fs.create_dir_all(parent)?;
    loop {
        let tmp = parent.join(format!(
            ".skillbox-deploy-{}-{}",
            clock.now_ms(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        if !fs.exists(&tmp) {
            return Ok(tmp);
        }
    }
}

/// 把 `path` 整体 rename 进 `trash/<now_ms>/<basename>`（v1 无硬删除）。
/// 同卷 rename：跨卷直接报错，原路径保持原样（绝不删除式搬运）。
pub(crate) fn move_to_trash(
    fs: &dyn FileSystem,
    clock: &dyn Clock,
    trash_root: &Path,
    path: &Path,
) -> AppResult<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        AppError::invalid_input(format!("无法暂存没有文件名的路径: {}", path.display()))
    })?;
    let slot = trash_root.join(clock.now_ms().to_string());
    fs.create_dir_all(&slot)?;
    let mut dest = slot.join(name);
    let mut n: u32 = 0;
    while fs.exists(&dest) {
        n += 1;
        dest = slot.join(format!("{}-{n}", name.to_string_lossy()));
    }
    fs.rename(path, &dest).map_err(|e| {
        AppError::io(format!(
            "移入 trash 失败（可能跨卷；原路径未动）: {} -> {}: {e}",
            path.display(),
            dest.display()
        ))
    })?;
    Ok(dest)
}

/// 阶段一（链接）：在 `tmp` 上创建指向 `source` 的链接（回退链前两级）。
/// 两级都不可用时返回 Err，由调用方退化为 tracked copy。
pub(crate) fn stage_link(linker: &dyn Linker, source: &Path, tmp: &Path) -> AppResult<()> {
    if linker.supports_symlink() && linker.symlink_dir(source, tmp).is_ok() {
        return Ok(());
    }
    if linker.supports_junction() && linker.junction(source, tmp).is_ok() {
        return Ok(());
    }
    Err(AppError::io(format!(
        "无法为 {} 创建链接（symlink 与 junction 均不可用）",
        source.display()
    )))
}

/// 阶段一（副本）：递归复制 + 写来源标记（§5.2）。
pub(crate) fn stage_copy(
    fs: &dyn FileSystem,
    clock: &dyn Clock,
    source: &Path,
    tmp: &Path,
) -> AppResult<()> {
    fs.copy_dir_all(source, tmp)?;
    let marker = SourceMarker { source: source.to_path_buf(), written_at_ms: clock.now_ms() };
    let json = serde_json::to_vec_pretty(&marker)
        .map_err(|e| AppError::internal(format!("序列化 {SOURCE_MARKER_FILE} 失败: {e}")))?;
    fs.write(&tmp.join(SOURCE_MARKER_FILE), &json)
}

/// 清理残留的临时路径：链接 tmp 只删链接本身；目录 tmp 递归删
/// （tmp 属于我们，不涉及用户数据）。
pub(crate) fn cleanup_tmp(fs: &dyn FileSystem, linker: &dyn Linker, tmp: &Path) {
    let Ok(meta) = fs.symlink_metadata(tmp) else { return };
    match meta.kind {
        EntryKind::Symlink => {
            let _ = linker.remove_link(tmp);
        }
        _ => {
            let _ = fs.remove_dir_all(tmp);
        }
    }
}
