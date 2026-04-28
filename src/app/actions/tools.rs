#[derive(Clone)]
struct DiscoveredGameTool {
    label: String,
    path: PathBuf,
    source_mod_id: Option<String>,
}

impl HestiaApp {
    fn ensure_tool_icon_texture(&mut self, ctx: &egui::Context, tool: &ToolEntry) {
        if self.tool_icon_textures.contains_key(&tool.id) || !tool.path.is_file() {
            return;
        }
        let Some(image) = load_exe_icon_color_image(&tool.path, TOOL_ICON_TEXTURE_SIZE) else {
            return;
        };
        let texture = ctx.load_texture(
            format!("tool-icon-{}", tool.id),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.tool_icon_textures.insert(tool.id.clone(), texture);
    }

    fn tool_icon_texture(&self, tool_id: &str) -> Option<&egui::TextureHandle> {
        self.tool_icon_textures.get(tool_id)
    }

    fn normalize_tool_path_key(path: &Path) -> String {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    }

    fn selected_game_tools(&self) -> Vec<ToolEntry> {
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            return Vec::new();
        };
        let mut items: Vec<_> = self
            .state
            .tools
            .iter()
            .filter(|tool| tool.game_id == game_id)
            .cloned()
            .collect();
        items.sort_by(|a, b| {
            a.window_order
                .cmp(&b.window_order)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.label.to_ascii_lowercase().cmp(&b.label.to_ascii_lowercase()))
        });
        items
    }

    fn selected_game_pinned_tools(&self) -> Vec<ToolEntry> {
        let mut items: Vec<_> = self
            .selected_game_tools()
            .into_iter()
            .filter(|tool| tool.show_in_titlebar)
            .collect();
        items.sort_by(|a, b| {
            a.titlebar_order
                .unwrap_or(i32::MAX / 4)
                .cmp(&b.titlebar_order.unwrap_or(i32::MAX / 4))
                .then_with(|| a.window_order.cmp(&b.window_order))
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
        items.truncate(4);
        items
    }

    fn compact_tool_window_order_for_game(&mut self, game_id: &str) {
        let mut ids: Vec<String> = self
            .state
            .tools
            .iter()
            .filter(|tool| tool.game_id == game_id)
            .map(|tool| tool.id.clone())
            .collect();
        ids.sort_by(|a, b| {
            let left = self.state.tools.iter().find(|tool| tool.id == *a);
            let right = self.state.tools.iter().find(|tool| tool.id == *b);
            match (left, right) {
                (Some(left), Some(right)) => left
                    .window_order
                    .cmp(&right.window_order)
                    .then_with(|| left.created_at.cmp(&right.created_at))
                    .then_with(|| left.label.to_ascii_lowercase().cmp(&right.label.to_ascii_lowercase())),
                _ => a.cmp(b),
            }
        });
        for (index, id) in ids.iter().enumerate() {
            if let Some(tool) = self.state.tools.iter_mut().find(|tool| tool.id == *id) {
                tool.window_order = index as i32;
            }
        }
    }

    fn compact_tool_titlebar_order_for_game(&mut self, game_id: &str) {
        let mut ids: Vec<String> = self
            .state
            .tools
            .iter()
            .filter(|tool| tool.game_id == game_id && tool.show_in_titlebar)
            .map(|tool| tool.id.clone())
            .collect();
        ids.sort_by(|a, b| {
            let left = self.state.tools.iter().find(|tool| tool.id == *a);
            let right = self.state.tools.iter().find(|tool| tool.id == *b);
            match (left, right) {
                (Some(left), Some(right)) => left
                    .titlebar_order
                    .unwrap_or(i32::MAX / 4)
                    .cmp(&right.titlebar_order.unwrap_or(i32::MAX / 4))
                    .then_with(|| left.window_order.cmp(&right.window_order))
                    .then_with(|| left.created_at.cmp(&right.created_at)),
                _ => a.cmp(b),
            }
        });
        for tool in self.state.tools.iter_mut().filter(|tool| tool.game_id == game_id) {
            tool.titlebar_order = None;
        }
        for (index, id) in ids.iter().enumerate() {
            if let Some(tool) = self.state.tools.iter_mut().find(|tool| tool.id == *id) {
                tool.titlebar_order = Some(index as i32);
            }
        }
    }

    fn next_window_order_for_game(&self, game_id: &str) -> i32 {
        self.state
            .tools
            .iter()
            .filter(|tool| tool.game_id == game_id)
            .map(|tool| tool.window_order)
            .max()
            .map_or(0, |value| value.saturating_add(1))
    }

    fn move_tool_window_order_to_slot(&mut self, tool_id: &str, slot_index: usize) -> bool {
        let Some(game_id) = self
            .state
            .tools
            .iter()
            .find(|tool| tool.id == tool_id)
            .map(|tool| tool.game_id.clone())
        else {
            return false;
        };
        let mut ordered_ids: Vec<String> = self
            .selected_game_tools()
            .into_iter()
            .map(|tool| tool.id)
            .collect();
        let Some(current_index) = ordered_ids.iter().position(|id| id == tool_id) else {
            return false;
        };
        let slot_index = slot_index.min(ordered_ids.len());
        let adjusted_index = if slot_index > current_index {
            slot_index.saturating_sub(1)
        } else {
            slot_index
        };
        if current_index == adjusted_index {
            return false;
        }
        let moving = ordered_ids.remove(current_index);
        ordered_ids.insert(adjusted_index, moving);
        for (index, id) in ordered_ids.iter().enumerate() {
            if let Some(tool) = self.state.tools.iter_mut().find(|tool| tool.id == *id) {
                tool.window_order = index as i32;
            }
        }
        self.compact_tool_window_order_for_game(&game_id);
        true
    }

    fn move_tool_titlebar_order_to_slot(&mut self, tool_id: &str, slot_index: usize) -> bool {
        let Some(game_id) = self
            .state
            .tools
            .iter()
            .find(|tool| tool.id == tool_id)
            .map(|tool| tool.game_id.clone())
        else {
            return false;
        };
        let mut ordered_ids: Vec<String> = self
            .selected_game_pinned_tools()
            .into_iter()
            .map(|tool| tool.id)
            .collect();
        let Some(current_index) = ordered_ids.iter().position(|id| id == tool_id) else {
            return false;
        };
        let slot_index = slot_index.min(ordered_ids.len());
        let adjusted_index = if slot_index > current_index {
            slot_index.saturating_sub(1)
        } else {
            slot_index
        };
        if current_index == adjusted_index {
            return false;
        }
        let moving = ordered_ids.remove(current_index);
        ordered_ids.insert(adjusted_index, moving);
        for tool in self.state.tools.iter_mut().filter(|tool| tool.game_id == game_id) {
            if tool.show_in_titlebar {
                tool.titlebar_order = None;
            }
        }
        for (index, id) in ordered_ids.iter().enumerate() {
            if let Some(tool) = self.state.tools.iter_mut().find(|tool| tool.id == *id) {
                tool.titlebar_order = Some(index as i32);
            }
        }
        self.compact_tool_titlebar_order_for_game(&game_id);
        true
    }

    fn infer_tool_source_mod_id(&self, game_id: &str, path: &Path) -> Option<String> {
        let mut best: Option<(&ModEntry, usize)> = None;
        for mod_entry in self.state.mods.iter().filter(|item| item.game_id == game_id) {
            if !path.starts_with(&mod_entry.root_path) {
                continue;
            }
            let depth = mod_entry.root_path.components().count();
            let should_replace = best
                .as_ref()
                .map(|(_, best_depth)| depth > *best_depth)
                .unwrap_or(true);
            if should_replace {
                best = Some((mod_entry, depth));
            }
        }
        best.map(|(mod_entry, _)| mod_entry.id.clone())
    }

    fn discover_tools_for_game(&self, game_id: &str, mods_root: &Path) -> Vec<DiscoveredGameTool> {
        let mut by_key: HashMap<String, DiscoveredGameTool> = HashMap::new();
        for entry in WalkDir::new(mods_root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let is_exe = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
            if !is_exe {
                continue;
            }
            let key = Self::normalize_tool_path_key(path);
            if self
                .state
                .tool_blacklist
                .get(game_id)
                .is_some_and(|items| items.iter().any(|item| item == &key))
            {
                continue;
            }
            let label = path
                .file_stem()
                .or_else(|| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("Tool")
                .to_string();
            by_key.insert(
                key,
                DiscoveredGameTool {
                    label,
                    path: path.to_path_buf(),
                    source_mod_id: self.infer_tool_source_mod_id(game_id, path),
                },
            );
        }
        let mut items: Vec<_> = by_key.into_values().collect();
        items.sort_by(|a, b| a.label.to_ascii_lowercase().cmp(&b.label.to_ascii_lowercase()));
        items
    }

    fn sync_tools_for_selected_game(&mut self) -> bool {
        let Some(game) = self.selected_game().cloned() else {
            return false;
        };
        let Some(mods_root) = game.mods_path(self.state.use_default_mods_path) else {
            return false;
        };
        if !mods_root.is_dir() {
            return false;
        }
        let game_id = game.definition.id;
        let discovered = self.discover_tools_for_game(&game_id, &mods_root);
        self.merge_discovered_tools(&game_id, discovered)
    }

    fn merge_discovered_tools(&mut self, game_id: &str, discovered: Vec<DiscoveredGameTool>) -> bool {
        let manual_keys: HashSet<String> = self
            .state
            .tools
            .iter()
            .filter(|tool| tool.game_id == game_id && !tool.auto_detected)
            .map(|tool| Self::normalize_tool_path_key(&tool.path))
            .collect();
        let discovered_by_key: HashMap<String, DiscoveredGameTool> = discovered
            .into_iter()
            .map(|tool| (Self::normalize_tool_path_key(&tool.path), tool))
            .collect();
        let mut changed = false;

        self.state.tools.retain_mut(|tool| {
            if tool.game_id != game_id {
                return true;
            }
            if !tool.auto_detected {
                return true;
            }
            let key = Self::normalize_tool_path_key(&tool.path);
            if manual_keys.contains(&key) {
                changed = true;
                return false;
            }
            if let Some(discovered) = discovered_by_key.get(&key) {
                if tool.label != discovered.label
                    || tool.path != discovered.path
                    || tool.source_mod_id != discovered.source_mod_id
                {
                    if tool.path != discovered.path {
                        self.tool_icon_textures.remove(&tool.id);
                    }
                    tool.label = discovered.label.clone();
                    tool.path = discovered.path.clone();
                    tool.source_mod_id = discovered.source_mod_id.clone();
                    changed = true;
                }
                true
            } else {
                changed = true;
                false
            }
        });

        let existing_keys: HashSet<String> = self
            .state
            .tools
            .iter()
            .filter(|tool| tool.game_id == game_id)
            .map(|tool| Self::normalize_tool_path_key(&tool.path))
            .collect();

        for (key, discovered) in discovered_by_key {
            if existing_keys.contains(&key) {
                continue;
            }
            self.state.tools.push(ToolEntry {
                id: Uuid::new_v4().to_string(),
                game_id: game_id.to_string(),
                label: discovered.label,
                path: discovered.path,
                launch_args: String::new(),
                source_mod_id: discovered.source_mod_id,
                auto_detected: true,
                show_in_titlebar: false,
                window_order: self.next_window_order_for_game(game_id),
                titlebar_order: None,
                created_at: Utc::now(),
            });
            changed = true;
        }

        if changed {
            self.compact_tool_window_order_for_game(game_id);
            self.compact_tool_titlebar_order_for_game(game_id);
        }

        changed
    }

    fn prompt_add_tool_for_selected_game(&mut self) {
        let Some(path) = FileDialog::new().pick_file() else {
            return;
        };
        self.add_manual_tool_for_selected_game(path);
    }

    fn add_manual_tool_for_selected_game(&mut self, path: PathBuf) {
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            self.report_warn("No game selected for tool add", Some("No game selected"));
            return;
        };
        let key = Self::normalize_tool_path_key(&path);
        if self
            .state
            .tools
            .iter()
            .any(|tool| tool.game_id == game_id && Self::normalize_tool_path_key(&tool.path) == key)
        {
            self.report_warn("Tool already exists for this game", Some("Tool already added"));
            return;
        }
        if let Some(items) = self.state.tool_blacklist.get_mut(&game_id) {
            items.retain(|item| item != &key);
            if items.is_empty() {
                self.state.tool_blacklist.remove(&game_id);
            }
        }
        let label = path
            .file_stem()
            .or_else(|| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Tool")
            .to_string();
        self.state.tools.push(ToolEntry {
            id: Uuid::new_v4().to_string(),
            game_id: game_id.clone(),
            label: label.clone(),
            path,
            launch_args: String::new(),
            source_mod_id: None,
            auto_detected: false,
            show_in_titlebar: false,
            window_order: self.next_window_order_for_game(&game_id),
            titlebar_order: None,
            created_at: Utc::now(),
        });
        self.save_state();
        self.log_action("Tool Added", &label);
        self.set_message_ok("Tool added");
    }

    fn remove_tool(&mut self, tool_id: &str) {
        let Some(index) = self.state.tools.iter().position(|tool| tool.id == tool_id) else {
            return;
        };
        let tool = self.state.tools.remove(index);
        self.tool_icon_textures.remove(&tool.id);
        if tool.auto_detected {
            let key = Self::normalize_tool_path_key(&tool.path);
            let items = self.state.tool_blacklist.entry(tool.game_id.clone()).or_default();
            if !items.iter().any(|item| item == &key) {
                items.push(key);
                items.sort();
                items.dedup();
            }
        }
        self.compact_tool_window_order_for_game(&tool.game_id);
        self.compact_tool_titlebar_order_for_game(&tool.game_id);
        self.save_state();
        self.log_action("Tool Removed", &tool.label);
        self.set_message_ok("Tool removed");
    }

    fn toggle_tool_titlebar_pin(&mut self, tool_id: &str, value: bool) {
        let Some(index) = self.state.tools.iter().position(|tool| tool.id == tool_id) else {
            return;
        };
        let game_id = self.state.tools[index].game_id.clone();
        if value
            && !self.state.tools[index].show_in_titlebar
            && self
                .state
                .tools
                .iter()
                .filter(|tool| tool.game_id == game_id && tool.show_in_titlebar)
                .count()
                >= 4
        {
            self.report_warn(
                "Only up to 4 tools can be shown in the titlebar for one game",
                Some("Titlebar tool limit reached"),
            );
            return;
        }
        if value {
            for tool in self
                .state
                .tools
                .iter_mut()
                .filter(|tool| tool.game_id == game_id && tool.show_in_titlebar)
            {
                tool.titlebar_order = Some(tool.titlebar_order.unwrap_or(0).saturating_add(1));
            }
            self.state.tools[index].show_in_titlebar = true;
            self.state.tools[index].titlebar_order = Some(0);
        } else {
            self.state.tools[index].show_in_titlebar = false;
            self.state.tools[index].titlebar_order = None;
        }
        self.compact_tool_titlebar_order_for_game(&game_id);
        self.save_state();
    }

    fn launch_tool(&mut self, ctx: &egui::Context, tool_id: &str) {
        let Some(tool) = self.state.tools.iter().find(|tool| tool.id == tool_id).cloned() else {
            return;
        };
        if !tool.path.is_file() {
            self.report_warn(
                format!("Tool not found: {}", tool.path.display()),
                Some("Tool executable is missing"),
            );
            return;
        }
        match xxmi::launch_path_with_raw_args(&tool.path, &tool.launch_args) {
            Ok(_) => {
                self.log_action("Tool Launched", &tool.label);
                self.set_message_ok(format!("Launched tool: {}", tool.label));
                Self::apply_launch_behavior(ctx, self.state.tool_launch_behavior);
            }
            Err(err) => self.report_error_message(
                format!("failed to launch tool {}: {err}", tool.path.display()),
                Some("Could not launch tool"),
            ),
        }
    }

    fn open_tool_location(&mut self, tool_id: &str) {
        let Some(tool) = self.state.tools.iter().find(|tool| tool.id == tool_id).cloned() else {
            return;
        };
        let target = if tool.path.is_dir() {
            tool.path.clone()
        } else if let Some(parent) = tool.path.parent() {
            parent.to_path_buf()
        } else {
            tool.path.clone()
        };
        if let Err(err) = open_in_explorer(&target) {
            self.report_error_message(
                format!("failed to open tool location {}: {err:#}", target.display()),
                Some("Could not open location"),
            );
        }
    }

    fn toggle_tools_window(&mut self) {
        self.state.show_tools = !self.state.show_tools;
        if self.state.show_tools {
            self.tools_window_nonce = self.tools_window_nonce.wrapping_add(1);
            self.tools_force_default_pos = true;
        }
        self.save_state();
    }

    fn open_tool_launch_options_prompt(&mut self, tool_id: &str) {
        let Some(tool) = self.state.tools.iter().find(|tool| tool.id == tool_id) else {
            return;
        };
        self.tool_launch_options_prompt = Some(ToolLaunchOptionsPrompt {
            tool_id: tool.id.clone(),
            launch_args: tool.launch_args.clone(),
        });
    }

    fn save_tool_launch_options_prompt(&mut self) {
        let Some(prompt) = self.tool_launch_options_prompt.take() else {
            return;
        };
        let Some(tool) = self.state.tools.iter_mut().find(|tool| tool.id == prompt.tool_id) else {
            return;
        };
        tool.launch_args = prompt.launch_args;
        self.save_state();
        self.set_message_ok("Tool launch options saved");
    }
}
