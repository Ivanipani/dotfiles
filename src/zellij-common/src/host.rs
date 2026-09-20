use std::collections::VecDeque;
use std::path::{Component, Path, PathBuf};

/// Where zellij mounts the host filesystem for plugins with `FullHdAccess`,
/// once `change_host_folder("/")` has completed.
pub const HOST_MOUNT: &str = "/host";

/// Resolves `host` (an absolute host path) under `mount`, following symlinks
/// ourselves: WASI cannot follow absolute targets.
pub fn mounted_path(mount: &Path, host: &Path) -> std::io::Result<PathBuf> {
    let mut pending: VecDeque<_> = host.components().map(|c| c.as_os_str().to_owned()).collect();
    let mut resolved = mount.to_path_buf();
    let mut links = 0;
    while let Some(part) = pending.pop_front() {
        match Path::new(&part).components().next() {
            Some(Component::RootDir) => resolved = mount.to_path_buf(),
            Some(Component::ParentDir) => {
                if resolved != mount {
                    resolved.pop();
                }
            }
            Some(Component::Normal(_)) => {
                resolved.push(&part);
                if std::fs::symlink_metadata(&resolved)?.file_type().is_symlink() {
                    links += 1;
                    if links > 40 {
                        return Err(std::io::Error::other("too many symbolic links"));
                    }
                    let target = std::fs::read_link(&resolved)?;
                    resolved.pop();
                    for component in target.components().rev() {
                        pending.push_front(component.as_os_str().to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(resolved)
}

/// Inverse of [`mounted_path`] for a path already under `mount`.
pub fn host_path(mount: &Path, guest: &Path) -> PathBuf {
    Path::new("/").join(guest.strip_prefix(mount).expect("path outside host mount"))
}
