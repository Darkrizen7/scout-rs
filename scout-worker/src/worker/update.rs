//! Updates of the tool itself: check for a newer GitHub release, then replace the running binary.

use std::path::Path;
use std::process;

use anyhow::{Context, Result};
use scout_core::update::{self, Release};
use scout_sources::{self as sources, github};

use crate::message::Event;

/// The newest release of the tool when it is newer than this build. Dev builds never update
/// themselves: `cargo run` must not overwrite `target/debug` with a release.
pub(super) fn check_tool() -> Option<Release> {
    if cfg!(debug_assertions) {
        return None;
    }
    // Offline or GitHub unreachable: nothing to offer, not worth an error.
    let release = github::latest_release(github::TOOL_REPO, update::BINARY?).ok()??;
    update::is_newer(&release.version, env!("CARGO_PKG_VERSION")).then_some(release)
}

/// The newest release of the Scout addon (the UI compares it with the installed one).
pub(super) fn check_addon() -> Option<Release> {
    github::latest_release(github::ADDON_REPO, update::ADDON_ARCHIVE)
        .ok()
        .flatten()
}

/// Downloads the release's binary, checks its SHA-256 and puts it in place of the running one.
pub(super) fn install(release: &Release, exe: &Path, send: &dyn Fn(Event)) -> Result<()> {
    send(Event::Status(format!(
        "Téléchargement de la version {}…",
        release.version
    )));
    let binary = github::download_checked(release)?;

    // Next to the executable: same disk, and a folder the user can write to.
    let dir = exe.parent().context("dossier de l'exécutable")?;
    let new = dir.join(".scout-rs-update");
    std::fs::write(&new, &binary).with_context(|| format!("écriture de {}", new.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    }
    let replaced = self_replace::self_replace(&new)
        .with_context(|| format!("remplacement de {}", exe.display()));
    let _ = std::fs::remove_file(&new);
    replaced?;
    send(Event::UpdateInstalled(release.version.clone()));
    send(Event::Status(format!(
        "Version {} installée : redémarre pour l'utiliser.",
        release.version
    )));
    Ok(())
}

/// Starts the (new) executable; the UI closes itself once told.
pub(super) fn restart(exe: &Path, send: &dyn Fn(Event)) -> Result<()> {
    process::Command::new(exe)
        .spawn()
        .with_context(|| format!("lancement de {}", exe.display()))?;
    send(Event::Quit);
    Ok(())
}

/// Downloads the addon's zip, checks it and installs it in the WoW folder.
pub(super) fn install_addon(release: &Release, wow_dir: &Path, send: &dyn Fn(Event)) -> Result<()> {
    send(Event::Status(format!(
        "Téléchargement de l'addon Scout {}…",
        release.version
    )));
    let zip = github::download_checked(release)?;
    sources::install_addon(wow_dir, &zip)?;
    send(Event::ScoutAddon(sources::addon_install(wow_dir)));
    send(Event::Status(format!(
        "Addon Scout {} installé : /reload en jeu (ou relance le jeu).",
        release.version
    )));
    Ok(())
}
