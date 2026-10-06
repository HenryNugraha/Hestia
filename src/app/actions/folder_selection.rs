#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FolderContentsScope {
    #[default]
    Visible,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LibrarySelectionSection {
    #[default]
    Folders,
    Mods,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderBatchAction {
    Enable,
    Disable,
    Archive,
    Restore,
    CheckUpdates,
    Update,
    MoveContents,
    RemoveFolders,
    DeleteContents,
    DeleteFoldersAndContents,
}

/// An invocation captures its category and member ids once. UI selection changes
/// and newly installed members must not retarget an operation already in progress.
#[derive(Debug, Clone)]
struct FolderBatchTargets {
    game_id: String,
    category_ids: Vec<String>,
    mod_ids: Vec<String>,
    all_mod_ids: Vec<String>,
    scope: FolderContentsScope,
}

fn folder_member_ids(
    categories: &[ModCategory],
    mods: &[ModEntry],
    game_id: &str,
    category_ids: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    mods.iter()
        .filter(|entry| {
            entry.game_id == game_id
                && effective_category_id(categories, entry)
                    .is_some_and(|id| category_ids.contains(id))
        })
        .filter(|entry| seen.insert(entry.id.as_str()))
        .map(|entry| entry.id.clone())
        .collect()
}

fn build_folder_batch_targets(
    game_id: &str,
    categories: &[ModCategory],
    mods: &[ModEntry],
    selected: &HashSet<String>,
    visible_mod_ids: &HashSet<String>,
    scope: FolderContentsScope,
) -> Option<FolderBatchTargets> {
    let category_ids: Vec<_> = categories
        .iter()
        .filter(|category| category.game_id == game_id && selected.contains(&category.id))
        .map(|category| category.id.clone())
        .collect();
    if category_ids.is_empty() {
        return None;
    }
    let all_mod_ids = folder_member_ids(
        categories,
        mods,
        game_id,
        &category_ids.iter().cloned().collect(),
    );
    let mod_ids = all_mod_ids
        .iter()
        .filter(|id| scope == FolderContentsScope::All || visible_mod_ids.contains(*id))
        .cloned()
        .collect();
    Some(FolderBatchTargets {
        game_id: game_id.to_owned(),
        category_ids,
        mod_ids,
        all_mod_ids,
        scope,
    })
}

/// Checkbox/Ctrl toggles; Shift replaces a range and Ctrl+Shift adds one.
/// A missing or filtered-out anchor starts a new range at the clicked folder.
fn select_visible_folder_range(
    selected: &mut HashSet<String>,
    anchor: Option<&str>,
    current_id: &str,
    visible_ids: &[String],
    range: bool,
    additive: bool,
) -> bool {
    let Some(current) = visible_ids.iter().position(|id| id == current_id) else {
        return false;
    };
    if range {
        let start = anchor
            .and_then(|id| visible_ids.iter().position(|visible| visible == id))
            .unwrap_or(current);
        if !additive {
            selected.clear();
        }
        selected.extend(
            visible_ids[start.min(current)..=start.max(current)]
                .iter()
                .cloned(),
        );
    } else if !selected.remove(current_id) {
        selected.insert(current_id.to_owned());
    }
    true
}

impl HestiaApp {
    fn clear_library_folder_selection(&mut self) {
        self.selected_library_folder_ids.clear();
        self.library_folder_selection_anchor = None;
        self.library_folder_contents_scope = FolderContentsScope::Visible;
        if !self.dragging_library_folder_ids.is_empty() {
            self.dragging_category_id = None;
            self.dragging_category_target_index = None;
        }
        self.dragging_library_folder_ids.clear();
        self.folder_delete_menu_requested = false;
    }

    fn begin_library_mod_selection(&mut self) {
        self.clear_library_folder_selection();
        self.library_selection_section = LibrarySelectionSection::Mods;
    }

    fn begin_library_folder_selection(&mut self) {
        self.selected_mods.clear();
        self.set_selected_mod_id(None);
        self.dragging_mod_ids.clear();
        self.library_selection_section = LibrarySelectionSection::Folders;
    }

    fn library_folder_selection_active(&self) -> bool {
        !self.selected_library_folder_ids.is_empty()
            && self.current_view == ViewMode::Library
            && self.selected_category_folder_id.is_none()
            && self
                .state
                .static_prefs
                .effective_library_category_display_mode()
                == LibraryCategoryDisplayMode::Folders
    }

    fn select_library_folder(&mut self, id: &str, range: bool, additive: bool) {
        if !self
            .library_visible_folder_ids
            .iter()
            .any(|visible| visible == id)
        {
            return;
        }
        self.begin_library_folder_selection();
        let anchor = self.library_folder_selection_anchor.clone();
        if select_visible_folder_range(
            &mut self.selected_library_folder_ids,
            anchor.as_deref(),
            id,
            &self.library_visible_folder_ids,
            range,
            additive,
        ) {
            if !range || anchor.is_none() {
                self.library_folder_selection_anchor = Some(id.to_owned());
            }
            if self.selected_library_folder_ids.is_empty() {
                self.clear_library_folder_selection();
            }
        }
    }

    fn select_all_library_folders(&mut self) {
        self.begin_library_folder_selection();
        self.selected_library_folder_ids =
            self.library_visible_folder_ids.iter().cloned().collect();
        self.library_folder_selection_anchor = self.library_visible_folder_ids.first().cloned();
    }

    fn sync_library_folder_selection_context(&mut self) {
        if !self.selected_library_folder_ids.is_empty()
            && (!self.selected_mods.is_empty() || self.selected_mod_id.is_some())
        {
            self.begin_library_mod_selection();
        }
        let mut hasher = DefaultHasher::new();
        self.selected_game()
            .map(|game| &game.definition.id)
            .hash(&mut hasher);
        self.mods_search_query.hash(&mut hasher);
        self.show_enabled_mods.hash(&mut hasher);
        self.show_unlinked_mods.hash(&mut hasher);
        self.show_up_to_date_mods.hash(&mut hasher);
        self.show_update_available_mods.hash(&mut hasher);
        self.show_check_skipped_mods.hash(&mut hasher);
        self.show_missing_source_mods.hash(&mut hasher);
        self.show_modified_locally_mods.hash(&mut hasher);
        self.show_ignoring_update_mods.hash(&mut hasher);
        self.state.static_prefs.hide_disabled.hash(&mut hasher);
        self.state.static_prefs.hide_archived.hash(&mut hasher);
        self.state
            .static_prefs
            .library_show_empty_category_folders
            .hash(&mut hasher);
        Self::hash_discriminant_for_library_cache(
            &mut hasher,
            &self.state.static_prefs.unsafe_content_mode,
        );
        Self::hash_discriminant_for_library_cache(&mut hasher, &self.current_view);
        Self::hash_discriminant_for_library_cache(
            &mut hasher,
            &self.state.static_prefs.library_category_display_mode,
        );
        self.selected_category_folder_id.hash(&mut hasher);
        let context = hasher.finish();
        if self
            .library_folder_selection_context
            .is_some_and(|old| old != context)
        {
            self.clear_library_folder_selection();
        }
        self.library_folder_selection_context = Some(context);
        let overview = self.current_view == ViewMode::Library
            && self.selected_category_folder_id.is_none()
            && self
                .state
                .static_prefs
                .effective_library_category_display_mode()
                == LibraryCategoryDisplayMode::Folders;
        if !overview {
            self.clear_library_folder_selection();
            self.library_visible_folder_ids.clear();
            self.library_selection_section = LibrarySelectionSection::Mods;
            return;
        }
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            self.clear_library_folder_selection();
            self.library_visible_folder_ids.clear();
            return;
        };
        let visible_categories: HashSet<&str> = self
            .mods_for_selected_game()
            .into_iter()
            .filter_map(|entry| effective_category_id(&self.state.categories, entry))
            .collect();
        let visible_folder_ids: Vec<_> = self
            .categories_for_game(&game_id)
            .into_iter()
            .filter(|category| {
                visible_categories.contains(category.id.as_str())
                    || (self.mods_search_query.trim().is_empty()
                        && self.state.static_prefs.library_show_empty_category_folders)
                    || self
                        .category_rename_matches(&category.id, CategoryRenameSurface::LibraryFolder)
            })
            .map(|category| category.id)
            .collect();
        let mod_ids = self.library_visible_mod_ids.clone();
        self.update_library_selection_surface(&visible_folder_ids, &mod_ids);
        if !self.selected_mods.is_empty() || self.selected_mod_id.is_some() {
            self.library_selection_section = LibrarySelectionSection::Mods;
        }
    }

    fn update_library_selection_surface(&mut self, folder_ids: &[String], mod_ids: &[String]) {
        if self.library_visible_folder_ids != folder_ids {
            self.library_folder_selection_anchor = None;
        }
        self.library_visible_folder_ids = folder_ids.to_vec();
        self.library_visible_mod_ids = mod_ids.to_vec();
        let visible: HashSet<&str> = folder_ids.iter().map(String::as_str).collect();
        self.selected_library_folder_ids
            .retain(|id| visible.contains(id.as_str()));
        if self.selected_library_folder_ids.is_empty() {
            self.library_folder_contents_scope = FolderContentsScope::Visible;
            self.folder_delete_menu_requested = false;
        }
    }

    fn folder_batch_targets(&self) -> Option<FolderBatchTargets> {
        let game = self.selected_game()?;
        if !self.library_folder_selection_active() || !game.enabled {
            return None;
        }
        let visible_ids = self
            .mods_for_selected_game()
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        build_folder_batch_targets(
            &game.definition.id,
            &self.categories_for_game(&game.definition.id),
            &self.state.mods,
            &self.selected_library_folder_ids,
            &visible_ids,
            self.library_folder_contents_scope,
        )
    }

    fn folder_batch_show_counts(&self, targets: &FolderBatchTargets) -> bool {
        targets.scope != FolderContentsScope::All
            || self.state.static_prefs.unsafe_content_mode != UnsafeContentMode::HideNoCounter
    }

    fn folder_batch_scope_has_hidden_contents(&self, targets: &FolderBatchTargets) -> bool {
        let shown: HashSet<&str> = self
            .mods_for_selected_game()
            .into_iter()
            .map(|entry| entry.id.as_str())
            .collect();
        targets
            .all_mod_ids
            .iter()
            .any(|id| !shown.contains(id.as_str()))
    }
}

#[cfg(test)]
mod folder_selection_tests {
    use super::*;

    fn category(id: &str, game: &str, name: &str) -> ModCategory {
        ModCategory {
            id: id.into(),
            game_id: game.into(),
            name: name.into(),
            order: 0,
            gamebanana_character: None,
        }
    }

    fn entry(id: &str, game: &str, category_id: Option<&str>, legacy: &str) -> ModEntry {
        let mut metadata = crate::model::ModMetadata::default();
        metadata.user.category_id = category_id.map(str::to_owned);
        metadata.user.category = legacy.into();
        ModEntry {
            id: id.into(),
            game_id: game.into(),
            folder_name: id.into(),
            root_path: PathBuf::from(id),
            status: ModStatus::Active,
            metadata,
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: UnsafeContentPreference::Auto,
            source: None,
            update_state: ModUpdateState::Unlinked,
        }
    }

    #[test]
    fn folder_targets_include_legacy_members_and_exclude_other_games_and_stale_categories() {
        let categories = vec![
            category("a", "game", "Characters"),
            category("b", "other", "Characters"),
        ];
        let mods = vec![
            entry("direct", "game", Some("a"), ""),
            entry("legacy", "game", None, "characters"),
            entry("other", "other", Some("b"), "Characters"),
            entry("loose", "game", None, ""),
        ];
        let selected = HashSet::from(["a".into(), "b".into(), "deleted".into()]);
        let visible = HashSet::from(["direct".into(), "other".into()]);
        let targets = build_folder_batch_targets(
            "game",
            &categories,
            &mods,
            &selected,
            &visible,
            FolderContentsScope::Visible,
        )
        .unwrap();
        assert_eq!(targets.category_ids, vec!["a"]);
        assert_eq!(targets.mod_ids, vec!["direct"]);
        assert_eq!(targets.all_mod_ids, vec!["direct", "legacy"]);
        let all = build_folder_batch_targets(
            "game",
            &categories,
            &mods,
            &selected,
            &visible,
            FolderContentsScope::All,
        )
        .unwrap();
        assert_eq!(all.mod_ids, all.all_mod_ids);
    }

    #[test]
    fn folder_target_snapshot_does_not_add_later_installs_and_retains_empty_folders() {
        let categories = vec![
            category("a", "game", "A"),
            category("empty", "game", "Empty"),
        ];
        let mut mods = vec![entry("first", "game", Some("a"), "")];
        let selected = HashSet::from(["a".into(), "empty".into()]);
        let snapshot = build_folder_batch_targets(
            "game",
            &categories,
            &mods,
            &selected,
            &HashSet::new(),
            FolderContentsScope::All,
        )
        .unwrap();
        mods.push(entry("later", "game", Some("a"), ""));
        assert_eq!(snapshot.mod_ids, vec!["first"]);
        assert_eq!(snapshot.category_ids.len(), 2);
        assert_eq!(
            folder_member_ids(&categories, &mods, "game", &selected),
            vec!["first", "later"]
        );
    }

    #[test]
    fn folder_shift_range_follows_display_order_and_ctrl_shift_adds() {
        let visible: Vec<String> = ["z", "b", "a", "c"].map(str::to_owned).into();
        let mut selected = HashSet::from(["c".into()]);
        assert!(select_visible_folder_range(
            &mut selected,
            Some("z"),
            "a",
            &visible,
            true,
            false
        ));
        assert_eq!(
            selected,
            HashSet::from(["z".into(), "b".into(), "a".into()])
        );
        assert!(select_visible_folder_range(
            &mut selected,
            Some("a"),
            "c",
            &visible,
            true,
            true
        ));
        assert_eq!(selected.len(), 4);
        assert!(select_visible_folder_range(
            &mut selected,
            None,
            "b",
            &visible,
            false,
            true
        ));
        assert!(!selected.contains("b"));
    }

    #[test]
    fn stale_range_anchor_never_selects_hidden_folders() {
        let visible = vec!["shown".into(), "next".into()];
        let mut selected = HashSet::from(["next".into()]);
        assert!(select_visible_folder_range(
            &mut selected,
            Some("hidden"),
            "shown",
            &visible,
            true,
            false
        ));
        assert_eq!(selected, HashSet::from(["shown".into()]));
        assert!(!select_visible_folder_range(
            &mut selected,
            None,
            "hidden",
            &visible,
            false,
            false
        ));
        assert_eq!(selected, HashSet::from(["shown".into()]));
    }
}
