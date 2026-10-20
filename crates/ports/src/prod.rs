//! 生产实现：真实文件系统 / 系统时钟 / SHA-256 / 平台链接 / git 子进程。
//!
//! engine 在运行时注入这些实现；测试注入假实现。所有实现不持有可变状态，
//! 均为零开销的单元结构体。

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use skillbox_domain::{AppError, AppResult};

use super::{Clock, EntryKind, EntryMeta, FileHash, FileSystem, GitClient, Hasher, LinkKind, Linker};

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn modified_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn to_entry_meta(name: String, meta: &fs::Metadata) -> EntryMeta {
    let ft = meta.file_type();
    let kind = if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    };
    EntryMeta { name, kind, len: meta.len(), modified_ms: modified_ms(meta) }
}

fn basename_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// FileSystem
// ---------------------------------------------------------------------------

/// 真实文件系统实现（std::fs 直通）。
#[derive(Debug, Clone, Copy, Default)]
pub struct ProdFileSystem;

impl FileSystem for ProdFileSystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn symlink_metadata(&self, path: &Path) -> AppResult<EntryMeta> {
        let meta = fs::symlink_metadata(path)?;
        Ok(to_entry_meta(basename_of(path), &meta))
    }

    fn read_dir(&self, path: &Path) -> AppResult<Vec<EntryMeta>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            // DirEntry::metadata() 为 lstat 语义：不跟随符号链接（§7 约束 4）。
            let meta = entry.metadata()?;
            out.push(to_entry_meta(entry.file_name().to_string_lossy().into_owned(), &meta));
        }
        Ok(out)
    }

    fn read_head(&self, path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
        let mut f = fs::File::open(path)?;
        let mut buf = Vec::new();
        f.by_ref().take(max_bytes).read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn read_to_string(&self, path: &Path) -> AppResult<String> {
        Ok(fs::read_to_string(path)?)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> AppResult<()> {
        Ok(fs::write(path, contents)?)
    }

    fn create_dir_all(&self, path: &Path) -> AppResult<()> {
        Ok(fs::create_dir_all(path)?)
    }

    fn rename(&self, from: &Path, to: &Path) -> AppResult<()> {
        Ok(fs::rename(from, to)?)
    }

    fn copy_file(&self, from: &Path, to: &Path) -> AppResult<u64> {
        Ok(fs::copy(from, to)?)
    }

    fn copy_dir_all(&self, from: &Path, to: &Path) -> AppResult<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let meta = entry.metadata()?; // lstat：不跟随符号链接
            let dest = to.join(entry.file_name());
            if meta.file_type().is_dir() {
                self.copy_dir_all(&entry.path(), &dest)?;
            } else if meta.file_type().is_file() {
                fs::copy(entry.path(), dest)?;
            }
            // 符号链接与其他类型不复制：副本不得携带链接（§9.1 symlink_escape 规则）。
        }
        Ok(())
    }

    fn remove_dir(&self, path: &Path) -> AppResult<()> {
        Ok(fs::remove_dir(path)?)
    }

    fn remove_dir_all(&self, path: &Path) -> AppResult<()> {
        Ok(fs::remove_dir_all(path)?)
    }

    fn remove_file(&self, path: &Path) -> AppResult<()> {
        Ok(fs::remove_file(path)?)
    }

    fn canonicalize(&self, path: &Path) -> AppResult<PathBuf> {
        Ok(fs::canonicalize(path)?)
    }

    fn is_local_volume(&self, path: &Path) -> bool {
        #[cfg(windows)]
        {
            use std::path::{Component, Prefix};
            use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
            use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;
            // 规范化失败（路径不存在等）按原路径兜底；UNC（\\server\share）
            // 无盘符 → 一律视为网络卷跳过。
            let canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let disk = match canon.components().next() {
                Some(Component::Prefix(p)) => match p.kind() {
                    Prefix::Disk(d) | Prefix::VerbatimDisk(d) => Some(d),
                    _ => None,
                },
                _ => None,
            };
            let Some(disk) = disk else { return false };
            let root = format!("{}:\\", disk as char);
            let mut wide: Vec<u16> = root.encode_utf16().collect();
            wide.push(0);
            // 只认 DRIVE_FIXED（本地固定盘）；REMOTE / REMOVABLE / CDROM /
            // RAMDISK / UNKNOWN 一律跳过（保守取向，§7 约束 5）。
            unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_FIXED }
        }
        #[cfg(not(windows))]
        {
            // Unix：/proc /sys /dev /mnt 按首段前缀跳过（§7 约束 5）。
            let first = path.components().find_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            });
            match first.as_deref() {
                Some("proc" | "sys" | "dev" | "mnt") => false,
                _ => true,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// 系统时钟。
#[derive(Debug, Clone, Copy, Default)]
pub struct ProdClock;

impl Clock for ProdClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Hasher
// ---------------------------------------------------------------------------

/// SHA-256 哈希。
#[derive(Debug, Clone, Copy, Default)]
pub struct ProdHasher;

impl ProdHasher {
    fn sha256_hex(&self, data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        to_hex(&h.finalize())
    }
}

impl Hasher for ProdHasher {
    fn hash_bytes(&self, data: &[u8]) -> String {
        self.sha256_hex(data)
    }

    fn hash_file(&self, path: &Path, max_bytes: u64) -> AppResult<FileHash> {
        let meta = fs::symlink_metadata(path)?;
        if !meta.file_type().is_file() {
            return Err(AppError::invalid_input(format!("不是常规文件: {}", path.display())));
        }
        if meta.len() > max_bytes {
            // 超限：用 (mtime, size) 近似并标记（§7 约束 6）。
            let approx = format!("approx:{}:{}", modified_ms(&meta), meta.len());
            Ok(FileHash { hex: self.sha256_hex(approx.as_bytes()), approximated: true })
        } else {
            let data = fs::read(path)?;
            Ok(FileHash { hex: self.sha256_hex(&data), approximated: false })
        }
    }
}

// ---------------------------------------------------------------------------
// Linker
// ---------------------------------------------------------------------------

/// 平台链接实现：Unix symlink / Windows symlink + junction。
#[derive(Debug, Clone, Copy, Default)]
pub struct ProdLinker;

impl ProdLinker {
    #[cfg(windows)]
    fn remove_symlink_windows(link: &Path) -> AppResult<()> {
        // Windows 目录符号链接用 RemoveDirectory 语义删除，文件符号链接用
        // DeleteFile；file_type() 无法区分指向类型，故按顺序尝试。
        match fs::remove_dir(link) {
            Ok(()) => Ok(()),
            Err(_) => fs::remove_file(link).map_err(AppError::from),
        }
    }
}

impl Linker for ProdLinker {
    fn supports_symlink(&self) -> bool {
        // Windows 上实际能否创建取决于开发者模式/管理员权限；
        // 这里保守返回 true，失败由部署回退链处理（§5.2）。
        cfg!(any(windows, unix))
    }

    fn supports_junction(&self) -> bool {
        cfg!(windows)
    }

    fn symlink_dir(&self, target: &Path, link: &Path) -> AppResult<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)?;
            Ok(())
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link)?;
            Ok(())
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (target, link);
            Err(AppError::internal("平台不支持符号链接"))
        }
    }

    fn junction(&self, target: &Path, link: &Path) -> AppResult<()> {
        #[cfg(windows)]
        {
            junction::create(target, link).map_err(|e| AppError::io(e.to_string()))
        }
        #[cfg(not(windows))]
        {
            let _ = (target, link);
            Err(AppError::invalid_input("junction 仅 Windows 可用"))
        }
    }

    fn read_link(&self, link: &Path) -> AppResult<PathBuf> {
        Ok(fs::read_link(link)?)
    }

    fn remove_link(&self, link: &Path) -> AppResult<()> {
        match self.link_kind(link)? {
            LinkKind::NotALink => {
                Err(AppError::invalid_input(format!("不是链接，拒绝删除: {}", link.display())))
            }
            LinkKind::Symlink => {
                #[cfg(unix)]
                {
                    fs::remove_file(link)?;
                    Ok(())
                }
                #[cfg(windows)]
                {
                    Self::remove_symlink_windows(link)
                }
                #[cfg(not(any(unix, windows)))]
                {
                    let _ = link;
                    Err(AppError::internal("平台不支持符号链接"))
                }
            }
            LinkKind::Junction => {
                #[cfg(windows)]
                {
                    junction::delete(link).map_err(|e| AppError::io(e.to_string()))
                }
                #[cfg(not(windows))]
                {
                    let _ = link;
                    Err(AppError::internal("junction 仅 Windows 存在"))
                }
            }
        }
    }

    fn link_kind(&self, path: &Path) -> AppResult<LinkKind> {
        #[cfg(windows)]
        {
            // Windows 上 std 的 is_symlink() 对 junction 同样返回 true
            //（都是 name-surrogate 重解析点），必须先用重解析标签区分。
            if junction::exists(path).unwrap_or(false) {
                return Ok(LinkKind::Junction);
            }
        }
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => Ok(LinkKind::Symlink),
            Ok(_) => Ok(LinkKind::NotALink),
            // 不存在的路径视为 NotALink（Absent 判定由调用方用 exists 区分）
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LinkKind::NotALink),
            Err(e) => Err(AppError::from(e)),
        }
    }
}

// ---------------------------------------------------------------------------
// GitClient
// ---------------------------------------------------------------------------

/// 通过 git 子进程实现（v1 不引入 git2 绑定，保持依赖面最小）。
#[derive(Debug, Clone, Copy, Default)]
pub struct ProdGitClient;

impl ProdGitClient {
    fn run(&self, repo: Option<&Path>, args: &[String]) -> AppResult<std::process::Output> {
        let mut cmd = Command::new("git");
        if let Some(dir) = repo {
            cmd.current_dir(dir);
        }
        cmd.args(args);
        let out = cmd
            .output()
            .map_err(|e| AppError::git(format!("无法启动 git 进程: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(AppError::git(format!(
                "git {} 失败: {stderr}",
                args.first().map(String::as_str).unwrap_or_default()
            )));
        }
        Ok(out)
    }
}

impl GitClient for ProdGitClient {
    fn is_available(&self) -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn clone(&self, url: &str, dest: &Path, depth: Option<u32>) -> AppResult<()> {
        let mut args: Vec<String> = vec!["clone".into(), "--quiet".into()];
        if let Some(d) = depth {
            args.push("--depth".into());
            args.push(d.to_string());
        }
        args.push(url.to_string());
        args.push(dest.to_string_lossy().into_owned());
        self.run(None, &args).map(|_| ())
    }

    fn fetch(&self, repo: &Path) -> AppResult<()> {
        self.run(Some(repo), &["fetch".into(), "--quiet".into()]).map(|_| ())
    }

    fn head_revision(&self, repo: &Path) -> AppResult<String> {
        let out = self.run(Some(repo), &["rev-parse".into(), "HEAD".into()])?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn diff_summary(&self, repo: &Path, from: &str, to: &str) -> AppResult<String> {
        let args: Vec<String> = vec![
            "diff".into(),
            "--stat".into(),
            format!("{from}..{to}"),
        ];
        let out = self.run(Some(repo), &args)?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn checkout(&self, repo: &Path, revision: &str) -> AppResult<()> {
        self.run(
            Some(repo),
            &["checkout".into(), "--quiet".into(), revision.to_string()],
        )
        .map(|_| ())
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("skillbox-ports-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_head_truncates() {
        let dir = temp_root("read_head");
        let f = dir.join("data.txt");
        fs::write(&f, b"0123456789").unwrap();
        let fsx = ProdFileSystem;
        assert_eq!(fsx.read_head(&f, 4).unwrap(), b"0123");
        assert_eq!(fsx.read_head(&f, 100).unwrap(), b"0123456789");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_dir_reports_lstat_kinds() {
        let dir = temp_root("read_dir");
        fs::write(dir.join("a.txt"), b"x").unwrap();
        fs::create_dir_all(dir.join("sub")).unwrap();
        let fsx = ProdFileSystem;
        let mut entries = fsx.read_dir(&dir).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[0].kind, EntryKind::File);
        assert_eq!(entries[1].name, "sub");
        assert_eq!(entries[1].kind, EntryKind::Dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_file_exact_and_approx() {
        let dir = temp_root("hash");
        let small = dir.join("small.txt");
        fs::write(&small, b"hello").unwrap();
        let big = dir.join("big.txt");
        fs::write(&big, vec![0u8; 1024]).unwrap();

        let h = ProdHasher;
        let exact = h.hash_file(&small, 1024).unwrap();
        assert!(!exact.approximated);
        assert_eq!(exact.hex.len(), 64);
        assert_eq!(exact.hex, h.hash_bytes(b"hello"));

        let approx = h.hash_file(&big, 512).unwrap();
        assert!(approx.approximated);
        assert_ne!(approx.hex, h.hash_bytes(&vec![0u8; 1024]));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn link_kind_detects_non_link() {
        let dir = temp_root("link_kind");
        fs::create_dir_all(dir.join("d")).unwrap();
        let linker = ProdLinker;
        assert_eq!(linker.link_kind(&dir.join("d")).unwrap(), LinkKind::NotALink);
        assert_eq!(linker.link_kind(&dir.join("missing")).unwrap(), LinkKind::NotALink);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_link_rejects_real_dir() {
        let dir = temp_root("rm_link");
        fs::create_dir_all(dir.join("d")).unwrap();
        let linker = ProdLinker;
        let err = linker.remove_link(&dir.join("d")).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
        // 目录必须原样保留
        assert!(dir.join("d").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }
}
