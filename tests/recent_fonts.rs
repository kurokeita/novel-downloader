use novel_downloader::recent_fonts::{load, record, resolve_config_dir};
use std::path::{Path, PathBuf};

/// Write a minimal readable font file whose family name falls back to the stem.
async fn write_font(dir: &Path, name: &str) -> PathBuf {
    write_font_sized(dir, name, 64).await
}

/// Same as [`write_font`] but with an explicit byte length, so tests can make
/// two fonts differ in size.
async fn write_font_sized(dir: &Path, name: &str, size: usize) -> PathBuf {
    let path = dir.join(name);
    let mut buffer = vec![0u8; size];
    buffer[0..4].copy_from_slice(b"\x00\x01\x00\x00");
    tokio::fs::write(&path, &buffer).await.unwrap();
    path
}

/// Path of the store file inside a config root.
fn store_path(config_dir: &Path) -> PathBuf {
    config_dir
        .join("novel-downloader")
        .join("recent-fonts.json")
}

#[tokio::test]
async fn record_then_load_round_trips_the_entry() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let font = write_font(fonts.path(), "Roboto.ttf").await;

    record(config.path(), &font).await.unwrap();
    let loaded = load(config.path()).await;

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].path, std::fs::canonicalize(&font).unwrap());
    assert_eq!(loaded[0].family_name, "Roboto");
    assert_eq!(loaded[0].extension, ".ttf");
    assert_eq!(loaded[0].size, 64);
}

#[tokio::test]
async fn record_creates_the_directory_and_store_file() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let font = write_font(fonts.path(), "Roboto.ttf").await;

    record(config.path(), &font).await.unwrap();

    assert!(store_path(config.path()).exists());
}

#[tokio::test]
async fn load_returns_empty_when_the_store_file_is_missing() {
    let config = tempfile::tempdir().unwrap();
    assert!(load(config.path()).await.is_empty());
}

#[tokio::test]
async fn load_returns_empty_for_an_empty_store_file() {
    let config = tempfile::tempdir().unwrap();
    let path = store_path(config.path());
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&path, b"").await.unwrap();
    assert!(load(config.path()).await.is_empty());
}

#[tokio::test]
async fn load_returns_empty_for_malformed_json() {
    let config = tempfile::tempdir().unwrap();
    let path = store_path(config.path());
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&path, b"{ not json at all").await.unwrap();
    assert!(load(config.path()).await.is_empty());
}

#[tokio::test]
async fn record_repairs_a_malformed_store() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let font = write_font(fonts.path(), "Roboto.ttf").await;
    let path = store_path(config.path());
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&path, b"garbage").await.unwrap();

    record(config.path(), &font).await.unwrap();

    let loaded = load(config.path()).await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].family_name, "Roboto");
}

#[tokio::test]
async fn load_ignores_unknown_fields_and_defaults_absent_ones() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let font = write_font(fonts.path(), "Roboto.ttf").await;
    let canonical = std::fs::canonicalize(&font).unwrap();
    let path = store_path(config.path());
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    let json = format!(
        r#"{{"fonts":[{{"path":{:?},"future_field":true}}],"future_top_level":1}}"#,
        canonical
    );
    tokio::fs::write(&path, json.as_bytes()).await.unwrap();

    let loaded = load(config.path()).await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].path, canonical);
}

#[tokio::test]
async fn recording_a_remembered_font_moves_it_to_the_front() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let a = write_font(fonts.path(), "A.ttf").await;
    let b = write_font(fonts.path(), "B.ttf").await;
    let c = write_font(fonts.path(), "C.ttf").await;
    for font in [&c, &b, &a] {
        record(config.path(), font).await.unwrap();
    }

    record(config.path(), &b).await.unwrap();

    let names: Vec<String> = load(config.path())
        .await
        .into_iter()
        .map(|f| f.family_name)
        .collect();
    assert_eq!(names, vec!["B", "A", "C"]);
}

#[tokio::test]
async fn a_new_font_at_capacity_evicts_the_oldest_entry() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    for index in 0..10 {
        let font = write_font(fonts.path(), &format!("F{index}.ttf")).await;
        record(config.path(), &font).await.unwrap();
    }
    let newcomer = write_font(fonts.path(), "New.ttf").await;

    record(config.path(), &newcomer).await.unwrap();

    let names: Vec<String> = load(config.path())
        .await
        .into_iter()
        .map(|f| f.family_name)
        .collect();
    assert_eq!(names.len(), 10);
    assert_eq!(names[0], "New");
    assert!(!names.contains(&"F0".to_string()));
}

#[tokio::test]
async fn two_equivalent_paths_collapse_to_one_entry() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let font = write_font(fonts.path(), "Roboto.ttf").await;
    let equivalent = fonts.path().join(".").join("Roboto.ttf");

    record(config.path(), &font).await.unwrap();
    record(config.path(), &equivalent).await.unwrap();

    assert_eq!(load(config.path()).await.len(), 1);
}

/// Write a store file verbatim, so a test can tell a rewrite apart from an
/// untouched file by the formatting alone.
async fn write_raw_store(config_dir: &Path, json: &str) {
    let path = store_path(config_dir);
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&path, json.as_bytes()).await.unwrap();
}

/// Compact one-entry JSON for `path`, distinguishable from a pretty rewrite.
fn compact_store(path: &Path, family: &str, size: u64) -> String {
    format!(
        r#"{{"fonts":[{{"path":{:?},"family_name":"{}","extension":".ttf","size":{}}}]}}"#,
        path, family, size
    )
}

#[tokio::test]
async fn a_deleted_font_is_pruned_and_survivors_keep_their_order() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let a = write_font(fonts.path(), "A.ttf").await;
    let b = write_font(fonts.path(), "B.ttf").await;
    let c = write_font(fonts.path(), "C.ttf").await;
    for font in [&c, &b, &a] {
        record(config.path(), font).await.unwrap();
    }
    tokio::fs::remove_file(&b).await.unwrap();

    let names: Vec<String> = load(config.path())
        .await
        .into_iter()
        .map(|f| f.family_name)
        .collect();
    assert_eq!(names, vec!["A", "C"]);
}

#[tokio::test]
async fn a_load_that_pruned_rewrites_the_store() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let a = write_font(fonts.path(), "A.ttf").await;
    let b = write_font(fonts.path(), "B.ttf").await;
    record(config.path(), &b).await.unwrap();
    record(config.path(), &a).await.unwrap();
    tokio::fs::remove_file(&b).await.unwrap();

    load(config.path()).await;

    let raw = tokio::fs::read_to_string(store_path(config.path()))
        .await
        .unwrap();
    assert!(!raw.contains("B.ttf"), "pruned entry still in store: {raw}");
}

#[tokio::test]
async fn a_clean_load_leaves_the_store_untouched() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let font = write_font(fonts.path(), "Roboto.ttf").await;
    let canonical = std::fs::canonicalize(&font).unwrap();
    let json = compact_store(&canonical, "Roboto", 64);
    write_raw_store(config.path(), &json).await;

    assert_eq!(load(config.path()).await.len(), 1);

    let raw = tokio::fs::read_to_string(store_path(config.path()))
        .await
        .unwrap();
    assert_eq!(raw, json);
}

/// Build a raw store from `(path, family, size)` triples, so a test can seed
/// cached metadata that deliberately disagrees with the file on disk.
fn raw_store(entries: &[(&Path, &str, u64)]) -> String {
    let items: Vec<String> = entries
        .iter()
        .map(|(path, family, size)| {
            format!(
                r#"{{"path":{:?},"family_name":"{}","extension":".ttf","size":{}}}"#,
                path, family, size
            )
        })
        .collect();
    format!(r#"{{"fonts":[{}]}}"#, items.join(","))
}

#[tokio::test]
async fn an_unchanged_font_is_served_from_the_cached_family_name() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let font = write_font(fonts.path(), "Roboto.ttf").await;
    let canonical = std::fs::canonicalize(&font).unwrap();
    write_raw_store(
        config.path(),
        &raw_store(&[(&canonical, "Cached Family", 64)]),
    )
    .await;

    let loaded = load(config.path()).await;

    assert_eq!(loaded[0].family_name, "Cached Family");
}

#[tokio::test]
async fn a_replaced_font_of_a_different_size_is_refreshed_in_place() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let first = write_font(fonts.path(), "First.ttf").await;
    let middle = write_font(fonts.path(), "Middle.ttf").await;
    let last = write_font(fonts.path(), "Last.ttf").await;
    let canonical: Vec<PathBuf> = [&first, &middle, &last]
        .iter()
        .map(|p| std::fs::canonicalize(p).unwrap())
        .collect();
    write_raw_store(
        config.path(),
        &raw_store(&[
            (&canonical[0], "First", 64),
            (&canonical[1], "Stale", 64),
            (&canonical[2], "Last", 64),
        ]),
    )
    .await;
    write_font_sized(fonts.path(), "Middle.ttf", 128).await;

    let loaded = load(config.path()).await;

    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[1].path, canonical[1]);
    assert_eq!(loaded[1].family_name, "Middle");
    assert_eq!(loaded[1].size, 128);

    let raw = tokio::fs::read_to_string(store_path(config.path()))
        .await
        .unwrap();
    assert!(raw.contains("128"), "refreshed size not persisted: {raw}");
}

#[tokio::test]
async fn a_path_that_is_not_a_font_leaves_the_store_unchanged() {
    let config = tempfile::tempdir().unwrap();
    let fonts = tempfile::tempdir().unwrap();
    let good = write_font(fonts.path(), "Roboto.ttf").await;
    record(config.path(), &good).await.unwrap();
    let notes = fonts.path().join("notes.txt");
    tokio::fs::write(&notes, b"nope").await.unwrap();

    record(config.path(), &notes).await.unwrap();
    record(config.path(), &fonts.path().join("missing.ttf"))
        .await
        .unwrap();

    let loaded = load(config.path()).await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].family_name, "Roboto");
}

#[tokio::test]
async fn the_bundled_font_is_never_recorded() {
    let config = tempfile::tempdir().unwrap();
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Bokerlam.ttf");

    record(config.path(), &bundled).await.unwrap();

    assert!(load(config.path()).await.is_empty());
}

#[test]
fn xdg_config_home_wins_when_set_and_non_empty() {
    assert_eq!(
        resolve_config_dir(Some("/xdg"), Some("/home/user")),
        Some(PathBuf::from("/xdg"))
    );
}

#[test]
fn an_empty_xdg_config_home_falls_back_to_home() {
    assert_eq!(
        resolve_config_dir(Some(""), Some("/home/user")),
        Some(PathBuf::from("/home/user/.config"))
    );
}

#[test]
fn home_is_the_fallback_config_root() {
    assert_eq!(
        resolve_config_dir(None, Some("/home/user")),
        Some(PathBuf::from("/home/user/.config"))
    );
}

#[test]
fn no_config_root_is_available_without_xdg_or_home() {
    assert_eq!(resolve_config_dir(None, None), None);
}
