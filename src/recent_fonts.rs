use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::utils::validate_font_file;

/// How many fonts the store remembers before the oldest one falls off.
pub const MAX_RECENT_FONTS: usize = 10;

/// One remembered custom EPUB font, with the metadata cached at record time so
/// listing the fonts costs no font parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentFont {
    /// Canonical absolute path of the font file.
    pub path: PathBuf,
    /// Family name as extracted when the font was recorded or last refreshed.
    #[serde(default)]
    pub family_name: String,
    /// Lowercased dot-prefixed file extension (e.g. `.ttf`).
    #[serde(default)]
    pub extension: String,
    /// File size in bytes, used to detect that the cached metadata is stale.
    #[serde(default)]
    pub size: u64,
}

/// On-disk container. Kept separate from the entry list so the format can grow
/// fields without breaking older stores.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    fonts: Vec<RecentFont>,
}

/// Path of the store file inside `config_dir`.
fn store_path(config_dir: &Path) -> PathBuf {
    config_dir
        .join("novel-downloader")
        .join("recent-fonts.json")
}

/// Read the store, treating any missing, empty, or malformed file as empty.
async fn read_store(config_dir: &Path) -> Store {
    let Ok(bytes) = tokio::fs::read(store_path(config_dir)).await else {
        return Store::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Write the store, creating the application directory when needed.
async fn write_store(config_dir: &Path, store: &Store) -> Result<()> {
    let path = store_path(config_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, serde_json::to_vec_pretty(store)?).await?;
    Ok(())
}

/// Load the remembered fonts, newest-used first, dropping every entry whose
/// file can no longer be inspected. Entries are stat'ed concurrently so a
/// single hung lookup on a stale network mount costs one wait, not a sum of
/// waits. Pruning is silent; when it changed the list the store is rewritten
/// on a best-effort basis, since a store that cannot be written is no reason
/// to fail the run.
pub async fn load(config_dir: &Path) -> Vec<RecentFont> {
    let stored = read_store(config_dir).await.fonts;
    let count_before = stored.len();
    let checked = futures::future::join_all(stored.into_iter().map(|font| async move {
        let metadata = tokio::fs::metadata(&font.path).await.ok()?;
        Some((font, metadata.len()))
    }))
    .await;

    let mut fonts: Vec<RecentFont> = Vec::with_capacity(checked.len());
    let mut refreshed = false;
    for (mut font, size) in checked.into_iter().flatten() {
        if font.size != size
            && let Ok(metadata) = crate::font::extract_font_metadata(&font.path).await
        {
            font.family_name = metadata.family_name;
            font.extension = metadata.extension;
            font.size = size;
            refreshed = true;
        }
        fonts.push(font);
    }

    if refreshed || fonts.len() != count_before {
        let _ = write_store(
            config_dir,
            &Store {
                fonts: fonts.clone(),
            },
        )
        .await;
    }
    fonts
}

/// Return true when `path` is the bundled `Bokerlam.ttf` that ships with the
/// application. Compared by canonical path so a user's own unrelated file of
/// the same name is still remembered.
async fn is_bundled_font(path: &Path) -> bool {
    let Ok(Some(bundled)) = crate::utils::find_font_file(None).await else {
        return false;
    };
    tokio::fs::canonicalize(&bundled)
        .await
        .is_ok_and(|bundled| bundled == path)
}

/// Remember `path` as the most recently used custom font. A path that cannot
/// be read as a font, and the bundled font, are silently ignored: neither is a
/// failure the user needs to hear about at the end of a confirmed plan.
pub async fn record(config_dir: &Path, path: &Path) -> Result<()> {
    let Ok((canonical, metadata)) = validate_font_file(path).await else {
        return Ok(());
    };
    if is_bundled_font(&canonical).await {
        return Ok(());
    }
    let size = tokio::fs::metadata(&canonical).await?.len();
    let mut store = read_store(config_dir).await;
    store.fonts.retain(|font| font.path != canonical);
    store.fonts.insert(
        0,
        RecentFont {
            path: canonical,
            family_name: metadata.family_name,
            extension: metadata.extension,
            size,
        },
    );
    store.fonts.truncate(MAX_RECENT_FONTS);
    write_store(config_dir, &store).await
}

/// Pure config-root resolution: `$XDG_CONFIG_HOME` when set and non-empty,
/// otherwise `$HOME/.config`, otherwise `None`. Kept separate from the
/// environment read so tests never mutate process-global variables.
pub fn resolve_config_dir(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(xdg));
    }
    home.filter(|value| !value.is_empty())
        .map(|home| Path::new(home).join(".config"))
}

/// Config root for this application's stored state, or `None` when the
/// environment offers none — in which case persistence is simply disabled.
pub fn config_dir() -> Option<PathBuf> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    resolve_config_dir(xdg.as_deref(), home.as_deref())
}
