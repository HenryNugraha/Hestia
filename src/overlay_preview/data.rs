//! The overlay's library: the one Hestia sends, or in the preview a saved
//! sample of one.

use std::path::{Path, PathBuf};

use crate::{
    model::{DISABLED_CONTAINER, MOD_META_DIR},
    overlay_protocol::{ModHotkey, ModHotkeys, Settings, Start},
};

/// The preview's sample library, in target/overlay-preview.
const SAMPLE_FILE: &str = "sample-library.json";

#[derive(Clone, Default)]
pub(super) struct Catalog {
    pub game: String,
    pub categories: Vec<Category>,
    pub note: Option<String>,
}

/// Ids keep the overlay's place when the library changes under it.
#[derive(Clone, Default)]
pub(super) struct Category {
    pub id: String,
    pub name: String,
    pub image: Option<PathBuf>,
    /// The GameBanana character whose mods the in-game overlay shows after
    /// the category's own.
    pub character: Option<u64>,
    /// A GameBanana character without a category, which "Show all
    /// characters" adds after the library's categories.
    pub extra: bool,
    pub costumes: Vec<Costume>,
}

#[derive(Clone, Default)]
pub(super) struct Costume {
    pub id: String,
    pub name: String,
    pub image: Option<PathBuf>,
    pub active: bool,
    /// The library censors its picture, so it shows darkened.
    pub censored: bool,
    /// The GameBanana mod it was installed from.
    pub gamebanana_id: Option<u64>,
}

/// The library Hestia sends to the live overlay.
pub(super) fn catalog_from_library(library: crate::overlay_protocol::Library) -> Catalog {
    Catalog {
        game: library.game_name,
        categories: library
            .categories
            .into_iter()
            .map(|category| Category {
                id: category.id,
                name: category.name,
                image: category.image,
                character: category.character,
                extra: false,
                costumes: category
                    .mods
                    .into_iter()
                    .map(|item| Costume {
                        id: item.id,
                        name: item.name,
                        image: item.image,
                        active: item.active,
                        censored: item.censored,
                        gamebanana_id: item.gamebanana_id,
                    })
                    .collect(),
            })
            .collect(),
        note: None,
    }
}

/// The hotkeys the preview gives every mod, enough to scroll.
pub(super) fn sample_hotkeys(mod_id: &str) -> ModHotkeys {
    let hotkeys = [
        ("Alt+H", "Menu"),
        ("H", "Hat"),
        ("J", "Jacket"),
        ("K", "Skirt length"),
        ("Ctrl+L", "Long hair"),
        ("Shift+O", "Outfit color"),
        ("Up", "Glasses"),
        ("Num 5", "Weapon glow"),
        (
            "Ctrl+Shift+Num 9",
            "Reset everything to how the mod's author set it up",
        ),
        ("F7", "Swap / pose"),
    ];
    ModHotkeys {
        mod_id: mod_id.to_owned(),
        hotkeys: hotkeys
            .into_iter()
            .map(|(key, label)| ModHotkey {
                key: key.to_owned(),
                label: label.to_owned(),
                raw: key.to_owned(),
            })
            .collect(),
    }
}

/// The preview's library: a start message the in-game overlay saved, so the
/// screenshot runs don't change with the real library.  Start Hestia with
/// `HESTIA_OVERLAY_SAVE_SAMPLE` set to a file to save one.
pub(super) fn load_sample() -> (Catalog, Settings) {
    let start = sample_path()
        .ok_or_else(|| anyhow::anyhow!("no {SAMPLE_FILE} in target/overlay-preview"))
        .and_then(|path| read_sample(&path));
    match start {
        Ok(start) => (catalog_from_library(start.library), start.settings),
        Err(error) => (
            Catalog {
                game: "Mods".into(),
                categories: Vec::new(),
                note: Some(format!(
                    "Could not load the sample library: {error:#}. Start Hestia with \
                     HESTIA_OVERLAY_SAVE_SAMPLE set to a file, then start a game, to save one."
                )),
            },
            Settings::default(),
        ),
    }
}

/// Saves the start message as the preview's sample library, when
/// `HESTIA_OVERLAY_SAVE_SAMPLE` asks for it.
pub(super) fn save_sample(start: &Start) {
    let Some(path) = std::env::var_os("HESTIA_OVERLAY_SAVE_SAMPLE") else {
        return;
    };
    let path = PathBuf::from(path);
    match write_sample(&path, start) {
        Ok(()) => tracing::info!(?path, "Saved the overlay's sample library"),
        Err(error) => tracing::warn!(?path, %error, "Could not save the overlay's sample library"),
    }
}

fn sample_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HESTIA_OVERLAY_PREVIEW_SAMPLE") {
        return Some(PathBuf::from(path));
    }
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("target/overlay-preview").join(SAMPLE_FILE));
    }
    if let Ok(exe) = std::env::current_exe() {
        candidates.extend(
            exe.ancestors()
                .skip(1)
                .map(|directory| directory.join("overlay-preview").join(SAMPLE_FILE)),
        );
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn read_sample(path: &Path) -> anyhow::Result<Start> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn write_sample(path: &Path, start: &Start) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(start)?)?;
    Ok(())
}

/// A mod's picture for the overlay's cards: its cover, else a screenshot, else
/// its GameBanana preview if Hestia has downloaded it, else a thumbnail Hestia
/// baked.  The thumbnails are small for the cards, so they come last.
pub(crate) fn mod_image(
    root: &Path,
    cover: Option<&str>,
    screenshots: &[String],
    downloaded_preview: Option<PathBuf>,
) -> Option<PathBuf> {
    cover
        .and_then(|image| resolve_image(root, image))
        .or_else(|| {
            screenshots
                .iter()
                .find_map(|image| resolve_image(root, image))
        })
        .or_else(|| downloaded_preview.filter(|path| path.is_file()))
        .or_else(|| {
            [
                "card_thumb_v2.png",
                "card_thumb.png",
                "rail_thumb.png",
                "icon_thumb.png",
            ]
            .into_iter()
            .map(|name| root.join(MOD_META_DIR).join(name))
            .find(|path| path.is_file())
        })
}

fn resolve_image(root: &Path, value: &str) -> Option<PathBuf> {
    if value.trim().is_empty() {
        return None;
    }
    if value.starts_with("http:") || value.starts_with("https:") {
        return None;
    }
    let image = Path::new(value);
    [
        image.to_path_buf(),
        root.join(image),
        root.join(DISABLED_CONTAINER).join(image),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

pub(super) fn state_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HESTIA_OVERLAY_PREVIEW_STATE") {
        return Some(PathBuf::from(path));
    }
    let mut candidates = Vec::new();
    // This isolated development preview follows the installed app's settings,
    // rather than an old portable fixture left beside target/debug/hestia.exe.
    // An explicitly selected portable profile still wins via the override above.
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(PathBuf::from(appdata).join("Hestia/hestia.toml"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("hestia.toml"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("hestia.toml"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_pictures_prefer_full_images_over_baked_thumbnails() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let file = |relative: &str| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"image").unwrap();
            path
        };
        let thumbnail = file(&format!("{MOD_META_DIR}/card_thumb_v2.png"));
        let preview = file("preview.bin");
        let screenshot = file(&format!("{DISABLED_CONTAINER}/shot.png"));
        let cover = file("cover.png");
        let screenshots = ["gone.png".to_owned(), "shot.png".to_owned()];
        let pick = |cover: Option<&str>, screenshots: &[String], preview: Option<&Path>| {
            mod_image(root, cover, screenshots, preview.map(Path::to_path_buf))
        };
        assert_eq!(
            pick(Some("cover.png"), &screenshots, Some(&preview)),
            Some(cover)
        );
        // A disabled mod's files move into the disabled folder.
        assert_eq!(
            pick(Some("missing.png"), &screenshots, Some(&preview)),
            Some(screenshot)
        );
        assert_eq!(pick(None, &[], Some(&preview)), Some(preview));
        assert_eq!(
            pick(Some("https://example.com/a.png"), &[], None),
            Some(thumbnail.clone())
        );
        assert_eq!(
            pick(None, &[], Some(&root.join("evicted.bin"))),
            Some(thumbnail)
        );
    }

    #[test]
    fn a_saved_sample_loads_as_it_was() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample").join(SAMPLE_FILE);
        let start = Start {
            host_pid: 1,
            host_window: None,
            game_pids: Vec::new(),
            library: crate::overlay_protocol::Library {
                game_id: "endfield".into(),
                game_name: "Arknights Endfield".into(),
                categories: Vec::new(),
            },
            settings: Settings {
                language: crate::model::AppLanguage::Indonesian,
                opacity: Some(60),
                ..Default::default()
            },
            selection: None,
        };
        write_sample(&path, &start).unwrap();
        assert_eq!(read_sample(&path).unwrap(), start);
    }
}
