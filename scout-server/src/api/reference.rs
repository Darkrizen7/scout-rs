//! The reference package (loot and merchants), built into the binary from `data/` (see `docs/serveur.md`).

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use scout_core::reference::ReferenceManifest;

use super::Format;

static PACKAGE: &[u8] = include_bytes!("../../data/reference.db.gz");
static MANIFEST: &str = include_str!("../../data/reference.json");

/// The manifest of the package built in. Checked by a test, so a bad file fails the build of the server.
fn manifest() -> ReferenceManifest {
    serde_json::from_str(MANIFEST).expect("data/reference.json")
}

/// The manifest when the package has the app's `format`, 404 otherwise.
pub(super) async fn latest(Query(Format { format }): Query<Format>) -> Response {
    let manifest = manifest();
    if manifest.format == format {
        Json(manifest).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            "pas de données de référence pour ce format\n",
        )
            .into_response()
    }
}

/// The package itself (gzipped SQLite), only if it is the version served.
pub(super) async fn package(Path((format, updated)): Path<(i64, i64)>) -> Response {
    let manifest = manifest();
    if manifest.format == format && manifest.updated == updated {
        ([(header::CONTENT_TYPE, "application/gzip")], PACKAGE).into_response()
    } else {
        (StatusCode::NOT_FOUND, "paquet inconnu ou remplacé\n").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scout_core::reference::REFERENCE_FORMAT;

    #[test]
    fn built_in_manifest_matches_the_package() {
        let manifest = manifest();
        assert_eq!(manifest.format, REFERENCE_FORMAT);
        assert_eq!(manifest.bytes, PACKAGE.len() as u64);
    }
}
