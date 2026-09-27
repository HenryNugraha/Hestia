//! Read-only snapshot of the installed library for the disposable overlay preview.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    model::{DISABLED_CONTAINER, MOD_META_DIR, MOD_META_FILE, ModCategory, ModCategorySortMode},
    persistence,
};

const UNCATEGORIZED: &str = "Uncategorized";

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
                costumes: category
                    .mods
                    .into_iter()
                    .map(|item| Costume {
                        id: item.id,
                        name: item.name,
                        image: item.image,
                        active: item.active,
                        censored: item.censored,
                    })
                    .collect(),
            })
            .collect(),
        note: None,
    }
}

pub(super) fn load_catalog() -> Catalog {
    match read_catalog() {
        Ok(catalog) => catalog,
        Err(error) => Catalog {
            game: "Mods".into(),
            categories: Vec::new(),
            note: Some(format!("Could not load the local library: {error:#}")),
        },
    }
}

fn read_catalog() -> anyhow::Result<Catalog> {
    let state_path = state_path().ok_or_else(|| {
        anyhow::anyhow!(
            "No Hestia preferences found. Set HESTIA_OVERLAY_PREVIEW_STATE to your hestia.toml."
        )
    })?;
    // Construct paths explicitly: do not run startup, writable-directory probes,
    // migrations on disk, library scanning workers, or history database setup.
    let state = persistence::load_app_state(&persistence::PortablePaths {
        history_db: state_path.with_extension("dat"),
        state_archive: state_path,
        state_source: None,
    })?;
    let requested_game = std::env::var("HESTIA_OVERLAY_PREVIEW_GAME")
        .ok()
        .or_else(|| state.last_selected_game_id.clone());
    let game = state
        .games
        .iter()
        .find(|game| requested_game.as_deref() == Some(game.definition.id.as_str()))
        .or_else(|| {
            state.games.iter().find(|game| {
                game.enabled
                    && game
                        .mods_path(state.static_prefs.use_default_mods_path)
                        .is_some_and(|path| path.is_dir())
            })
        })
        .ok_or_else(|| anyhow::anyhow!("No configured game with installed mods"))?;
    anyhow::ensure!(
        game.is_xxmi(),
        "This overlay preview currently reads XXMI libraries"
    );
    let root = game
        .mods_path(state.static_prefs.use_default_mods_path)
        .filter(|path| path.is_dir())
        .ok_or_else(|| anyhow::anyhow!("The selected game's mod folder is unavailable"))?;
    let definitions: Vec<_> = state
        .categories
        .iter()
        .filter(|category| category.game_id == game.definition.id)
        .cloned()
        .collect();
    let mut categories: Vec<_> = definitions
        .iter()
        .map(|category| Category {
            id: category.id.clone(),
            name: category.name.clone(),
            image: None,
            costumes: Vec::new(),
        })
        .collect();
    let mut source_categories: Vec<Vec<crate::model::GameBananaSnapshot>> =
        vec![Vec::new(); categories.len()];
    let mut walk = walkdir::WalkDir::new(&root)
        .max_depth(6)
        .follow_links(false)
        .into_iter();
    let mut skipped = 0;
    let mut grouped_for_preview = false;
    let mut member_counts: HashMap<String, usize> = HashMap::new();
    while let Some(entry) = walk.next() {
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        if entry.file_name() == MOD_META_DIR || entry.file_name() == DISABLED_CONTAINER {
            walk.skip_current_dir();
            continue;
        }
        if !path.join(MOD_META_DIR).join(MOD_META_FILE).is_file() {
            continue;
        }
        walk.skip_current_dir();
        let metadata = match persistence::load_portable_mod_state(path) {
            Ok(Some(metadata)) => metadata,
            _ => {
                skipped += 1;
                continue;
            }
        };
        let user = &metadata.metadata.user;
        if let Some(id) = &user.category_id {
            *member_counts.entry(id.clone()).or_default() += 1;
        }
        let name = user
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
        let category_index = user
            .category_id
            .as_ref()
            .and_then(|id| definitions.iter().position(|category| &category.id == id))
            .or_else(|| {
                categories
                    .iter()
                    .position(|category| category.name == user.category)
            });
        let category_index = category_index.unwrap_or_else(|| {
            let inferred = (game.definition.id == "endfield" && user.category.trim().is_empty())
                .then(|| preview_character(&name))
                .flatten();
            grouped_for_preview |= inferred.is_some();
            let group = if let Some(character) = inferred {
                character
            } else if user.category.trim().is_empty() {
                UNCATEGORIZED
            } else {
                &user.category
            };
            categories
                .iter()
                .position(|category| {
                    category
                        .name
                        .strip_prefix("Operators: ")
                        .unwrap_or(&category.name)
                        .eq_ignore_ascii_case(group)
                })
                .unwrap_or_else(|| {
                    categories.push(Category {
                        id: format!("preview-group:{}", group.to_lowercase()),
                        name: group.into(),
                        image: None,
                        costumes: Vec::new(),
                    });
                    categories.len() - 1
                })
        });
        let active = if !path.join(DISABLED_CONTAINER).exists() {
            true
        } else {
            std::fs::read_dir(path)?
                .filter_map(Result::ok)
                .any(|child| {
                    child.file_name() != MOD_META_DIR && child.file_name() != DISABLED_CONTAINER
                })
        };
        let image = mod_image(path, user.cover_image.as_deref(), &user.screenshots, None);
        let category = &mut categories[category_index];
        if source_categories.len() <= category_index {
            source_categories.resize_with(category_index + 1, Vec::new);
        }
        if let Some(snapshot) = metadata
            .source
            .as_ref()
            .and_then(|source| source.snapshot.as_ref())
        {
            source_categories[category_index].push(crate::model::GameBananaSnapshot {
                category: snapshot.category.clone(),
                super_category_id: snapshot.super_category_id,
                ..Default::default()
            });
        }
        if category.image.is_none() || (active && image.is_some()) {
            category.image = image.clone();
        }
        category.costumes.push(Costume {
            id: path.to_string_lossy().into_owned(),
            name,
            image,
            active,
            censored: false,
        });
    }
    for category in &mut categories {
        category
            .costumes
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }
    let character_links: Vec<_> = categories
        .iter()
        .enumerate()
        .map(|(index, _)| {
            definitions.get(index).and_then(|category| {
                category.resolved_gamebanana_character(
                    source_categories[index].iter(),
                    crate::integrations::gamebanana::character_super_category_id_for_hestia(
                        &game.definition.id,
                    ),
                )
            })
        })
        .collect();
    apply_character_portraits(&mut categories, &game.definition.id, &character_links);
    let sort_mode = state
        .category_sort_mode_by_game
        .get(&game.definition.id)
        .copied()
        .unwrap_or_default();
    if matches!(
        sort_mode,
        ModCategorySortMode::ByModCountAsc | ModCategorySortMode::ByModCountDesc
    ) {
        // The library's counts include archived mods.
        if let Ok(archive) = crate::integrations::xxmi::archived_mods_root(
            game,
            state.static_prefs.use_default_mods_path,
        ) {
            count_archived_members(&archive, &mut member_counts);
        }
    }
    categories = library_order(categories, &definitions, sort_mode, &member_counts);
    if !state.static_prefs.library_show_empty_category_folders {
        categories.retain(|category| !category.costumes.is_empty());
    }
    let mut notes = Vec::new();
    if grouped_for_preview {
        notes.push(
            "Uncategorized costumes use temporary character groups in this preview.".to_owned(),
        );
    }
    if skipped > 0 {
        notes.push(format!("{skipped} unreadable entries were skipped."));
    }
    Ok(Catalog {
        game: game.definition.name.clone(),
        categories,
        note: (!notes.is_empty()).then(|| notes.join(" ")),
    })
}

/// Orders the rail the way the main window orders its folders: the categories
/// in the library's sort, then the groups this preview adds for mods the
/// library leaves uncategorized, A to Z, with the catch-all last.
fn library_order(
    mut categories: Vec<Category>,
    definitions: &[ModCategory],
    mode: ModCategorySortMode,
    member_counts: &HashMap<String, usize>,
) -> Vec<Category> {
    let mut groups = categories.split_off(definitions.len());
    groups.sort_by_cached_key(|group| (group.name == UNCATEGORIZED, group.name.to_lowercase()));
    let mut sorted = definitions.to_vec();
    crate::app::sort_categories_with_counts(&mut sorted, mode, |id| {
        member_counts.get(id).copied().unwrap_or_default()
    });
    let mut folders: Vec<_> = definitions.iter().zip(categories).collect();
    folders.sort_by_cached_key(|(definition, _)| {
        sorted
            .iter()
            .position(|category| category.id == definition.id)
    });
    folders
        .into_iter()
        .map(|(_, category)| category)
        .chain(groups)
        .collect()
}

fn count_archived_members(root: &Path, member_counts: &mut HashMap<String, usize>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        if let Ok(Some(state)) = persistence::load_portable_mod_state(&entry.path())
            && let Some(id) = state.metadata.user.category_id
        {
            *member_counts.entry(id).or_default() += 1;
        }
    }
}

// Preview fixture grouping for the clearly named, uncategorized costumes in the
// local Endfield library. This never assigns or persists production categories.
fn preview_character(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    ["Ardelia", "Fluorite", "Liino", "TangTang", "Typhoeus"]
        .into_iter()
        .find(|name| {
            let prefix = name.to_lowercase();
            lower
                .strip_prefix(&prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '_', '-']))
        })
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

/// Use the preview's downloaded category icons without touching Hestia's cache or state.
/// Startup stays offline; absent icons simply retain the existing local-cover fallback.
fn apply_character_portraits(
    categories: &mut [Category],
    game_id: &str,
    character_links: &[Option<crate::model::GameBananaCategoryLink>],
) {
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("HESTIA_OVERLAY_PREVIEW_PORTRAITS") {
        roots.push(PathBuf::from(root));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.join("target/overlay-preview/portraits"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(target) = exe.parent().and_then(Path::parent) {
            roots.push(target.join("overlay-preview/portraits"));
        }
    }
    for root in roots {
        let directory = root.join(game_id);
        let Some(manifest) = std::fs::read(directory.join("manifest.json"))
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<std::collections::BTreeMap<String, PortraitManifestEntry>>(
                    &bytes,
                )
                .ok()
            })
        else {
            continue;
        };
        for (index, category) in categories.iter_mut().enumerate() {
            let link = character_links.get(index).and_then(Option::as_ref);
            let name = link.map(|link| link.name.as_str()).unwrap_or_else(|| {
                category
                    .name
                    .strip_prefix("Operators: ")
                    .unwrap_or(&category.name)
            });
            if let Some((_, entry)) = manifest.iter().find(|(key, entry)| {
                link.is_some_and(|link| entry.files().0 == format!("category_{}.webp", link.id))
                    || key.eq_ignore_ascii_case(name)
            }) {
                if let Some(portrait) = resolve_portrait(&directory, entry) {
                    category.image = Some(portrait);
                }
            }
        }
        break;
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum PortraitManifestEntry {
    File(String),
    WithHighResolution {
        file: String,
        high_res: Option<String>,
    },
}

impl PortraitManifestEntry {
    fn files(&self) -> (&str, Option<&str>) {
        match self {
            Self::File(file) => (file, None),
            Self::WithHighResolution { file, high_res } => (file, high_res.as_deref()),
        }
    }
}

// High-resolution replacements are opt-in manifest data. Never probe arbitrary
// sibling names: a same-stem costume or unrelated image must not replace the
// category's source sprite by accident.
fn resolve_portrait(directory: &Path, entry: &PortraitManifestEntry) -> Option<PathBuf> {
    let (file, high_res) = entry.files();
    let source = directory.join(file);
    let source_dimensions = image::image_dimensions(&source).ok();
    let high_res = high_res
        .map(|file| directory.join(file))
        .filter(|path| path.is_file())
        .filter(|path| {
            let Some((width, height)) = image::image_dimensions(path).ok() else {
                return false;
            };
            let Some((source_width, source_height)) = source_dimensions else {
                return true;
            };
            u64::from(width) * u64::from(height)
                > u64::from(source_width) * u64::from(source_height)
        });
    high_res.or_else(|| source.is_file().then_some(source))
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

    fn definition(id: &str, name: &str, order: i32) -> ModCategory {
        ModCategory {
            id: id.into(),
            game_id: "endfield".into(),
            name: name.into(),
            order,
            gamebanana_character: None,
        }
    }

    fn category(name: &str) -> Category {
        Category {
            id: name.into(),
            name: name.into(),
            ..Category::default()
        }
    }

    fn names(categories: &[Category]) -> Vec<&str> {
        categories
            .iter()
            .map(|category| category.name.as_str())
            .collect()
    }

    #[test]
    fn categories_follow_the_library_sort_and_preview_groups_come_after() {
        let definitions = [
            definition("misc", "Other/Misc", 0),
            definition("endministrator", "Operators: Endministrator (F)", 1),
            definition("akekuri", "Operators: Akekuri", 2),
        ];
        let categories = [
            "Other/Misc",
            "Operators: Endministrator (F)",
            "Operators: Akekuri",
            UNCATEGORIZED,
            "Typhoeus",
            "Ardelia",
            "fan art",
        ]
        .map(category)
        .into();
        let ordered = library_order(
            categories,
            &definitions,
            ModCategorySortMode::ByNameAsc,
            &HashMap::new(),
        );
        assert_eq!(
            names(&ordered),
            [
                "Operators: Akekuri",
                "Operators: Endministrator (F)",
                "Other/Misc",
                "Ardelia",
                "fan art",
                "Typhoeus",
                UNCATEGORIZED,
            ]
        );
    }

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
    fn manual_sort_uses_the_library_order() {
        let definitions = [
            definition("c", "Chen", 2),
            definition("a", "Perlica", 0),
            definition("b", "Ember", 1),
        ];
        let categories = ["Chen", "Perlica", "Ember"].map(category).into();
        let ordered = library_order(
            categories,
            &definitions,
            ModCategorySortMode::Manual,
            &HashMap::new(),
        );
        assert_eq!(names(&ordered), ["Perlica", "Ember", "Chen"]);
    }

    #[test]
    fn mod_count_sort_uses_the_counted_members() {
        let definitions = [
            definition("empty", "Arclight", 0),
            definition("one", "Chen", 1),
            definition("three", "Perlica", 2),
            definition("also-three", "Ember", 3),
        ];
        let categories = ["Arclight", "Chen", "Perlica", "Ember"]
            .map(category)
            .into();
        let counts = HashMap::from([
            ("one".to_owned(), 1),
            ("three".to_owned(), 3),
            ("also-three".to_owned(), 3),
        ]);
        let ordered = library_order(
            categories,
            &definitions,
            ModCategorySortMode::ByModCountDesc,
            &counts,
        );
        assert_eq!(names(&ordered), ["Ember", "Perlica", "Chen", "Arclight"]);
    }
}
