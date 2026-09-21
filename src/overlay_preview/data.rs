//! Read-only snapshot of the installed library for the disposable overlay preview.

use std::path::{Path, PathBuf};

use crate::{
    model::{DISABLED_CONTAINER, MOD_META_DIR, MOD_META_FILE},
    persistence,
};

#[derive(Clone)]
pub(super) struct Catalog {
    pub game: String,
    pub categories: Vec<Category>,
    pub note: Option<String>,
}

#[derive(Clone)]
pub(super) struct Category {
    pub name: String,
    pub image: Option<PathBuf>,
    pub costumes: Vec<Costume>,
}

#[derive(Clone)]
pub(super) struct Costume {
    pub name: String,
    pub image: Option<PathBuf>,
    pub active: bool,
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
    let mut definitions: Vec<_> = state
        .categories
        .iter()
        .filter(|category| category.game_id == game.definition.id)
        .cloned()
        .collect();
    definitions.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
    let mut categories: Vec<_> = definitions
        .iter()
        .map(|category| Category {
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
                "Uncategorized"
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
        let image = user
            .cover_image
            .as_deref()
            .and_then(|image| resolve_image(path, image))
            .or_else(|| {
                user.screenshots
                    .iter()
                    .find_map(|image| resolve_image(path, image))
            })
            .or_else(|| {
                [
                    "card_thumb_v2.png",
                    "card_thumb.png",
                    "rail_thumb.png",
                    "icon_thumb.png",
                ]
                .into_iter()
                .map(|name| path.join(MOD_META_DIR).join(name))
                .find(|path| path.is_file())
            });
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
            name,
            image,
            active,
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

fn resolve_image(root: &Path, value: &str) -> Option<PathBuf> {
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
