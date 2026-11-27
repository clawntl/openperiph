//! Updating OpenPeriph itself from its GitHub releases.
//!
//! How depends on how it was installed:
//! - **Arch package** (the program belongs to pacman): download the release's `.pkg.tar.zst` and
//!   install it with `pkexec pacman -U`, which asks for the password in a system window.
//! - **A file you can write to** (the Windows .exe, or the Linux binary in your home folder):
//!   download the new one next to it and swap them. Windows can't overwrite a running .exe but
//!   can rename it, so the old one becomes `openperiph.old.exe` and is deleted next start.
//! - **Windows installer** (an `unins000.exe` sits next to the program): download the new
//!   installer and run it quietly; it closes this copy, installs and starts the new one.
//! - **Anywhere else** (a system folder without pacman): open the release page instead.
//!
//! Downloads are checked against the SHA-256 GitHub reports for each release file.

use std::path::{Path, PathBuf};

use crate::net::{self, GhAsset, GhRelease};
use crate::qmk::firmware::parse_version;

pub const REPO: &str = "clawntl/openperiph";

pub fn current_version() -> u32 {
    parse_version(env!("CARGO_PKG_VERSION")).unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum InstallKind {
    /// Owned by pacman.
    Pacman,
    /// A program file this user can replace.
    Replace(PathBuf),
    /// Installed by the Windows installer.
    WindowsSetup,
    /// Can't update in place; show the release page.
    Manual,
}

#[derive(Clone, Debug)]
pub struct AppRelease {
    pub release: GhRelease,
    pub version: u32,
}

impl AppRelease {
    pub fn newer(&self) -> bool {
        self.version > current_version()
    }
}

pub fn latest() -> Result<AppRelease, String> {
    let release = net::latest_release(REPO)?;
    let version = parse_version(&release.tag).ok_or_else(|| format!("The latest release, {}, has no version number.", release.tag))?;
    Ok(AppRelease { release, version })
}

fn writable_dir(p: &Path) -> bool {
    let Some(dir) = p.parent() else { return false };
    let probe = dir.join(".openperiph-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn install_kind() -> InstallKind {
    let Ok(exe) = std::env::current_exe() else {
        return InstallKind::Manual;
    };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    #[cfg(target_os = "linux")]
    {
        let owned = std::process::Command::new("pacman")
            .args(["-Qqo"])
            .arg(&exe)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if owned {
            return InstallKind::Pacman;
        }
    }
    if cfg!(windows) && exe.with_file_name("unins000.exe").is_file() {
        return InstallKind::WindowsSetup;
    }
    if writable_dir(&exe) {
        InstallKind::Replace(exe)
    } else {
        InstallKind::Manual
    }
}

/// The release file this kind of install needs.
pub fn asset<'a>(kind: &InstallKind, r: &'a GhRelease) -> Option<&'a GhAsset> {
    let find = |f: &dyn Fn(&str) -> bool| r.assets.iter().find(|a| f(&a.name));
    match kind {
        InstallKind::Pacman => {
            let arch = std::env::consts::ARCH;
            find(&|n| n.starts_with("openperiph-") && n.ends_with(&format!("-{arch}.pkg.tar.zst")))
        }
        InstallKind::WindowsSetup => find(&|n| n == "openperiph-setup-x64.exe"),
        InstallKind::Replace(_) if cfg!(windows) => find(&|n| n == "openperiph.exe"),
        InstallKind::Replace(_) if cfg!(all(target_os = "linux", target_arch = "x86_64")) => find(&|n| n == "openperiph-linux-x64"),
        _ => None,
    }
}

/// Puts `data` in place of the program at `exe`. Returns the program to start.
pub fn replace_program(exe: &Path, data: &[u8]) -> Result<PathBuf, String> {
    let dir = exe.parent().ok_or("No folder for the program.")?;
    let new = dir.join(if cfg!(windows) { "openperiph.new.exe" } else { ".openperiph.new" });
    std::fs::write(&new, data).map_err(|e| format!("Couldn't save the new version: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    if cfg!(windows) {
        // A running .exe can be renamed but not overwritten.
        let old = old_path(exe);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old).map_err(|e| format!("Couldn't move the old version aside: {e}"))?;
        if let Err(e) = std::fs::rename(&new, exe) {
            let _ = std::fs::rename(&old, exe);
            return Err(format!("Couldn't put the new version in place: {e}"));
        }
    } else {
        // Replacing the file a running program came from is fine on Linux.
        std::fs::rename(&new, exe).map_err(|e| format!("Couldn't put the new version in place: {e}"))?;
    }
    Ok(exe.to_path_buf())
}

fn old_path(exe: &Path) -> PathBuf {
    exe.with_file_name(if cfg!(windows) { "openperiph.old.exe" } else { ".openperiph.old" })
}

/// Deletes the version an earlier update left behind (Windows).
pub fn clean_up() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

/// Downloads and installs; returns the program to start afterwards.
pub fn install(r: &AppRelease, kind: &InstallKind, progress: &dyn Fn(&str, f32)) -> Result<PathBuf, String> {
    let a = asset(kind, &r.release).ok_or("This release has no file for this kind of install. Download it from the release page.")?;
    progress("Downloading", 0.0);
    let data = net::download(&a.url, 200 * 1024 * 1024, &|f| progress("Downloading", f))?;
    net::verify(&data, a.sha256.as_deref())?;
    progress("Installing", 1.0);
    match kind {
        InstallKind::Pacman => {
            let file = std::env::temp_dir().join(&a.name);
            std::fs::write(&file, &data).map_err(|e| e.to_string())?;
            let status = std::process::Command::new("pkexec")
                .args(["pacman", "-U", "--noconfirm"])
                .arg(&file)
                .status()
                .map_err(|_| "pkexec isn't available; install the package with: sudo pacman -U <file>".to_string())?;
            let _ = std::fs::remove_file(&file);
            if !status.success() {
                return Err("The package wasn't installed (the password window was closed, or pacman refused).".into());
            }
            Ok(PathBuf::from("/usr/bin/openperiph"))
        }
        InstallKind::Replace(exe) => replace_program(exe, &data),
        InstallKind::WindowsSetup => {
            let file = std::env::temp_dir().join(&a.name);
            std::fs::write(&file, &data).map_err(|e| format!("Couldn't save the installer: {e}"))?;
            // the same install mode as before: per user, or for everyone (then Windows asks)
            let per_user = std::env::current_exe().map(|e| writable_dir(&e)).unwrap_or(true);
            std::process::Command::new(&file)
                .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", if per_user { "/CURRENTUSER" } else { "/ALLUSERS" }])
                .spawn()
                .map_err(|e| format!("Couldn't start the installer: {e}"))?;
            // The installer replaces this program and starts the new one.
            std::process::exit(0);
        }
        InstallKind::Manual => Err("Can't update this copy in place.".into()),
    }
}

/// Starts the new version and ends this one.
pub fn restart(program: &Path) -> ! {
    let _ = std::process::Command::new(program).spawn();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_right_file() {
        let names = ["openperiph.exe", "openperiph-linux-x64", "openperiph-0.3.0-1-x86_64.pkg.tar.zst", "openperiph-v0.3.0-windows-x64.zip", "openperiph-setup-x64.exe"];
        let r = GhRelease {
            tag: "v0.3.0".into(),
            page_url: String::new(),
            body: String::new(),
            assets: names.iter().map(|n| GhAsset { name: n.to_string(), ..Default::default() }).collect(),
        };
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            assert_eq!(asset(&InstallKind::Pacman, &r).unwrap().name, names[2]);
            assert_eq!(asset(&InstallKind::Replace(PathBuf::new()), &r).unwrap().name, names[1]);
        }
        if cfg!(windows) {
            assert_eq!(asset(&InstallKind::Replace(PathBuf::new()), &r).unwrap().name, names[0]);
        }
        assert_eq!(asset(&InstallKind::WindowsSetup, &r).unwrap().name, names[4]);
        assert!(asset(&InstallKind::Manual, &r).is_none());
    }

    #[test]
    fn replaces_a_program_file() {
        let dir = std::env::temp_dir().join(format!("openperiph-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) { "openperiph.exe" } else { "openperiph" });
        std::fs::write(&exe, b"old").unwrap();
        let out = replace_program(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum() {
        // sha256("abc")
        let h = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(net::verify(b"abc", Some(h)).is_ok());
        assert!(net::verify(b"abd", Some(h)).is_err());
        assert!(net::verify(b"abd", None).is_ok());
    }
}
