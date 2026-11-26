//! The Scout addon in `Interface/AddOns`: what is installed, and installing a release's zip.

use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use scout_core::addon_history::HISTORY_FILE;
use scout_core::update::{AddonInstall, toc_version};

fn addons(wow_dir: &Path) -> PathBuf {
    wow_dir.join("Interface/AddOns")
}

/// A link to a working copy or a git clone: the developer's own, never replaced.
fn is_dev(dir: &Path) -> bool {
    fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) || dir.join(".git").exists()
}

/// What the WoW folder holds: no addon, a dev install, or an install and its version.
pub fn addon_install(wow_dir: &Path) -> AddonInstall {
    let dir = addons(wow_dir).join("Scout");
    let Ok(toc) = fs::read(dir.join("Scout.toc")) else {
        return AddonInstall::Missing;
    };
    let version = toc_version(&String::from_utf8_lossy(&toc));
    if is_dev(&dir) {
        AddonInstall::Dev(version)
    } else {
        AddonInstall::Installed(version)
    }
}

/// Puts the release's `Scout/` folder in place of the installed one. The new files are unpacked
/// beside it first, so a failure leaves the old addon as it was. Saved data lives in `WTF/`: untouched.
pub fn install_addon(wow_dir: &Path, zip: &[u8]) -> Result<()> {
    let addons = addons(wow_dir);
    let target = addons.join("Scout");
    if is_dev(&target) {
        bail!(
            "{} est une installation de développement : rien n'est remplacé",
            target.display()
        );
    }
    fs::create_dir_all(&addons).with_context(|| format!("création de {}", addons.display()))?;
    // Dot folders: the game only loads a folder holding a .toc of the same name.
    let staging = addons.join(".Scout-update");
    let old = addons.join(".Scout-old");
    for leftover in [&staging, &old] {
        if leftover.exists() {
            fs::remove_dir_all(leftover)?;
        }
    }
    let unpacked = unpack(zip, &staging);
    if let Err(e) = unpacked {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    if target.exists() {
        fs::rename(&target, &old)
            .with_context(|| format!("déplacement de {}", target.display()))?;
    }
    if let Err(e) = fs::rename(&staging, &target) {
        // Put the previous addon back.
        if old.exists() {
            let _ = fs::rename(&old, &target);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(e).with_context(|| format!("installation dans {}", target.display()));
    }
    if old.exists() {
        fs::remove_dir_all(&old)?;
    }
    Ok(())
}

/// Writes `History.lua` in the addon's folder: to a temporary file first, then renamed over the
/// old one, so the game never reads a half-written file. Works through a dev install's symlink.
pub fn write_history(wow_dir: &Path, lua: &str) -> Result<()> {
    let dir = addons(wow_dir).join("Scout");
    let tmp = dir.join(".History.lua.tmp");
    let target = dir.join(HISTORY_FILE);
    fs::write(&tmp, lua).with_context(|| format!("écriture de {}", tmp.display()))?;
    fs::rename(&tmp, &target).with_context(|| format!("remplacement de {}", target.display()))
}

/// The zip's `Scout/…` entries, written under `dest`; refuses paths that leave it.
fn unpack(zip: &[u8], dest: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).context("zip de l'addon illisible")?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(path) = entry.enclosed_name() else {
            bail!(
                "chemin invalide dans le zip : {}",
                String::from_utf8_lossy(entry.name_raw())
            );
        };
        let Ok(relative) = path.strip_prefix("Scout") else {
            continue;
        };
        let out = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file =
            fs::File::create(&out).with_context(|| format!("écriture de {}", out.display()))?;
        io::copy(&mut entry, &mut file)?;
    }
    if !dest.join("Scout.toc").is_file() {
        bail!("le zip ne contient pas Scout/Scout.toc");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn temp_wow(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("scout-addon-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn installs_then_updates_the_addon() {
        let wow = temp_wow("update");
        assert_eq!(addon_install(&wow), AddonInstall::Missing);

        let v1 = zip(&[
            ("Scout/Scout.toc", "## Version: 0.1.0\n"),
            ("Scout/Old.lua", ""),
        ]);
        install_addon(&wow, &v1).unwrap();
        assert_eq!(
            addon_install(&wow),
            AddonInstall::Installed(Some("0.1.0".into()))
        );

        let v2 = zip(&[
            ("Scout/Scout.toc", "## Version: 0.2.0\n"),
            ("Scout/tools/export.lua", ""),
        ]);
        install_addon(&wow, &v2).unwrap();
        let dir = wow.join("Interface/AddOns/Scout");
        assert_eq!(
            addon_install(&wow),
            AddonInstall::Installed(Some("0.2.0".into()))
        );
        assert!(dir.join("tools/export.lua").is_file());
        assert!(
            !dir.join("Old.lua").exists(),
            "files of the old version are gone"
        );
        assert!(!wow.join("Interface/AddOns/.Scout-old").exists());
        fs::remove_dir_all(&wow).unwrap();
    }

    #[test]
    fn keeps_the_old_addon_when_the_zip_is_wrong() {
        let wow = temp_wow("wrong");
        install_addon(&wow, &zip(&[("Scout/Scout.toc", "## Version: 0.1.0\n")])).unwrap();
        assert!(install_addon(&wow, &zip(&[("Other/Other.toc", "")])).is_err());
        assert!(install_addon(&wow, b"not a zip").is_err());
        assert_eq!(
            addon_install(&wow),
            AddonInstall::Installed(Some("0.1.0".into()))
        );
        fs::remove_dir_all(&wow).unwrap();
    }

    #[test]
    fn never_replaces_a_dev_install() {
        let wow = temp_wow("dev");
        let dir = wow.join("Interface/AddOns/Scout");
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join("Scout.toc"), "## Version: 0.1.0\n").unwrap();
        assert_eq!(addon_install(&wow), AddonInstall::Dev(Some("0.1.0".into())));
        let v2 = zip(&[("Scout/Scout.toc", "## Version: 0.2.0\n")]);
        assert!(install_addon(&wow, &v2).is_err());
        assert!(dir.join(".git").is_dir());
        fs::remove_dir_all(&wow).unwrap();
    }

    #[test]
    fn writes_history_atomically() {
        let wow = temp_wow("history");
        let dir = wow.join("Interface/AddOns/Scout");
        fs::create_dir_all(&dir).unwrap();
        write_history(&wow, "a").unwrap();
        write_history(&wow, "ScoutHistory = {}").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(HISTORY_FILE)).unwrap(),
            "ScoutHistory = {}"
        );
        assert!(!dir.join(".History.lua.tmp").exists());
        assert!(write_history(&temp_wow("history-none").join("x"), "a").is_err());
        fs::remove_dir_all(&wow).unwrap();
    }
}
