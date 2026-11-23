//! Updates of the tool itself: check for a newer GitHub release, then replace the running binary.

use std::fmt::Write as _;
use std::path::Path;
use std::process;

use anyhow::{Context, Result, bail};
use scout_core::update::{self, Release};
use scout_sources::github;
use sha2::{Digest, Sha256};

use crate::message::Event;

/// The newest release when it is newer than this build. Dev builds never update themselves:
/// `cargo run` must not overwrite `target/debug` with a release.
pub(super) fn check() -> Option<Release> {
    if cfg!(debug_assertions) {
        return None;
    }
    // Offline or GitHub unreachable: nothing to offer, not worth an error.
    let release = github::latest_release().ok()??;
    update::is_newer(&release.version, env!("CARGO_PKG_VERSION")).then_some(release)
}

/// Downloads the release's binary, checks its SHA-256 and puts it in place of the running one.
pub(super) fn install(release: &Release, exe: &Path, send: &dyn Fn(Event)) -> Result<()> {
    send(Event::Status(format!(
        "Téléchargement de la version {}…",
        release.version
    )));
    let checksum = String::from_utf8(github::download(&release.checksum_url)?)
        .ok()
        .as_deref()
        .and_then(update::parse_checksum)
        .context("somme de contrôle de la mise à jour illisible")?;
    let binary = github::download(&release.binary_url)?;
    let actual = Sha256::digest(&binary)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        });
    if actual != checksum {
        bail!("mise à jour corrompue (somme de contrôle différente), rien n'a été remplacé");
    }

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
