use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
};

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Label, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, Vec2,
};

mod card_keys;

use card_keys::{CardKeys, Face};

use super::data::{Catalog, Category, Costume};
use super::gamebanana::{self, GameBanana, InstallNews, ListKey, StatusCard};
use super::text;
use super::thumbnails::ThumbnailCache;
use crate::app::TextKey;
use crate::overlay_protocol::{
    BrowseMod, ChangeAction, FromOverlay, InstallStage, ModHotkeys, SameNameChoice, Selection,
    ToOverlay, UNCATEGORIZED_ID, names_character,
};

const OVERLAY_OPACITY_MIN: u8 = 50;
const OVERLAY_OPACITY_MAX: u8 = 94;
const CAROUSEL_HEIGHT: f32 = 266.0;
const FOCUSED_CARD_SIZE: Vec2 = Vec2::new(214.0, 246.0);
const NEIGHBOR_CARD_SIZE: Vec2 = Vec2::new(133.0, 153.0);
const CAROUSEL_CARD_GAP: f32 = 12.0;
const CATEGORY_STRIP_HEIGHT: f32 = 78.0;
const CATEGORY_ITEM_HEIGHT: f32 = 64.0;
const CATEGORY_ITEM_WIDTH: f32 = 64.0;
const CATEGORY_NAV_WIDTH: f32 = 40.0;
const CATEGORY_GAP: f32 = 6.0;
const ACCENT: Color32 = Color32::from_rgb(196, 91, 52);
const CAROUSEL_TRANSITION_SECS: f64 = 0.14;
/// How long an installed mod takes to cross from its GameBanana card to its
/// place among the category's own.
const CROSSING_SECS: f64 = 0.5;
const ACTIVE_FEEDBACK_SECS: f64 = 0.15;
/// A second click on the same card this soon is part of a double-click,
/// which would otherwise turn a mod on and straight back off.
const DOUBLE_CLICK_SECS: f64 = 0.35;
/// How fast a card's badge pulses while its change waits for Hestia.
const WAITING_PULSE_SPEED: f64 = 4.0;
const CATEGORY_SPRITE_SIZE: f32 = 30.0;
const CATEGORY_SELECTED_SPRITE_SIZE: f32 = 40.0;
const CARD_TITLE_BOTTOM_PADDING: f32 = 8.0;
/// The extra room between a category's own mods and its GameBanana mods,
/// which holds the divider.
const DIVIDER_ZONE: f32 = 30.0;
/// Selection ids of GameBanana cards start with this.
const GAMEBANANA_PREFIX: &str = "gamebanana:";
/// The selection id of the card after a GameBanana list.
const GAMEBANANA_END: &str = "gamebanana:end";
/// Ids of the categories "Show all characters" adds start with this.
const EXTRA_CATEGORY_PREFIX: &str = "gamebanana:character:";

// Keep one physical wheel notch from selecting several entries when egui exposes its
// smoothing tail over multiple frames.
const WHEEL_POINTS_PER_STEP: f32 = 50.0;
const WHEEL_IDLE_RESET_SECS: f64 = 0.35;
const WHEEL_STEP_COOLDOWN_SECS: f64 = 0.12;
const TOOLTIP_SUPPRESS_SECS: f64 = 0.50;
const REVEAL_INTERACTION_THRESHOLD: f32 = 0.92;
const REVEAL_OFFSET: f32 = 10.0;

fn tooltip_suppress_id() -> egui::Id {
    egui::Id::new("overlay-preview-tooltip-suppress-until")
}

/// Add a delayed tooltip unless a scroll/collapse has just dismissed transient UI.
/// egui still applies its normal still-pointer delay on top of this guard.
pub(super) fn delayed_tooltip(
    response: egui::Response,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let now = response.ctx.input(|input| input.time);
    let suppressed_until = response.ctx.data(|data| {
        data.get_temp::<f64>(tooltip_suppress_id())
            .unwrap_or(f64::NEG_INFINITY)
    });
    if now >= suppressed_until {
        response.on_hover_text(text)
    } else {
        response
    }
}

/// Suppress transient tooltips for a short settling period after a scroll/collapse.
pub(super) fn suppress_tooltips(ctx: &egui::Context) {
    let until = ctx.input(|input| input.time) + TOOLTIP_SUPPRESS_SECS;
    ctx.data_mut(|data| data.insert_temp(tooltip_suppress_id(), until));
}

#[derive(Clone)]
struct CarouselTransition {
    from: Vec<CarouselCardPlacement>,
    started_at: f64,
    seconds: f64,
}

#[derive(Clone, Copy)]
struct CarouselCardPlacement {
    index: usize,
    rect: Rect,
    focused: bool,
    /// Fixed visual slot: left, center, or right. This is carried through a
    /// transition instead of being inferred from an interpolated rectangle.
    slot: usize,
}

/// The card that won the pointer press. Keep its original rectangle until release so a
/// carousel transition cannot make a click activate a different card underneath the cursor.
#[derive(Clone)]
struct CarouselPointerPress {
    category_index: usize,
    costume_index: usize,
    rect: Rect,
    placements: Vec<CarouselCardPlacement>,
}

#[derive(Clone, Copy)]
struct HeldPreview {
    category_index: usize,
    costume_index: usize,
}

enum PendingCommand {
    Mod(i32),
    Category(i32),
    Action(ModAction),
    /// Turn the focused card over to its hotkeys, or back.
    Flip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModAction {
    Exclusive,
    Toggle,
}

/// A change to mods the overlay made on screen, for Hestia to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModRequest {
    pub mod_id: String,
    pub action: ChangeAction,
    /// The mods it turned on or off on screen.
    pub states: Vec<(String, bool)>,
}

/// The row that A/D and the side arrows move in. The overlay opens on the
/// categories row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Row {
    Mods,
    #[default]
    Categories,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ShortcutAvailability {
    pub categories: bool,
    pub mods: bool,
    pub exclusive: bool,
    pub toggle: bool,
    pub space: Space,
    /// The focused card has hotkeys to turn over to.
    pub hotkeys: bool,
}

/// What Space does on the focused card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Space {
    /// Turns the mod on and the rest of its category off.
    #[default]
    Exclusive,
    /// Uncategorized only turns the mod on.
    Enable,
    Install,
    TryAgain,
}

/// What a search shows.  A category whose name matches shows all its mods,
/// any other category shows only the mods that match, and categories with no
/// match are hidden.  An empty search shows everything.
///
/// A category linked to a GameBanana character shows the character's
/// GameBanana mods after its own, as cards numbered on from its own mods,
/// then a card for the list's state while it loads, failed or is empty.
/// GameBanana mods already installed are left out.
struct Filter {
    /// The search as typed.
    query: String,
    /// The search as `search_key` has it.
    needle: String,
    /// Visible categories, in catalog order.
    categories: Vec<usize>,
    /// Each category's visible cards, in order.  Indexed like the catalog,
    /// so a hidden category has an empty list.
    mods: Vec<Vec<usize>>,
}

impl Filter {
    fn new(catalog: &Catalog, query: &str, gamebanana: &GameBanana) -> Self {
        let needle = search_key(query);
        let installed: HashSet<u64> = catalog
            .categories
            .iter()
            .flat_map(|category| &category.costumes)
            .filter_map(|costume| costume.gamebanana_id)
            .collect();
        let mut categories = Vec::new();
        let mut mods = Vec::with_capacity(catalog.categories.len());
        for (index, category) in catalog.categories.iter().enumerate() {
            let all = matches_category(category, &needle);
            let list = list_key(category, &needle).and_then(|key| gamebanana.list(&key));
            // A mod the overlay installs waits behind its card until the
            // card crosses.
            let held = |gamebanana_id: Option<u64>| {
                gamebanana_id.is_some_and(|id| {
                    gamebanana.holds(id)
                        && list.is_some_and(|list| list.mods.iter().any(|item| item.id == id))
                })
            };
            let mut visible: Vec<usize> = category
                .costumes
                .iter()
                .enumerate()
                .filter(|(_, costume)| all || search_key(&costume.name).contains(&needle))
                .filter(|(_, costume)| !held(costume.gamebanana_id))
                .map(|(costume_index, _)| costume_index)
                .collect();
            if all || !visible.is_empty() {
                categories.push(index);
                if let Some(list) = list {
                    let own = category.costumes.len();
                    visible.extend(
                        list.mods
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| {
                                !installed.contains(&item.id) || gamebanana.holds(item.id)
                            })
                            .map(|(position, _)| own + position),
                    );
                    if list.status_card().is_some() {
                        visible.push(own + list.mods.len());
                    }
                }
            }
            mods.push(visible);
        }
        Self {
            query: query.to_owned(),
            needle,
            categories,
            mods,
        }
    }
}

/// Whether a search shows all of a category's mods: it's empty, or it
/// matches the category's name.
fn matches_category(category: &Category, needle: &str) -> bool {
    needle.is_empty() || search_key(character_name(&category.name)).contains(needle)
}

/// The GameBanana list a category shows with the search `needle`: the
/// character's mods when the search shows all of the category's own,
/// otherwise the ones that match.
fn list_key(category: &Category, needle: &str) -> Option<ListKey> {
    Some(ListKey {
        character: category.character?,
        query: if matches_category(category, needle) {
            String::new()
        } else {
            needle.to_owned()
        },
    })
}

/// An answer in the panel that shows an install's question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    File(u64),
    SameName(SameNameChoice),
    Cancel,
}

/// How the question panel shows an answer.
struct AnswerRow {
    label: String,
    detail: Option<String>,
    size: Option<String>,
}

impl AnswerRow {
    /// `files` are the ones a file answer picks from.
    fn new(answer: Answer, files: &[crate::overlay_protocol::InstallFile]) -> Self {
        let (label, detail, size) = match answer {
            Answer::File(id) => match files.iter().find(|file| file.id == id) {
                Some(file) => (
                    file.name.clone(),
                    file.description.clone(),
                    Some(file_size_label(file.size)),
                ),
                None => (id.to_string(), None, None),
            },
            Answer::SameName(SameNameChoice::Replace) => (
                text(TextKey::GameOverlayReplace).to_owned(),
                Some(text(TextKey::GameOverlayReplaceDetail).to_owned()),
                None,
            ),
            Answer::SameName(SameNameChoice::Merge) => (
                text(TextKey::GameOverlayMerge).to_owned(),
                Some(text(TextKey::GameOverlayMergeDetail).to_owned()),
                None,
            ),
            Answer::SameName(SameNameChoice::KeepBoth) => (
                text(TextKey::GameOverlayKeepBoth).to_owned(),
                Some(text(TextKey::GameOverlayKeepBothDetail).to_owned()),
                None,
            ),
            Answer::Cancel => (text(TextKey::GameOverlayCancel).to_owned(), None, None),
        };
        Self {
            label,
            detail,
            size,
        }
    }
}

/// The answers to the question an install asks at `stage`, then Cancel.
fn answers(stage: &InstallStage) -> Vec<Answer> {
    let mut answers = match stage {
        InstallStage::ChooseFile { files } => {
            files.iter().map(|file| Answer::File(file.id)).collect()
        }
        InstallStage::SameName { .. } => vec![
            Answer::SameName(SameNameChoice::Replace),
            Answer::SameName(SameNameChoice::Merge),
            Answer::SameName(SameNameChoice::KeepBoth),
        ],
        _ => return Vec::new(),
    };
    answers.push(Answer::Cancel);
    answers
}

/// One card of the carousel.
#[derive(Clone, Copy)]
enum Card<'a> {
    Own(&'a Costume),
    GameBanana(&'a BrowseMod),
    Status(StatusCard),
}

/// Text as a search compares it: underscores read as spaces, runs of spaces
/// count as one, and case does not matter.  A mod's raw name holds its
/// displayed name, so matching the raw name also matches what is shown.
fn search_key(text: &str) -> String {
    text.replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// In-memory state for the native costume switcher preview.
pub(super) struct Layouts {
    catalog: Catalog,
    selected_category: usize,
    carousel_focus: usize,
    carousel_focus_by_category: Vec<usize>,
    active_images: Vec<Option<PathBuf>>,
    thumbnails: ThumbnailCache,
    reveal_category: bool,
    category_wheel_accum: f32,
    category_wheel_last_event_at: f64,
    category_wheel_last_step_at: f64,
    category_wheel_suppress_until: f64,
    carousel_wheel_accum: f32,
    carousel_wheel_last_event_at: f64,
    carousel_wheel_last_step_at: f64,
    carousel_wheel_suppress_until: f64,
    carousel_transition: Option<CarouselTransition>,
    carousel_pointer_press: Option<CarouselPointerPress>,
    /// The card clicked last, by id, and when.
    last_card_click: Option<(String, f64)>,
    held_preview: Option<HeldPreview>,
    reveal_progress: f32,
    visible_card_rects: Vec<Rect>,
    active_feedback_until: f64,
    active_feedback_costume: Option<usize>,
    boundary_feedback_until: f64,
    boundary_feedback_edge: Option<i32>,
    pending_commands: VecDeque<PendingCommand>,
    row: Row,
    filter: Filter,
    /// Changes made on screen since the last `take_requests`.
    requests: Vec<ModRequest>,
    /// Mods whose change waits for Hestia, by id.  None in the preview, whose
    /// changes are made on screen only.
    waiting: Option<HashSet<String>>,
    gamebanana: GameBanana,
    /// "Show all characters" adds the game's GameBanana characters without
    /// a category to the rail.
    show_all_characters: bool,
    /// The highlighted answer in the question panel, and the mod whose
    /// question it answers.
    question_row: usize,
    question_for: Option<u64>,
    /// Mods the overlay installed, tagged NEW until they're used.
    new_mods: HashSet<String>,
    /// "... is now with your mods", for the installed mods that crossed out
    /// of sight since the last `take_arrivals`.
    arrivals: Vec<String>,
    keys: CardKeys,
}

impl Layouts {
    pub(super) fn new(catalog: Catalog) -> Self {
        // Start at the leftmost category.
        let selected_category = 0;
        let carousel_focus = catalog
            .categories
            .get(selected_category)
            .map(active_costume_index)
            .unwrap_or(0);
        let carousel_focus_by_category = catalog
            .categories
            .iter()
            .map(active_costume_index)
            .collect::<Vec<_>>();
        let active_images = catalog
            .categories
            .iter()
            .map(active_image)
            .collect::<Vec<_>>();
        let gamebanana = GameBanana::default();
        let filter = Filter::new(&catalog, "", &gamebanana);
        let mut thumbnails = ThumbnailCache::new();
        thumbnails.set_censored(censored_images(&catalog));

        Self {
            catalog,
            selected_category,
            carousel_focus,
            carousel_focus_by_category,
            active_images,
            thumbnails,
            reveal_category: true,
            category_wheel_accum: 0.0,
            category_wheel_last_event_at: f64::NEG_INFINITY,
            category_wheel_last_step_at: f64::NEG_INFINITY,
            category_wheel_suppress_until: f64::NEG_INFINITY,
            carousel_wheel_accum: 0.0,
            carousel_wheel_last_event_at: f64::NEG_INFINITY,
            carousel_wheel_last_step_at: f64::NEG_INFINITY,
            carousel_wheel_suppress_until: f64::NEG_INFINITY,
            carousel_transition: None,
            carousel_pointer_press: None,
            last_card_click: None,
            held_preview: None,
            reveal_progress: 1.0,
            visible_card_rects: Vec::new(),
            active_feedback_until: f64::NEG_INFINITY,
            active_feedback_costume: None,
            boundary_feedback_until: f64::NEG_INFINITY,
            boundary_feedback_edge: None,
            pending_commands: VecDeque::new(),
            row: Row::default(),
            filter,
            requests: Vec::new(),
            waiting: None,
            gamebanana,
            show_all_characters: false,
            question_row: 0,
            question_for: None,
            new_mods: HashSet::new(),
            arrivals: Vec::new(),
            keys: CardKeys::default(),
        }
    }

    /// The installed mods that crossed out of sight since the last call, by
    /// name.
    pub(super) fn take_arrivals(&mut self) -> Vec<String> {
        std::mem::take(&mut self.arrivals)
    }

    /// The changes made on screen since the last call, oldest first.
    pub(super) fn take_requests(&mut self) -> Vec<ModRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Shows which mods' changes wait for Hestia.  From then on, a change
    /// shows that it waits in the frame it's made, before Hestia is asked.
    pub(super) fn set_waiting(&mut self, waiting: HashSet<String>) {
        self.waiting = Some(waiting);
    }

    /// Asks Hestia for a change made on screen.  In the game, its card shows
    /// that it waits at once, never done first.
    fn ask(&mut self, request: ModRequest) {
        if let Some(waiting) = &mut self.waiting {
            waiting.insert(request.mod_id.clone());
        }
        self.requests.push(request);
    }

    /// Shows GameBanana mods after the categories' own, which only the
    /// in-game overlay can ask Hestia for, and with `all_characters`, the
    /// characters no category stands for.  Or hides them.
    pub(super) fn set_gamebanana(&mut self, enabled: bool, all_characters: bool) {
        let show_all = enabled && all_characters;
        if self.gamebanana.enabled() == enabled && self.show_all_characters == show_all {
            return;
        }
        self.gamebanana.set_enabled(enabled);
        self.show_all_characters = show_all;
        // The characters give the categories their pictures too.
        self.gamebanana.ask_characters();
        self.replace_catalog(self.own_catalog());
    }

    /// What to ask Hestia for since the last call.
    pub(super) fn take_gamebanana_requests(&mut self) -> Vec<FromOverlay> {
        self.gamebanana.take_requests()
    }

    /// The mods whose hotkeys to ask Hestia for since the last call.
    pub(super) fn take_hotkey_requests(&mut self) -> Vec<FromOverlay> {
        self.keys.take_requests()
    }

    pub(super) fn receive_hotkeys(&mut self, answers: Vec<ModHotkeys>) {
        for answer in answers {
            self.keys.receive(answer);
        }
    }

    /// Takes GameBanana pages, characters, pictures and installs from
    /// Hestia.  Says what the installs' news tells.
    pub(super) fn receive_gamebanana(&mut self, messages: Vec<ToOverlay>) -> Vec<InstallNews> {
        let mut news = Vec::new();
        if messages.is_empty() {
            return news;
        }
        let mut lists_changed = false;
        let mut characters_changed = false;
        for message in messages {
            match message {
                ToOverlay::BrowsePage(page) => {
                    lists_changed |= self.gamebanana.receive_page(page);
                }
                ToOverlay::Characters { characters, error } => {
                    self.gamebanana.set_characters(characters, error);
                    characters_changed = true;
                }
                ToOverlay::Pictures(pictures) => {
                    characters_changed |= self.gamebanana.apply_pictures(&pictures);
                    lists_changed = true;
                }
                ToOverlay::Install(update) => {
                    let mods = match &update.stage {
                        InstallStage::Installed { mods } => mods.clone(),
                        _ => Vec::new(),
                    };
                    match self.gamebanana.receive_install(update) {
                        // Hestia's window can install a mod turned on, which
                        // is in use then, not new.
                        Some(InstallNews::Installed(name)) => {
                            let (on, off): (Vec<String>, Vec<String>) =
                                mods.into_iter().partition(|id| self.mod_active(id));
                            self.new_mods.extend(off);
                            news.push(if on.is_empty() {
                                InstallNews::Installed(name)
                            } else {
                                InstallNews::InstalledOn(name)
                            });
                        }
                        item => news.extend(item),
                    }
                }
                _ => {}
            }
        }
        if characters_changed {
            // Rebuilding the catalog rebuilds the lists' cards too.
            self.replace_catalog(self.own_catalog());
        } else if lists_changed {
            self.refresh_gamebanana();
        }
        news
    }

    pub(super) fn show_all_characters(&self) -> bool {
        self.show_all_characters
    }

    /// Adds the game's GameBanana characters without a category to the rail,
    /// after the library's categories, or takes them away.
    pub(super) fn set_show_all_characters(&mut self, show: bool) {
        if self.show_all_characters == show || !self.gamebanana.enabled() {
            return;
        }
        self.show_all_characters = show;
        if show {
            self.gamebanana.ask_characters();
        }
        self.replace_catalog(self.own_catalog());
    }

    /// The library without the categories "Show all characters" added.
    fn own_catalog(&self) -> Catalog {
        let mut catalog = self.catalog.clone();
        catalog.categories.retain(|category| !category.extra);
        catalog
    }

    /// `catalog` with GameBanana's characters.  A category without a link
    /// takes the character its name stands for, and the character's picture
    /// in place of its mods'.  With "Show all characters" on, the characters
    /// no category stands for come after the library's categories.
    fn with_characters(&self, mut catalog: Catalog) -> Catalog {
        let characters = self.gamebanana.characters();
        for category in &mut catalog.categories {
            if category.extra || category.id == UNCATEGORIZED_ID {
                continue;
            }
            let character = match category.character {
                Some(id) => characters.iter().find(|character| character.id == id),
                None => characters
                    .iter()
                    .find(|character| names_character(&category.name, &character.name)),
            };
            if let Some(character) = character {
                category.character = Some(character.id);
                if category.image.is_none() {
                    category.image = character.image.clone();
                }
            }
        }
        if !self.show_all_characters {
            return catalog;
        }
        let linked: HashSet<u64> = catalog
            .categories
            .iter()
            .filter_map(|category| category.character)
            .collect();
        let mut extras: Vec<Category> = self
            .gamebanana
            .characters()
            .iter()
            .filter(|character| !linked.contains(&character.id))
            .map(|character| Category {
                id: format!("{EXTRA_CATEGORY_PREFIX}{}", character.id),
                name: character.name.clone(),
                image: character.image.clone(),
                character: Some(character.id),
                extra: true,
                costumes: Vec::new(),
            })
            .collect();
        extras.sort_by_cached_key(|category| category.name.to_lowercase());
        catalog.categories.extend(extras);
        catalog
    }

    /// Rebuilds the cards after a GameBanana list changed, keeping the
    /// focused card.  A card that went away leaves the focus on the card
    /// before it.
    fn refresh_gamebanana(&mut self) {
        let query = std::mem::take(&mut self.filter.query);
        self.filter = Filter::new(&self.catalog, &query, &self.gamebanana);
        let mut censored = censored_images(&self.catalog);
        censored.extend(self.gamebanana.censored_images());
        self.thumbnails.set_censored(censored);
        let visible = self.visible_mods();
        if !visible.is_empty() && !visible.contains(&self.carousel_focus) {
            self.carousel_focus = visible
                .iter()
                .rev()
                .find(|&&index| index < self.carousel_focus)
                .copied()
                .unwrap_or(visible[0]);
            if let Some(slot) = self
                .carousel_focus_by_category
                .get_mut(self.selected_category)
            {
                *slot = self.carousel_focus;
            }
        }
    }

    /// Moves the cards of the installs that finished a moment ago into the
    /// categories' own mods, each to its sorted place.  The focus stays on
    /// the card it was on, so a focused card that crosses stays in the
    /// middle while the others slide around it.
    fn cross_installs(&mut self, ctx: &egui::Context, carousel_rect: Rect, now: f64) {
        let (crossed, next) = self.gamebanana.cross_finished(now);
        if let Some(at) = next {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((at - now).max(0.0)));
        }
        if crossed.is_empty() {
            return;
        }
        let category_index = self.selected_category;
        let from: Vec<(Option<String>, CarouselCardPlacement)> = self
            .current_carousel_placements(carousel_rect, now)
            .into_iter()
            .map(|placement| (self.card_id(category_index, placement.index), placement))
            .collect();
        let selection = self.selection();
        let query = std::mem::take(&mut self.filter.query);
        self.filter = Filter::new(&self.catalog, &query, &self.gamebanana);
        self.apply_selection(&selection, category_index);
        let from = from
            .into_iter()
            .filter_map(|(id, mut placement)| {
                placement.index = self.card_position(category_index, &id?)?;
                Some(placement)
            })
            .collect();
        self.begin_carousel_transition(from, carousel_rect, now);
        if let Some(transition) = &mut self.carousel_transition {
            transition.seconds = CROSSING_SECS;
        }
        // One that lands out of sight says where it went.
        let shown: Vec<usize> = self
            .target_placements(carousel_rect)
            .iter()
            .map(|placement| placement.index)
            .collect();
        if let Some(category) = self.catalog.categories.get(category_index) {
            for gamebanana_id in crossed {
                let landed = category
                    .costumes
                    .iter()
                    .position(|costume| costume.gamebanana_id == Some(gamebanana_id));
                if let Some(index) = landed
                    && !shown.contains(&index)
                {
                    self.arrivals
                        .push(clean_display_name(&category.costumes[index].name));
                    ctx.request_repaint();
                }
            }
        }
    }

    /// The GameBanana list the category shows with the current search.
    fn gamebanana_key(&self, category_index: usize) -> Option<ListKey> {
        let category = self.catalog.categories.get(category_index)?;
        list_key(category, &self.filter.needle)
    }

    fn gamebanana_list(&self, category_index: usize) -> Option<&gamebanana::List> {
        self.gamebanana.list(&self.gamebanana_key(category_index)?)
    }

    /// The card at `index` in a category: its own mods, then its GameBanana
    /// list's mods, then the list's state.
    fn card(&self, category_index: usize, index: usize) -> Option<Card<'_>> {
        let category = self.catalog.categories.get(category_index)?;
        if let Some(costume) = category.costumes.get(index) {
            return Some(Card::Own(costume));
        }
        let list = self.gamebanana_list(category_index)?;
        let position = index - category.costumes.len();
        match list.mods.get(position) {
            Some(item) => Some(Card::GameBanana(item)),
            None if position == list.mods.len() => list.status_card().map(Card::Status),
            None => None,
        }
    }

    fn card_count(&self, category_index: usize) -> usize {
        let own = self
            .catalog
            .categories
            .get(category_index)
            .map_or(0, |category| category.costumes.len());
        own + self.gamebanana_list(category_index).map_or(0, |list| {
            list.mods.len() + usize::from(list.status_card().is_some())
        })
    }

    fn card_image(&self, category_index: usize, index: usize) -> Option<PathBuf> {
        match self.card(category_index, index)? {
            Card::Own(costume) => costume.image.clone(),
            Card::GameBanana(item) => item.image.clone(),
            Card::Status(_) => None,
        }
    }

    /// A card's selection id: its mod's, or one for GameBanana cards.
    fn card_id(&self, category_index: usize, index: usize) -> Option<String> {
        Some(match self.card(category_index, index)? {
            Card::Own(costume) => costume.id.clone(),
            Card::GameBanana(item) => format!("{GAMEBANANA_PREFIX}{}", item.id),
            Card::Status(_) => GAMEBANANA_END.to_owned(),
        })
    }

    fn card_position(&self, category_index: usize, id: &str) -> Option<usize> {
        let category = self.catalog.categories.get(category_index)?;
        if let Some(position) = category
            .costumes
            .iter()
            .position(|costume| costume.id == id)
        {
            return Some(position);
        }
        let gamebanana_id: Option<u64> = id
            .strip_prefix(GAMEBANANA_PREFIX)
            .and_then(|id| id.parse().ok());
        // A GameBanana mod that got installed is one of the category's own,
        // once its card crossed.
        if let Some(position) = gamebanana_id
            .filter(|&gamebanana_id| !self.gamebanana.holds(gamebanana_id))
            .and_then(|gamebanana_id| {
                category
                    .costumes
                    .iter()
                    .position(|costume| costume.gamebanana_id == Some(gamebanana_id))
            })
        {
            return Some(position);
        }
        let list = self.gamebanana_list(category_index)?;
        let own = category.costumes.len();
        if id == GAMEBANANA_END {
            return list.status_card().map(|_| own + list.mods.len());
        }
        list.mods
            .iter()
            .position(|item| Some(item.id) == gamebanana_id)
            .map(|position| own + position)
    }

    /// Where the category `id` is in the rail.  A category "Show all
    /// characters" added is the category for its character once there is one.
    fn category_position(&self, id: &str) -> Option<usize> {
        let categories = &self.catalog.categories;
        categories
            .iter()
            .position(|category| category.id == id)
            .or_else(|| {
                let character: u64 = id.strip_prefix(EXTRA_CATEGORY_PREFIX)?.parse().ok()?;
                categories
                    .iter()
                    .position(|category| category.character == Some(character))
            })
    }

    /// Starts the selected category's GameBanana list, or asks for its next
    /// page, once the carousel nears the end of its cards.
    fn update_gamebanana(&mut self, ctx: &egui::Context, now: f64) {
        if !self.gamebanana.enabled() || !self.filter.categories.contains(&self.selected_category) {
            return;
        }
        let Some(key) = self.gamebanana_key(self.selected_category) else {
            return;
        };
        let visible = self.visible_mods();
        let position = visible
            .iter()
            .position(|&index| index == self.carousel_focus)
            .unwrap_or(0);
        if position + gamebanana::LOAD_AHEAD < visible.len() {
            return;
        }
        if self.gamebanana.want(&key, now) {
            self.refresh_gamebanana();
        }
        if let Some(at) = self.gamebanana.settles_at(&key) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((at - now).max(0.0)));
        }
    }

    /// Space or a click on a GameBanana card installs its mod into the
    /// category, or into a new one for its character when the category is
    /// one "Show all characters" added.  On the card after a list whose page
    /// failed, any action asks again.
    fn activate_gamebanana(&mut self, category_index: usize, index: usize, install: bool) {
        match self.card(category_index, index) {
            Some(Card::GameBanana(item)) if install => {
                let item = item.clone();
                let category_id = self
                    .catalog
                    .categories
                    .get(category_index)
                    .filter(|category| !category.extra)
                    .map(|category| category.id.clone());
                self.gamebanana.install(&item, category_id);
            }
            Some(Card::Status(StatusCard::Failed)) => {
                if let Some(key) = self.gamebanana_key(category_index) {
                    self.gamebanana.retry(&key);
                }
            }
            _ => {}
        }
    }

    /// What Space does on a focused GameBanana card: install it, or try
    /// again after a failed install or list.  Nothing while it installs.
    fn gamebanana_space(&self) -> Option<Space> {
        if !self.visible_mods().contains(&self.carousel_focus) {
            return None;
        }
        match self.card(self.selected_category, self.carousel_focus) {
            Some(Card::GameBanana(item)) => match self.gamebanana.install_state(item.id) {
                Some(install) if install.holds() => None,
                Some(install) if install.stage == InstallStage::Failed => Some(Space::TryAgain),
                _ => Some(Space::Install),
            },
            Some(Card::Status(StatusCard::Failed)) => Some(Space::TryAgain),
            _ => None,
        }
    }

    /// Where the overlay is, by ids: the selected category, and each
    /// category's focused mod where it isn't the one a fresh start would pick.
    pub(super) fn selection(&self) -> Selection {
        let mods = self
            .catalog
            .categories
            .iter()
            .enumerate()
            .filter_map(|(index, category)| {
                let focus = if index == self.selected_category {
                    self.carousel_focus
                } else {
                    *self.carousel_focus_by_category.get(index)?
                };
                (focus != active_costume_index(category))
                    .then_some((category.id.clone(), self.card_id(index, focus)?))
            })
            .collect();
        Selection {
            category_id: self
                .catalog
                .categories
                .get(self.selected_category)
                .map(|category| category.id.clone()),
            mods,
        }
    }

    /// Go back to where `selection` left the overlay.  Without its category,
    /// the overlay starts at the leftmost one.
    pub(super) fn restore_selection(&mut self, selection: &Selection) {
        self.apply_selection(selection, 0);
    }

    /// Take a new library from Hestia and stay on the same category and mods,
    /// by id.  If the selected category is gone, the selection stays at the
    /// same place in the rail.  A category left on its active mod follows the
    /// active mod, as it would on a fresh start.
    pub(super) fn replace_catalog(&mut self, catalog: Catalog) {
        self.keys.library_changed();
        let selection = self.selection();
        let previous_index = self.selected_category;
        let query = std::mem::take(&mut self.filter.query);
        self.catalog = self.with_characters(catalog);
        let mut censored = censored_images(&self.catalog);
        censored.extend(self.gamebanana.censored_images());
        self.thumbnails.set_censored(censored);
        self.carousel_focus_by_category = self
            .catalog
            .categories
            .iter()
            .map(active_costume_index)
            .collect();
        self.active_images = self.catalog.categories.iter().map(active_image).collect();
        self.filter = Filter::new(&self.catalog, &query, &self.gamebanana);
        self.carousel_transition = None;
        self.carousel_pointer_press = None;
        self.held_preview = None;
        self.visible_card_rects.clear();
        self.active_feedback_until = f64::NEG_INFINITY;
        self.active_feedback_costume = None;
        self.boundary_feedback_until = f64::NEG_INFINITY;
        self.boundary_feedback_edge = None;
        self.apply_selection(&selection, previous_index);
        self.forget_used_new_mods();
    }

    fn apply_selection(&mut self, selection: &Selection, fallback_category: usize) {
        for (category_id, mod_id) in &selection.mods {
            let Some(index) = self.category_position(category_id) else {
                continue;
            };
            if let (Some(focus), Some(slot)) = (
                self.card_position(index, mod_id),
                self.carousel_focus_by_category.get_mut(index),
            ) {
                *slot = focus;
            }
        }
        let last = self.catalog.categories.len().saturating_sub(1);
        self.selected_category = selection
            .category_id
            .as_ref()
            .and_then(|id| self.category_position(id))
            .unwrap_or(fallback_category)
            .min(last);
        self.carousel_focus = self
            .carousel_focus_by_category
            .get(self.selected_category)
            .copied()
            .unwrap_or(0);
        self.clamp_selection();
        if !self.filter.categories.contains(&self.selected_category) {
            if let Some(&first) = self.filter.categories.first() {
                self.selected_category = first;
                self.carousel_focus = self.carousel_focus_by_category[first];
                self.clamp_selection();
            }
        }
        self.carousel_focus = self.visible_focus(self.selected_category, self.carousel_focus);
        if let Some(slot) = self
            .carousel_focus_by_category
            .get_mut(self.selected_category)
        {
            *slot = self.carousel_focus;
        }
        self.reveal_category = true;
    }

    pub(super) fn prepare_thumbnails(&mut self, ctx: &egui::Context) {
        self.thumbnails.poll(ctx);
        let mut paths = Vec::new();
        // Prioritize the current carousel, then its next cards and adjacent
        // characters. The mini strip also calls this to warm the first view.
        append_nearby_mod_images(
            &mut paths,
            self.visible_mods(),
            self.carousel_focus,
            2,
            |index| self.card_image(self.selected_category, index),
        );
        let visible = &self.filter.categories;
        let selected = visible
            .iter()
            .position(|&index| index == self.selected_category)
            .unwrap_or(0);
        for (position, &index) in visible
            .iter()
            .enumerate()
            .take(selected + 5)
            .skip(selected.saturating_sub(4))
        {
            let category = &self.catalog.categories[index];
            paths.extend(
                category
                    .image
                    .clone()
                    .or_else(|| self.active_images[index].clone()),
            );
            if index != self.selected_category && position.abs_diff(selected) <= 1 {
                append_nearby_mod_images(
                    &mut paths,
                    &self.filter.mods[index],
                    active_costume_index(category),
                    1,
                    |costume| self.card_image(index, costume),
                );
            }
        }
        self.thumbnails.prefetch(ctx, paths);
    }

    /// Draw the centered mod carousel. The parent reserves approximately 560x266 for it.
    pub(super) fn show_carousel(&mut self, ui: &mut Ui, overlay_opacity: u8) {
        self.visible_card_rects.clear();
        if self.catalog.categories.is_empty() {
            let empty_rects = self.show_empty_catalog(ui, overlay_opacity);
            self.visible_card_rects.extend(empty_rects);
            return;
        }

        self.clamp_selection();
        let available = ui.available_size();
        let width = available.x.max(1.0);
        let height = available.y.min(CAROUSEL_HEIGHT).max(1.0);
        let (carousel_rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let now = ui.input(|input| input.time);
        self.consume_pending_commands(ui.ctx(), carousel_rect, now);
        self.clamp_selection();
        self.update_gamebanana(ui.ctx(), now);
        self.cross_installs(ui.ctx(), carousel_rect, now);
        let category_index = self.selected_category;
        let visible_count = self.visible_mods().len();
        self.keys.begin_frame();
        let focused_mod = self.focused_mod_id();
        self.keys.keep_focused(focused_mod.as_deref());
        if let Some(mod_id) = &focused_mod {
            self.keys.ask(mod_id);
        }
        // An install's question covers the carousel until it's answered.
        let question = self.current_question();
        if question.is_some() {
            self.cancel_pointer_interaction();
        }
        if visible_count == 0 {
            if let Some((mod_id, answers)) = question {
                self.show_question(ui, carousel_rect, mod_id, &answers, overlay_opacity);
                return;
            }
            let text = self.empty_carousel_text();
            let font = FontId::proportional(15.0);
            let galley = ui.painter().layout_no_wrap(
                text.to_owned(),
                font.clone(),
                content_gray(235, overlay_opacity),
            );
            ui.painter().text(
                carousel_rect.center(),
                Align2::CENTER_CENTER,
                text,
                font,
                content_gray(235, overlay_opacity),
            );
            self.visible_card_rects
                .push(Rect::from_center_size(carousel_rect.center(), galley.size()).expand(2.0));
            return;
        }

        if self.carousel_pointer_press.is_some()
            && ui.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::PointerGone))
                    || (input.pointer.latest_pos().is_none()
                        && !input.pointer.primary_down()
                        && !input.pointer.primary_released())
            })
        {
            // A lost pointer cannot produce a trustworthy release target. Drop the capture so
            // a later click after Alt/collapse cannot activate a stale card.
            self.cancel_pointer_interaction();
        }
        let mut placements = self.current_carousel_placements(carousel_rect, now);
        if let Some((mod_id, answers)) = question {
            // The cards stay behind the panel, dimmed and out of reach.
            for placement in &placements {
                let reveal = card_reveal_progress(placement, self.reveal_progress, &placements);
                self.show_carousel_card(
                    ui,
                    category_index,
                    placement.index,
                    placement.rect,
                    placement.focused,
                    placement.slot,
                    reveal.min(0.3),
                    overlay_opacity,
                );
            }
            self.show_question(ui, carousel_rect, mod_id, &answers, overlay_opacity);
            self.request_animation_repaint(ui.ctx(), now);
            return;
        }
        self.keys.scroll_with_wheel(ui);
        let wheel_direction = self.consume_carousel_wheel(ui, carousel_rect, visible_count);
        if let Some(direction) = wheel_direction {
            self.advance_mod_focus(carousel_rect, direction, now);
            placements = self.current_carousel_placements(carousel_rect, now);
            ui.ctx().request_repaint();
        }

        self.update_held_preview(ui, category_index, &placements);

        if self.held_preview.is_none() {
            self.capture_carousel_press(ui, category_index, &placements);
            if self
                .carousel_pointer_press
                .as_ref()
                .is_some_and(|press| press.category_index == category_index)
                && ui
                    .input(|input| input.pointer.primary_down() || input.pointer.primary_released())
            {
                // Keep the pressed card under the cursor through release. This makes a concurrent
                // repaint or carousel transition harmless to the pending click and its release frame.
                if let Some(press) = &self.carousel_pointer_press {
                    placements = press.placements.clone();
                }
            }
        }

        if self.held_preview.is_none() {
            for placement in &placements {
                // The carousel is a visual slot layout: when the category changes, a
                // different costume can occupy the same left/center/right rectangle. Use
                // that stable slot for egui's interaction id so its widget-rect identity
                // does not change underneath the pointer between passes.
                let reveal = card_reveal_progress(placement, self.reveal_progress, &placements);
                self.show_carousel_card(
                    ui,
                    category_index,
                    placement.index,
                    placement.rect,
                    placement.focused,
                    placement.slot,
                    reveal,
                    overlay_opacity,
                );
            }
            self.paint_gamebanana_divider(ui, &placements, carousel_rect, overlay_opacity);
        }
        if self.held_preview.is_none()
            && let Some(costume_index) =
                self.release_carousel_press(ui, category_index, carousel_rect, now)
            && !self.repeated_click(category_index, costume_index, now)
        {
            // A click activates in place. The focused card remains focused, so a side-card
            // click never moves the target away from the pointer before the next action.
            self.activate_costume_in_place(category_index, costume_index, now);
            ui.ctx().request_repaint();
        }
        self.request_animation_repaint(ui.ctx(), now);
    }

    /// Set the staged expansion progress used by the parent overlay animation.
    /// The focused card appears first; neighbors are staggered by
    /// `card_reveal_progress`. Invisible cards keep their stable slot IDs but do not
    /// accept activation until they are almost fully revealed.
    pub(super) fn set_reveal(&mut self, progress: f32) {
        self.reveal_progress = progress.clamp(0.0, 1.0);
        if self.reveal_progress <= 0.0 {
            self.visible_card_rects.clear();
            self.held_preview = None;
            self.carousel_pointer_press = None;
        }
    }

    /// Queue a mod navigation command for the next carousel pass, when its current geometry is
    /// available. This keeps keyboard navigation on the same animated path as the wheel.
    pub(super) fn navigate_mod(&mut self, ctx: &egui::Context, direction: i32) {
        if direction == 0 || self.pointer_gesture_active_in_context(ctx) {
            return;
        }
        self.pending_commands
            .push_back(PendingCommand::Mod(direction.signum()));
        ctx.request_repaint();
    }

    /// Navigate categories using the same clamped order as the category wheel.
    pub(super) fn navigate_category(&mut self, ctx: &egui::Context, direction: i32) {
        if direction == 0
            || self.pointer_gesture_active_in_context(ctx)
            || self.catalog.categories.is_empty()
        {
            return;
        }
        self.pending_commands
            .push_back(PendingCommand::Category(direction.signum()));
        suppress_tooltips(ctx);
        ctx.request_repaint();
    }

    /// Queue an explicit action for the focused mod. The command is applied on the next
    /// carousel pass, alongside ordinary activation, so navigation and actions retain FIFO order
    /// and reveal/gesture guards.
    pub(super) fn apply_focused_action(&mut self, ctx: &egui::Context, action: ModAction) {
        if self.pointer_gesture_active_in_context(ctx) {
            return;
        }
        self.pending_commands
            .push_back(PendingCommand::Action(action));
        ctx.request_repaint();
    }

    /// Turns the focused card over to its hotkeys, or back, after the
    /// commands queued before it.
    pub(super) fn flip_focused(&mut self, ctx: &egui::Context) {
        self.pending_commands.push_back(PendingCommand::Flip);
        ctx.request_repaint();
    }

    /// The focused card's mod, when it's one of the library's.
    fn focused_mod_id(&self) -> Option<String> {
        match self.card(self.selected_category, self.carousel_focus)? {
            Card::Own(costume) if self.visible_mods().contains(&self.carousel_focus) => {
                Some(costume.id.clone())
            }
            _ => None,
        }
    }

    /// An install asks a question, which W/S, Space and Esc answer until
    /// it's answered.
    pub(super) fn question_open(&self) -> bool {
        self.gamebanana.question().is_some()
    }

    /// The install that asks, its answers, and the highlighted one kept on
    /// them.  A new question starts on its first answer.
    fn current_question(&mut self) -> Option<(u64, Vec<Answer>)> {
        let (mod_id, install) = self.gamebanana.question()?;
        let answers = answers(&install.stage);
        if self.question_for != Some(mod_id) {
            self.question_for = Some(mod_id);
            self.question_row = 0;
        }
        self.question_row = self.question_row.min(answers.len().saturating_sub(1));
        Some((mod_id, answers))
    }

    /// Up for a negative direction, down for a positive one.
    pub(super) fn move_question(&mut self, direction: i32) {
        if let Some((_, answers)) = self.current_question() {
            self.question_row = self
                .question_row
                .saturating_add_signed(direction.signum() as isize)
                .min(answers.len().saturating_sub(1));
        }
    }

    /// Gives the highlighted answer.
    pub(super) fn answer_question(&mut self) {
        let Some((mod_id, answers)) = self.current_question() else {
            return;
        };
        match answers.get(self.question_row) {
            Some(Answer::File(file_id)) => self.gamebanana.pick_file(mod_id, *file_id),
            Some(Answer::SameName(choice)) => self.gamebanana.choose_same_name(mod_id, *choice),
            Some(Answer::Cancel) => self.gamebanana.cancel_install(mod_id),
            None => {}
        }
    }

    /// Stops the install that asks.  False without a question.
    pub(super) fn cancel_question(&mut self) -> bool {
        let Some((mod_id, _)) = self.gamebanana.question() else {
            return false;
        };
        self.gamebanana.cancel_install(mod_id);
        true
    }

    /// The panel over the carousel with the question an install asks, and
    /// its answers: Space or a click gives the highlighted one.
    fn show_question(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        mod_id: u64,
        answers: &[Answer],
        overlay_opacity: u8,
    ) {
        const PADDING: f32 = 12.0;
        const HEADER: f32 = 50.0;
        const ANSWER_HEIGHT: f32 = 34.0;
        let Some(install) = self.gamebanana.install_state(mod_id) else {
            return;
        };
        let title = clean_display_name(&install.name);
        let (prompt, files) = match &install.stage {
            InstallStage::ChooseFile { files } => (
                text(TextKey::GameOverlayPickFile).to_owned(),
                files.as_slice(),
            ),
            InstallStage::SameName { folder } => (
                text(TextKey::GameOverlaySameName).replace("{folder}", folder),
                &[][..],
            ),
            _ => return,
        };
        let rows: Vec<AnswerRow> = answers
            .iter()
            .map(|answer| AnswerRow::new(*answer, files))
            .collect();

        let width = (area.width() - 40.0).clamp(1.0, 440.0);
        let room = area.height() - 8.0 - HEADER - PADDING;
        let shown = ((room / ANSWER_HEIGHT).floor() as usize).clamp(1, rows.len());
        let first = self
            .question_row
            .saturating_sub(shown / 2)
            .min(rows.len() - shown);
        let panel = Rect::from_center_size(
            area.center(),
            Vec2::new(width, HEADER + shown as f32 * ANSWER_HEIGHT + PADDING),
        );
        self.visible_card_rects.push(panel);
        ui.painter().rect_filled(
            panel,
            CornerRadius::same(4),
            rgba_alpha(26, 26, 26, 245, overlay_opacity),
        );
        ui.painter().rect_stroke(
            panel,
            CornerRadius::same(4),
            Stroke::new(1.0, content_gray(82, overlay_opacity)),
            StrokeKind::Inside,
        );
        let text_width = width - PADDING * 2.0;
        let title_galley = elided_galley(
            ui,
            &title,
            FontId::proportional(14.0),
            content_gray(235, overlay_opacity),
            text_width,
        );
        ui.painter().galley(
            panel.min + Vec2::new(PADDING, PADDING),
            title_galley,
            Color32::PLACEHOLDER,
        );
        let prompt_galley = elided_galley(
            ui,
            &prompt,
            FontId::proportional(11.0),
            content_gray(170, overlay_opacity),
            text_width,
        );
        ui.painter().galley(
            panel.min + Vec2::new(PADDING, PADDING + 20.0),
            prompt_galley,
            Color32::PLACEHOLDER,
        );

        let mut clicked = None;
        for (index, answer) in rows.iter().enumerate().skip(first).take(shown) {
            let row = Rect::from_min_size(
                egui::pos2(
                    panel.min.x + 4.0,
                    panel.min.y + HEADER + (index - first) as f32 * ANSWER_HEIGHT,
                ),
                Vec2::new(width - 8.0, ANSWER_HEIGHT),
            );
            let response = ui
                .interact(
                    row,
                    ui.id().with(("overlay-question-answer", index)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                clicked = Some(index);
            }
            let highlighted = index == self.question_row;
            if highlighted || response.hovered() {
                ui.painter().rect_filled(
                    row,
                    CornerRadius::same(3),
                    Color32::from_white_alpha(chrome_alpha(
                        if highlighted { 22 } else { 12 },
                        overlay_opacity,
                    )),
                );
            }
            if highlighted {
                ui.painter().rect_filled(
                    Rect::from_min_size(row.min, Vec2::new(3.0, row.height())),
                    CornerRadius::ZERO,
                    content_color(ACCENT, overlay_opacity),
                );
            }
            let cancel = answers[index] == Answer::Cancel;
            let mut label_width = row.width() - 20.0;
            if let Some(size) = &answer.size {
                let size_galley = ui.painter().layout_no_wrap(
                    size.clone(),
                    FontId::proportional(11.0),
                    content_gray(150, overlay_opacity),
                );
                label_width -= size_galley.size().x + 10.0;
                ui.painter().galley(
                    egui::pos2(row.max.x - 10.0 - size_galley.size().x, row.min.y + 6.0),
                    size_galley,
                    Color32::PLACEHOLDER,
                );
            }
            let label_galley = elided_galley(
                ui,
                &answer.label,
                FontId::proportional(12.5),
                content_gray(if cancel { 180 } else { 235 }, overlay_opacity),
                label_width,
            );
            let label_y = if answer.detail.is_some() {
                row.min.y + 4.0
            } else {
                row.center().y - label_galley.size().y * 0.5
            };
            ui.painter().galley(
                egui::pos2(row.min.x + 10.0, label_y),
                label_galley,
                Color32::PLACEHOLDER,
            );
            if let Some(detail) = &answer.detail {
                let detail_galley = elided_galley(
                    ui,
                    detail,
                    FontId::proportional(10.0),
                    content_gray(150, overlay_opacity),
                    row.width() - 20.0,
                );
                ui.painter().galley(
                    egui::pos2(row.min.x + 10.0, row.min.y + 20.0),
                    detail_galley,
                    Color32::PLACEHOLDER,
                );
            }
        }
        if let Some(index) = clicked {
            self.question_row = index;
            self.answer_question();
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn row(&self) -> Row {
        self.row
    }

    /// Up to the mods row for a negative direction, down to the categories row
    /// for a positive one.
    pub(super) fn switch_row(&mut self, direction: i32) {
        self.row = match direction.signum() {
            -1 => Row::Mods,
            1 => Row::Categories,
            _ => return,
        };
    }

    pub(super) fn reset_row(&mut self) {
        self.row = Row::default();
    }

    pub(super) fn shortcut_availability(&self) -> ShortcutAvailability {
        let categories = self.filter.categories.len() > 1;
        let Some(category) = self.catalog.categories.get(self.selected_category) else {
            return ShortcutAvailability {
                categories,
                ..ShortcutAvailability::default()
            };
        };
        let visible = self.visible_mods();
        let mods = visible.len() > 1;
        let Some(focused) = category
            .costumes
            .get(self.carousel_focus)
            .filter(|_| visible.contains(&self.carousel_focus))
        else {
            let space = self.gamebanana_space();
            return ShortcutAvailability {
                categories,
                mods,
                exclusive: space.is_some(),
                toggle: false,
                space: space.unwrap_or_default(),
                hotkeys: false,
            };
        };
        // Exclusive also disables mods the search hides, so count them all.
        // In Uncategorized it only enables.
        let active_count = category
            .costumes
            .iter()
            .filter(|costume| costume.active)
            .count();
        let loose = is_loose(category);
        ShortcutAvailability {
            categories,
            mods,
            exclusive: !(focused.active && (loose || active_count == 1)),
            toggle: true,
            space: if loose {
                Space::Enable
            } else {
                Space::Exclusive
            },
            hotkeys: self.keys.has(&focused.id),
        }
    }

    /// Show only what `query` matches.  Keeps the selected category and mod
    /// while the search still shows them, otherwise moves to the first match.
    pub(super) fn set_search(&mut self, query: &str) {
        if self.filter.query == query {
            return;
        }
        self.filter = Filter::new(&self.catalog, query, &self.gamebanana);
        self.reveal_category = true;
        self.carousel_transition = None;
        self.carousel_pointer_press = None;
        self.held_preview = None;
        if self.filter.categories.contains(&self.selected_category) {
            self.carousel_focus = self.visible_focus(self.selected_category, self.carousel_focus);
            if let Some(focus) = self
                .carousel_focus_by_category
                .get_mut(self.selected_category)
            {
                *focus = self.carousel_focus;
            }
        } else if let Some(&first) = self.filter.categories.first() {
            self.select_category(first);
        }
    }

    /// What the carousel says when it has no cards.  A search that matches
    /// nothing leaves the selection on a category it hides.
    fn empty_carousel_text(&self) -> &'static str {
        text(
            if self.filter.categories.contains(&self.selected_category) {
                TextKey::GameOverlayNoInstalledMods
            } else {
                TextKey::GameOverlayNoMatches
            },
        )
    }

    /// The selected category's mods that the search shows.
    fn visible_mods(&self) -> &[usize] {
        self.filter
            .mods
            .get(self.selected_category)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// `focus` when the search shows it in `category`, otherwise the first
    /// mod the search shows there.
    fn visible_focus(&self, category: usize, focus: usize) -> usize {
        match self.filter.mods.get(category) {
            Some(visible) if !visible.is_empty() && !visible.contains(&focus) => visible[0],
            _ => focus,
        }
    }

    /// Return the current painted card/preview bounds for the native click-through mask.
    pub(super) fn visible_card_rects(&self) -> &[Rect] {
        &self.visible_card_rects
    }

    fn pointer_gesture_active(&self) -> bool {
        self.held_preview.is_some() || self.carousel_pointer_press.is_some()
    }

    fn pointer_gesture_active_in_context(&self, ctx: &egui::Context) -> bool {
        self.pointer_gesture_active()
            || ctx.input(|input| input.pointer.any_down() || input.pointer.any_pressed())
    }

    /// Clear press/preview state and defer transient tooltip display after collapse or mode
    /// changes. Active costume state is deliberately untouched.
    pub(super) fn dismiss_transient_ui(&mut self, ctx: &egui::Context) {
        self.carousel_pointer_press = None;
        self.held_preview = None;
        self.visible_card_rects.clear();
        self.pending_commands.clear();
        self.keys.reset();
        suppress_tooltips(ctx);
    }

    /// Paint the original image held by the secondary mouse button. The preview contains the
    /// source image without cropping and marks only its actual image bounds.
    pub(super) fn show_held_preview(
        &mut self,
        ui: &mut Ui,
        available_rect: Rect,
        overlay_opacity: u8,
    ) {
        let Some(preview) = self.held_preview else {
            return;
        };
        if self
            .card(preview.category_index, preview.costume_index)
            .is_none()
        {
            self.held_preview = None;
            return;
        }
        let image_path = self.card_image(preview.category_index, preview.costume_index);
        let texture = self.texture_for(ui, image_path.as_deref());
        let area = available_rect.shrink(18.0);
        let image_rect = texture
            .as_ref()
            .map(|texture| fitted_image_rect(area, texture.size_vec2()))
            .unwrap_or(area);
        let backdrop = image_rect.expand(10.0).intersect(available_rect);
        self.visible_card_rects.clear();
        self.visible_card_rects.push(backdrop);
        // Keep the game visible through the preview. The margin is only a subtle boundary;
        // filling it would effectively stack a second opaque window behind the image.
        ui.painter().rect_stroke(
            backdrop,
            CornerRadius::ZERO,
            Stroke::new(1.0, content_black_alpha(120, overlay_opacity)),
            StrokeKind::Inside,
        );
        paint_thumbnail_tinted(
            ui,
            image_rect,
            texture.as_ref(),
            false,
            true,
            image_alpha(overlay_opacity),
        );
    }

    fn update_held_preview(
        &mut self,
        ui: &Ui,
        category_index: usize,
        placements: &[CarouselCardPlacement],
    ) {
        let (pressed_pos, released, down) = ui.input(|input| {
            let pressed_pos = input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            });
            (
                pressed_pos,
                input.pointer.secondary_released(),
                input.pointer.secondary_down(),
            )
        });
        if released || !down {
            self.held_preview = None;
        }
        if released || !down {
            return;
        }
        let Some(pointer_pos) = pressed_pos else {
            return;
        };
        let Some(card) = placements.iter().rev().find(|placement| {
            card_reveal_progress(placement, self.reveal_progress, placements)
                >= REVEAL_INTERACTION_THRESHOLD
                && placement.rect.contains(pointer_pos)
        }) else {
            return;
        };
        if matches!(self.card(category_index, card.index), Some(Card::Status(_))) {
            return;
        }
        self.held_preview = Some(HeldPreview {
            category_index,
            costume_index: card.index,
        });
    }

    fn capture_carousel_press(
        &mut self,
        ui: &Ui,
        category_index: usize,
        placements: &[CarouselCardPlacement],
    ) {
        let Some(pointer_pos) = ui.input(|input| {
            if !input.pointer.primary_pressed() {
                return None;
            }
            input
                .events
                .iter()
                .find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
                .or_else(|| input.pointer.press_origin())
        }) else {
            return;
        };
        if self.keys.owns_press(pointer_pos) {
            return;
        }
        let Some(card) = placements.iter().rev().find(|placement| {
            card_reveal_progress(placement, self.reveal_progress, placements)
                >= REVEAL_INTERACTION_THRESHOLD
                && placement.rect.contains(pointer_pos)
        }) else {
            return;
        };
        self.carousel_pointer_press = Some(CarouselPointerPress {
            category_index,
            costume_index: card.index,
            rect: card.rect,
            placements: placements.to_vec(),
        });
    }

    fn release_carousel_press(
        &mut self,
        ui: &Ui,
        category_index: usize,
        carousel_rect: Rect,
        now: f64,
    ) -> Option<usize> {
        if !ui.input(|input| input.pointer.primary_released()) {
            return None;
        }
        let press = self.carousel_pointer_press.take()?;
        let pointer_pos = ui.input(|input| {
            input
                .pointer
                .interact_pos()
                .or_else(|| input.pointer.latest_pos())
        });
        let can_click = ui.input(|input| input.pointer.could_any_button_be_click());
        // Resume from the exact geometry shown while the button was held. This prevents a
        // long-held press from snapping to a transition's settled destination on release.
        self.begin_carousel_transition(press.placements.clone(), carousel_rect, now);
        valid_carousel_release(&press, category_index, pointer_pos, can_click)
    }

    pub(super) fn cancel_pointer_interaction(&mut self) {
        self.carousel_pointer_press = None;
    }

    fn current_carousel_placements(
        &mut self,
        carousel_rect: Rect,
        now: f64,
    ) -> Vec<CarouselCardPlacement> {
        if let Some(press) = &self.carousel_pointer_press {
            // The pointer target owns the geometry until release, even if the original
            // transition would have completed while the button was held.
            return press.placements.clone();
        }
        let target = self.target_placements(carousel_rect);
        let Some(transition) = self.carousel_transition.clone() else {
            return target;
        };
        let progress = ((now - transition.started_at) / transition.seconds).clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.carousel_transition = None;
            return target;
        }

        let eased = (progress * progress * (3.0 - 2.0 * progress)) as f32;
        let mut current = Vec::with_capacity(target.len());
        for target_card in target {
            let from_card = transition
                .from
                .iter()
                .find(|card| card.index == target_card.index)
                .copied()
                .unwrap_or(target_card);
            current.push(CarouselCardPlacement {
                index: target_card.index,
                rect: lerp_rect(from_card.rect, target_card.rect, eased),
                focused: target_card.focused,
                slot: target_card.slot,
            });
        }
        current
    }

    /// Where the cards settle.  GameBanana cards sit a divider's width away
    /// from the category's own.
    fn target_placements(&self, carousel_rect: Rect) -> Vec<CarouselCardPlacement> {
        // A linked category's GameBanana cards come as its list loads.
        let grows =
            self.gamebanana.enabled() && self.gamebanana_key(self.selected_category).is_some();
        let mut placements = visible_card_placements(
            carousel_rect,
            self.visible_mods(),
            self.carousel_focus,
            grows,
        );
        let own = self
            .catalog
            .categories
            .get(self.selected_category)
            .map_or(0, |category| category.costumes.len());
        if own > 0 {
            let focus_is_own = self.carousel_focus < own;
            for placement in &mut placements {
                if focus_is_own && placement.index >= own {
                    placement.rect = placement.rect.translate(Vec2::new(DIVIDER_ZONE, 0.0));
                } else if !focus_is_own && placement.index < own {
                    placement.rect = placement.rect.translate(Vec2::new(-DIVIDER_ZONE, 0.0));
                }
            }
        }
        placements
    }

    fn begin_carousel_transition(
        &mut self,
        from: Vec<CarouselCardPlacement>,
        carousel_rect: Rect,
        now: f64,
    ) {
        let to = self.target_placements(carousel_rect);
        if same_card_geometry(&from, &to) {
            self.carousel_transition = None;
        } else {
            self.carousel_transition = Some(CarouselTransition {
                from,
                started_at: now,
                seconds: CAROUSEL_TRANSITION_SECS,
            });
        }
    }

    fn consume_pending_commands(&mut self, ctx: &egui::Context, carousel_rect: Rect, now: f64) {
        while let Some(command) = self.pending_commands.pop_front() {
            if self.pointer_gesture_active_in_context(ctx) {
                continue;
            }
            match command {
                PendingCommand::Category(direction) => {
                    if let Some(index) = next_visible_category(
                        &self.filter.categories,
                        self.selected_category,
                        direction,
                    ) {
                        self.select_category(index);
                    }
                }
                PendingCommand::Mod(direction) => {
                    self.advance_mod_focus(carousel_rect, direction, now);
                }
                PendingCommand::Action(action) => {
                    if self.reveal_progress < REVEAL_INTERACTION_THRESHOLD {
                        self.pending_commands
                            .push_front(PendingCommand::Action(action));
                        break;
                    }
                    self.apply_focused_costume_action(action, now);
                }
                PendingCommand::Flip => {
                    if let Some(mod_id) = self.focused_mod_id() {
                        self.keys.toggle(&mod_id, now);
                    }
                }
            }
        }
    }

    fn advance_mod_focus(&mut self, carousel_rect: Rect, direction: i32, now: f64) {
        if direction == 0 || self.pointer_gesture_active() {
            return;
        }
        let Some(next_focus) =
            next_visible_mod(self.visible_mods(), self.carousel_focus, direction)
        else {
            return;
        };
        let from = self.current_carousel_placements(carousel_rect, now);
        if next_focus == self.carousel_focus {
            self.boundary_feedback_until = now + ACTIVE_FEEDBACK_SECS;
            self.boundary_feedback_edge = Some(direction.signum());
        } else {
            self.boundary_feedback_until = f64::NEG_INFINITY;
            self.boundary_feedback_edge = None;
        }
        self.carousel_focus = next_focus;
        if let Some(focus) = self
            .carousel_focus_by_category
            .get_mut(self.selected_category)
        {
            *focus = next_focus;
        }
        self.begin_carousel_transition(from, carousel_rect, now);
    }

    fn request_animation_repaint(&self, ctx: &egui::Context, now: f64) {
        let transition_remaining = self
            .carousel_transition
            .as_ref()
            .map(|transition| (transition.seconds - (now - transition.started_at)).max(0.0));
        let feedback_remaining = (self.active_feedback_until - now).max(0.0);
        let boundary_remaining = (self.boundary_feedback_until - now).max(0.0);
        let remaining = transition_remaining
            .unwrap_or(0.0)
            .max(feedback_remaining)
            .max(boundary_remaining);
        if remaining > 0.0 || self.keys.turning(now) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining.min(0.016)));
        }
    }

    pub(super) fn animation_pending(&self, now: f64) -> bool {
        self.carousel_transition
            .as_ref()
            .is_some_and(|transition| now - transition.started_at < transition.seconds)
            || now < self.active_feedback_until
            || now < self.boundary_feedback_until
            || self.keys.turning(now)
    }

    fn activate_costume_in_place(&mut self, category_index: usize, costume_index: usize, now: f64) {
        if !matches!(self.card(category_index, costume_index), Some(Card::Own(_))) {
            self.activate_gamebanana(category_index, costume_index, true);
            return;
        }
        if let Some(category) = self.catalog.categories.get_mut(category_index) {
            let request = select_costume(category, costume_index);
            self.active_images[category_index] = active_image(category);
            if let Some(request) = request {
                self.ask(request);
            }
        }
        self.forget_used_new_mods();
        self.active_feedback_until = now + ACTIVE_FEEDBACK_SECS;
        self.active_feedback_costume = Some(costume_index);
    }

    /// Whether a click on a card comes too soon after the last one on it to
    /// be a click of its own, as in a double-click.  Every click counts.
    fn repeated_click(&mut self, category_index: usize, index: usize, now: f64) -> bool {
        let id = self.card_id(category_index, index);
        let repeated = matches!(
            (&self.last_card_click, &id),
            (Some((last, at)), Some(id)) if last == id && now - at < DOUBLE_CLICK_SECS
        );
        self.last_card_click = id.map(|id| (id, now));
        repeated
    }

    /// Draw the horizontally scrollable category strip. The parent paints its neutral base.
    pub(super) fn show_category_strip(&mut self, ui: &mut Ui, overlay_opacity: u8) {
        if self.catalog.categories.is_empty() {
            return;
        }

        self.clamp_selection();
        let visible_indices = self.filter.categories.clone();
        let strip_rect = Rect::from_min_size(
            ui.cursor().min,
            Vec2::new(
                ui.available_width().max(1.0),
                CATEGORY_STRIP_HEIGHT.min(ui.available_height().max(1.0)),
            ),
        );
        if let Some(index) = self.consume_category_wheel(ui, strip_rect, &visible_indices) {
            self.select_category(index);
        }

        let rail_rect = strip_rect.shrink2(Vec2::new(12.0, 0.0));
        // Let the category rail run beneath the navigation controls. The controls are
        // painted over the rail below, so reserving side gutters would leave an
        // unnecessary gap before the first and after the last category.
        let scroll_rect = rail_rect;
        let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(scroll_rect));
        egui::ScrollArea::horizontal()
            .id_salt("overlay-preview-category-strip")
            .auto_shrink([false, false])
            .show(&mut content_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(CATEGORY_ITEM_HEIGHT);
                    let mut divided = false;
                    for (position, &index) in visible_indices.iter().enumerate() {
                        // A line sets the added characters apart from the
                        // library's categories.
                        if self.catalog.categories[index].extra && !divided {
                            divided = true;
                            if position > 0 {
                                let (line, _) = ui.allocate_exact_size(
                                    Vec2::new(1.0, CATEGORY_ITEM_HEIGHT),
                                    Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    Rect::from_center_size(line.center(), Vec2::new(1.0, 48.0)),
                                    CornerRadius::ZERO,
                                    white_alpha(60, overlay_opacity),
                                );
                                ui.add_space(CATEGORY_GAP);
                            }
                        }
                        let selected = self.selected_category == index;
                        self.show_category_item(ui, index, selected, overlay_opacity);
                        ui.add_space(CATEGORY_GAP);
                    }
                });
            });
        let left_enabled =
            next_visible_category(&visible_indices, self.selected_category, -1).is_some();
        let right_enabled =
            next_visible_category(&visible_indices, self.selected_category, 1).is_some();
        self.show_category_nav_button(
            ui,
            Rect::from_min_size(
                egui::pos2(strip_rect.min.x + 4.0, rail_rect.min.y + 9.0),
                Vec2::new(CATEGORY_NAV_WIDTH, 28.0),
            ),
            -1,
            'Z',
            left_enabled,
            &visible_indices,
            overlay_opacity,
        );
        self.show_category_nav_button(
            ui,
            Rect::from_min_size(
                egui::pos2(
                    strip_rect.max.x - CATEGORY_NAV_WIDTH - 4.0,
                    rail_rect.min.y + 9.0,
                ),
                Vec2::new(CATEGORY_NAV_WIDTH, 28.0),
            ),
            1,
            'C',
            right_enabled,
            &visible_indices,
            overlay_opacity,
        );
    }

    fn show_category_nav_button(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        direction: i32,
        shortcut: char,
        enabled: bool,
        visible_indices: &[usize],
        overlay_opacity: u8,
    ) {
        // At either end, leave the category beneath the unavailable control unobscured.
        if !enabled {
            return;
        }
        let sense = Sense::click();
        let response = ui
            .interact(
                rect,
                ui.id().with(("overlay-preview-category-nav", direction)),
                sense,
            )
            .on_hover_cursor(if enabled {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Default
            });
        let response = delayed_tooltip(
            response,
            text(if direction < 0 {
                TextKey::GameOverlayPreviousCategory
            } else {
                TextKey::GameOverlayNextCategory
            }),
        );
        let hover = ui.ctx().animate_bool_with_time(
            response.id.with("hover"),
            enabled && response.hovered(),
            0.12,
        );
        let pressed = ui.ctx().animate_bool_with_time(
            response.id.with("pressed"),
            enabled && response.is_pointer_button_down_on(),
            0.06,
        );
        let shade = (24.0 + 14.0 * hover - 12.0 * pressed).round() as u8;
        let alpha = if enabled {
            (224.0 + 12.0 * hover).round() as u8
        } else {
            180
        };
        let button_fill = rgba_alpha(shade, shade, shade, alpha, overlay_opacity);
        let visual_rect = rect;
        ui.painter()
            .rect_filled(visual_rect, CornerRadius::same(8), button_fill);
        ui.painter().rect_stroke(
            visual_rect,
            CornerRadius::same(8),
            Stroke::new(
                1.0,
                rgba_alpha(
                    190,
                    190,
                    190,
                    (56.0 + 24.0 * hover).round() as u8,
                    overlay_opacity,
                ),
            ),
            StrokeKind::Inside,
        );
        if enabled && response.clicked() {
            if let Some(index) =
                next_visible_category(visible_indices, self.selected_category, direction)
            {
                self.select_category(index);
                ui.ctx().request_repaint();
            }
        }

        let emphasis = if enabled { 1.0 } else { 0.38 };
        let arrow_color = scale_color_alpha(
            content_gray((220.0 + 30.0 * hover).round() as u8, overlay_opacity),
            emphasis,
        );
        let shortcut_color = scale_color_alpha(
            content_gray((164.0 + 20.0 * hover).round() as u8, overlay_opacity),
            emphasis,
        );
        let center = visual_rect.center();
        let icon = char::from(if direction < 0 {
            lucide_icons::Icon::ChevronLeft
        } else {
            lucide_icons::Icon::ChevronRight
        })
        .to_string();
        let (icon_pos, shortcut_pos) = if direction < 0 {
            (center.x - 8.0, center.x + 8.0)
        } else {
            (center.x + 8.0, center.x - 8.0)
        };
        ui.painter().text(
            egui::pos2(shortcut_pos, center.y),
            Align2::CENTER_CENTER,
            shortcut.to_string(),
            FontId::proportional(10.0),
            shortcut_color,
        );
        ui.painter().text(
            egui::pos2(icon_pos, center.y),
            Align2::CENTER_CENTER,
            icon,
            FontId::new(16.0, egui::FontFamily::Name("preview-icons".into())),
            arrow_color,
        );
    }

    fn show_empty_catalog(&mut self, ui: &mut Ui, overlay_opacity: u8) -> Vec<Rect> {
        let mut visible_rects = Vec::new();
        ui.vertical_centered(|ui| {
            ui.add_space(68.0);
            let title = ui.label(
                RichText::new(text(TextKey::GameOverlayNoInstalledMods))
                    .size(16.0)
                    .strong()
                    .color(content_gray(255, overlay_opacity)),
            );
            visible_rects.push(title.rect.expand(2.0));
            ui.add_space(5.0);
            let note = ui.add(
                Label::new(
                    RichText::new(
                        self.catalog
                            .note
                            .as_deref()
                            .unwrap_or(text(TextKey::GameOverlayInstallToSee)),
                    )
                    .size(12.0)
                    .color(content_gray(160, overlay_opacity)),
                )
                .wrap(),
            );
            visible_rects.push(note.rect.expand(2.0));
        });
        visible_rects
    }

    fn clamp_selection(&mut self) {
        if self.catalog.categories.is_empty() {
            self.selected_category = 0;
            self.carousel_focus = 0;
            return;
        }
        self.selected_category = self
            .selected_category
            .min(self.catalog.categories.len().saturating_sub(1));
        let count = self.card_count(self.selected_category);
        self.carousel_focus = self.carousel_focus.min(count.saturating_sub(1));
    }

    fn select_category(&mut self, index: usize) {
        if index >= self.catalog.categories.len() || self.selected_category == index {
            return;
        }
        if let Some(previous_focus) = self
            .carousel_focus_by_category
            .get_mut(self.selected_category)
        {
            *previous_focus = self.carousel_focus;
        }
        self.selected_category = index;
        self.carousel_focus = self
            .carousel_focus_by_category
            .get(index)
            .copied()
            .unwrap_or_else(|| active_costume_index(&self.catalog.categories[index]));
        self.carousel_focus = self
            .carousel_focus
            .min(self.card_count(index).saturating_sub(1));
        self.carousel_focus = self.visible_focus(index, self.carousel_focus);
        self.reveal_category = true;
        self.carousel_transition = None;
        self.carousel_pointer_press = None;
        self.held_preview = None;
        self.visible_card_rects.clear();
        self.active_feedback_until = f64::NEG_INFINITY;
        self.active_feedback_costume = None;
        self.boundary_feedback_until = f64::NEG_INFINITY;
        self.boundary_feedback_edge = None;
    }

    fn show_category_item(
        &mut self,
        ui: &mut Ui,
        index: usize,
        selected: bool,
        overlay_opacity: u8,
    ) {
        let category_name = self.catalog.categories[index].name.clone();
        // A GameBanana character without a category shows faded, as a ghost.
        let extra = self.catalog.categories[index].extra;
        let picture_alpha = if extra {
            scaled_alpha(image_alpha(overlay_opacity), 0.5)
        } else {
            image_alpha(overlay_opacity)
        };
        let name = character_name(&category_name).to_owned();
        let image = self.catalog.categories[index]
            .image
            .clone()
            .or_else(|| self.active_images[index].clone());
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(CATEGORY_ITEM_WIDTH, CATEGORY_ITEM_HEIGHT),
            Sense::click(),
        );
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            self.select_category(index);
            ui.ctx().request_repaint();
        }
        if selected && self.reveal_category {
            ui.scroll_to_rect(rect, Some(Align::Center));
            self.reveal_category = false;
        }
        if !ui.is_rect_visible(rect) {
            return;
        }

        let fill = if selected {
            rgba_alpha(65, 65, 65, 92, overlay_opacity)
        } else if response.hovered() {
            white_alpha(15, overlay_opacity)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
        if selected && self.row == Row::Categories {
            // The keyboard is on this row.  The focused card dims to match.
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(4),
                Stroke::new(1.5, content_gray(232, overlay_opacity)),
                StrokeKind::Inside,
            );
        }

        if selected {
            let image_rect = Rect::from_min_size(
                egui::pos2(
                    rect.center().x - CATEGORY_SELECTED_SPRITE_SIZE * 0.5,
                    rect.min.y + 2.0,
                ),
                Vec2::splat(CATEGORY_SELECTED_SPRITE_SIZE),
            );
            let texture = self.texture_for(ui, image.as_deref());
            paint_thumbnail_tinted(
                ui,
                image_rect,
                texture.as_ref(),
                false,
                false,
                picture_alpha,
            );
            if extra {
                paint_dashed_rect(
                    ui,
                    image_rect.expand(2.0),
                    Stroke::new(1.0, content_gray(154, overlay_opacity)),
                );
            }
            let truncated = paint_category_name(
                ui,
                Rect::from_min_max(
                    egui::pos2(rect.min.x + 3.0, rect.min.y + 42.0),
                    egui::pos2(rect.max.x - 3.0, rect.max.y - 3.0),
                ),
                &name,
                FontId::proportional(10.0),
                content_gray(242, overlay_opacity),
            );
            if truncated {
                let _ = delayed_tooltip(response, name.clone());
            }
            ui.painter().rect_filled(
                Rect::from_min_max(
                    egui::pos2(rect.min.x + 10.0, rect.max.y - 2.0),
                    egui::pos2(rect.max.x - 10.0, rect.max.y),
                ),
                CornerRadius::same(1),
                content_rgba(223, 119, 73, 220, overlay_opacity),
            );
        } else {
            let image_rect = Rect::from_min_size(
                egui::pos2(
                    rect.center().x - CATEGORY_SPRITE_SIZE * 0.5,
                    rect.min.y + 8.0,
                ),
                Vec2::splat(CATEGORY_SPRITE_SIZE),
            );
            let texture = self.texture_for(ui, image.as_deref());
            paint_thumbnail_tinted(
                ui,
                image_rect,
                texture.as_ref(),
                false,
                false,
                picture_alpha,
            );
            if extra {
                paint_dashed_rect(
                    ui,
                    image_rect.expand(2.0),
                    Stroke::new(1.0, content_gray(154, overlay_opacity)),
                );
            }
            let truncated = paint_category_name(
                ui,
                Rect::from_min_max(
                    egui::pos2(rect.min.x + 3.0, rect.min.y + 42.0),
                    egui::pos2(rect.max.x - 3.0, rect.max.y - 3.0),
                ),
                &name,
                FontId::proportional(10.0),
                content_gray(if extra { 150 } else { 220 }, overlay_opacity),
            );
            if truncated {
                let _ = delayed_tooltip(response, name.clone());
            }
        }
        // A dot shows where a mod the overlay installed went.
        if self.catalog.categories[index]
            .costumes
            .iter()
            .any(|costume| self.new_mods.contains(&costume.id))
        {
            // A dark ring keeps it apart from the picture.
            ui.painter().circle(
                egui::pos2(rect.max.x - 9.0, rect.min.y + 8.0),
                4.5,
                content_color(ACCENT, overlay_opacity),
                Stroke::new(1.5, content_gray(47, overlay_opacity)),
            );
        }
    }

    /// Whether the library's mod `id` is on.
    fn mod_active(&self, id: &str) -> bool {
        self.catalog
            .categories
            .iter()
            .flat_map(|category| &category.costumes)
            .any(|costume| costume.id == id && costume.active)
    }

    /// Mods tagged NEW lose the tag once they're used.
    fn forget_used_new_mods(&mut self) {
        if self.new_mods.is_empty() {
            return;
        }
        let used: HashSet<&str> = self
            .catalog
            .categories
            .iter()
            .flat_map(|category| &category.costumes)
            .filter(|costume| costume.active)
            .map(|costume| costume.id.as_str())
            .collect();
        self.new_mods.retain(|id| !used.contains(id.as_str()));
    }

    fn show_carousel_card(
        &mut self,
        ui: &mut Ui,
        category_index: usize,
        costume_index: usize,
        rect: Rect,
        focused: bool,
        slot: usize,
        reveal: f32,
        overlay_opacity: u8,
    ) {
        let (name, image, active, waiting, from_gamebanana, install) =
            match self.card(category_index, costume_index) {
                Some(Card::Own(costume)) => (
                    costume.name.clone(),
                    costume.image.clone(),
                    costume.active,
                    self.waiting
                        .as_ref()
                        .is_some_and(|waiting| waiting.contains(&costume.id)),
                    false,
                    None,
                ),
                Some(Card::GameBanana(item)) => (
                    item.name.clone(),
                    item.image.clone(),
                    false,
                    false,
                    true,
                    self.gamebanana
                        .install_state(item.id)
                        .map(|install| install.stage.clone()),
                ),
                Some(Card::Status(status)) => {
                    self.show_status_card(
                        ui,
                        category_index,
                        status,
                        rect,
                        focused,
                        slot,
                        reveal,
                        overlay_opacity,
                    );
                    return;
                }
                None => return,
            };
        let new = !active
            && matches!(
                self.card(category_index, costume_index),
                Some(Card::Own(costume)) if self.new_mods.contains(&costume.id)
            );
        // A finished install glows until its card crosses.
        let installed = matches!(install, Some(InstallStage::Installed { .. }));
        let interactable = reveal >= REVEAL_INTERACTION_THRESHOLD;
        let response = ui
            .interact(
                rect,
                ui.id().with(("overlay-preview-carousel-card-slot", slot)),
                if interactable {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            )
            .on_hover_cursor(if interactable {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Default
            });

        let hovered = interactable && response.hovered();
        let card_id = response.id;
        let now = ui.input(|input| input.time);
        let border = if installed {
            content_color(ACCENT, overlay_opacity)
        } else if focused && self.row == Row::Mods {
            content_gray(232, overlay_opacity)
        } else if focused || hovered {
            content_gray(180, overlay_opacity)
        } else if active {
            content_gray(132, overlay_opacity)
        } else {
            content_gray(82, overlay_opacity)
        };
        // The focused card of a library mod can turn over to its hotkeys.
        let own_focused = match self.card(category_index, costume_index) {
            Some(Card::Own(costume)) if focused => Some(costume.id.clone()),
            _ => None,
        };
        let face = own_focused
            .as_deref()
            .map_or(Face::FRONT, |mod_id| self.keys.face(mod_id, now));
        let full_rect = rect.translate(Vec2::new(0.0, (1.0 - reveal) * REVEAL_OFFSET));
        let visual_rect = card_keys::narrowed(full_rect, face.width);
        if reveal > 0.01 {
            self.visible_card_rects.push(visual_rect);
        }
        let settled = interactable && face.settled();
        if let Some(mod_id) = own_focused.as_deref().filter(|_| face.back) {
            let button = settled.then(|| {
                self.keys
                    .keys_button(ui, full_rect, card_id.with("keys"), true, overlay_opacity)
            });
            let clicked = self.keys.paint_back(
                ui,
                full_rect,
                visual_rect,
                mod_id,
                &name,
                Stroke::new(1.5, border),
                reveal,
                overlay_opacity,
                button,
            );
            if clicked || (settled && response.clicked()) {
                self.keys.toggle(mod_id, now);
                ui.ctx().request_repaint();
            }
            return;
        }
        let has_keys = own_focused
            .as_deref()
            .is_some_and(|mod_id| self.keys.has(mod_id));
        let keys_button = (settled && has_keys).then(|| {
            self.keys.keys_button(
                ui,
                visual_rect,
                card_id.with("keys"),
                false,
                overlay_opacity,
            )
        });
        let image_rect = visual_rect.shrink(1.0);
        let texture = self.texture_for(ui, image.as_deref());
        if !face.settled() {
            // Turning, only the picture shows.
            paint_thumbnail_squeezed(
                ui,
                full_rect.shrink(1.0),
                image_rect,
                texture.as_ref(),
                scaled_alpha(image_alpha(overlay_opacity), reveal),
            );
            ui.painter().rect_stroke(
                visual_rect.shrink(0.5),
                CornerRadius::ZERO,
                Stroke::new(1.5, scale_color_alpha(border, reveal)),
                StrokeKind::Inside,
            );
            return;
        }
        // How the install goes, on a line under the name.
        let status = install
            .as_ref()
            .and_then(|stage| install_status(stage, focused))
            .map(|status| {
                let color = scale_color_alpha(content_color(status.color, overlay_opacity), reveal);
                let galley = ui.painter().layout(
                    status.text,
                    FontId::proportional(if focused { 10.5 } else { 9.5 }),
                    color,
                    image_rect.width() - 16.0,
                );
                (galley, color, status.bar)
            });
        let status_height = status
            .as_ref()
            .map_or(0.0, |(galley, _, _)| galley.size().y + 3.0);
        let name_rect =
            card_name_rect(image_rect, focused).translate(Vec2::new(0.0, -status_height));
        let (name_size, _) = measure_card_name(ui, name_rect, &name, overlay_opacity, focused);
        if installed {
            // A soft orange glow until the card crosses.
            let glow = egui::epaint::Shadow {
                offset: [0, 0],
                blur: 18,
                spread: 0,
                color: scale_color_alpha(
                    Color32::from_rgba_unmultiplied(
                        232,
                        116,
                        59,
                        content_alpha(140, overlay_opacity),
                    ),
                    reveal,
                ),
            };
            ui.painter()
                .add(glow.as_shape(visual_rect, CornerRadius::ZERO));
            if reveal > 0.01 {
                self.visible_card_rects.push(visual_rect.expand(14.0));
            }
        }
        paint_thumbnail_tinted(
            ui,
            image_rect,
            texture.as_ref(),
            true,
            false,
            scaled_alpha(image_alpha(overlay_opacity), reveal),
        );
        paint_bottom_gradient(
            ui,
            image_rect,
            overlay_opacity,
            name_size.y + status_height + CARD_TITLE_BOTTOM_PADDING + 8.0,
            reveal,
        );
        ui.painter().rect_stroke(
            visual_rect.shrink(0.5),
            CornerRadius::ZERO,
            Stroke::new(
                if installed {
                    2.0
                } else if active || focused {
                    1.5
                } else {
                    1.0
                },
                scale_color_alpha(border, reveal),
            ),
            StrokeKind::Inside,
        );
        if focused && now < self.boundary_feedback_until && self.boundary_feedback_edge.is_some() {
            let edge_x = if self.boundary_feedback_edge == Some(-1) {
                visual_rect.min.x + 3.0
            } else {
                visual_rect.max.x - 3.0
            };
            ui.painter().line_segment(
                [
                    egui::pos2(edge_x, visual_rect.min.y + 10.0),
                    egui::pos2(edge_x, visual_rect.max.y - 10.0),
                ],
                Stroke::new(1.5, content_gray(180, overlay_opacity)),
            );
        }

        if active || waiting {
            let feedback = now < self.active_feedback_until
                && self.active_feedback_costume == Some(costume_index);
            if feedback {
                self.request_animation_repaint(ui.ctx(), now);
            }
            // While Hestia makes the change, the badge pulses with dots.
            let pulse = if waiting {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(33));
                0.6 + 0.4 * (now * WAITING_PULSE_SPEED).sin().abs() as f32
            } else {
                1.0
            };
            let badge_rect =
                Rect::from_min_size(visual_rect.min + Vec2::new(8.0, 8.0), Vec2::splat(21.0));
            ui.painter().rect_filled(
                badge_rect,
                CornerRadius::ZERO,
                scale_color_alpha(
                    Color32::from_black_alpha(chrome_alpha(
                        if feedback { 215 } else { 185 },
                        overlay_opacity,
                    )),
                    reveal,
                ),
            );
            let icon = if waiting {
                lucide_icons::Icon::Ellipsis
            } else {
                lucide_icons::Icon::Check
            };
            ui.painter().text(
                badge_rect.center(),
                Align2::CENTER_CENTER,
                char::from(icon).to_string(),
                FontId::new(14.0, egui::FontFamily::Name("preview-icons".into())),
                scale_color_alpha(content_color(ACCENT, overlay_opacity), reveal * pulse),
            );
        }
        if from_gamebanana {
            paint_gamebanana_badge(ui, visual_rect, focused, installed, reveal, overlay_opacity);
        }
        if new {
            // The keys button's label covers the tag while it's out.
            let (room, shown) = keys_button.as_ref().map_or((0.0, 1.0), |button| {
                (card_keys::BUTTON_ROOM, 1.0 - button.open)
            });
            paint_new_tag(ui, visual_rect, room, reveal * shown, overlay_opacity);
        }
        if let Some((galley, color, bar)) = status {
            let position = egui::pos2(
                name_rect.min.x,
                image_rect.max.y - CARD_TITLE_BOTTOM_PADDING - galley.size().y,
            );
            ui.painter().galley(position, galley, color);
            paint_install_bar(ui, image_rect, bar, now, reveal, overlay_opacity);
        }

        if reveal > 0.01 {
            let truncated =
                paint_card_name_revealed(ui, name_rect, &name, overlay_opacity, focused, reveal);
            if truncated && interactable {
                let _ = delayed_tooltip(response, clean_display_name(&name));
            }
        }
        if let Some(button) = keys_button
            && self.keys.button(ui, button, overlay_opacity)
            && let Some(mod_id) = own_focused
        {
            self.keys.toggle(&mod_id, now);
            ui.ctx().request_repaint();
        }
    }

    /// The card after a GameBanana list: a placeholder while it loads, or
    /// why the list has no more mods.
    #[allow(clippy::too_many_arguments)]
    fn show_status_card(
        &mut self,
        ui: &mut Ui,
        category_index: usize,
        status: StatusCard,
        rect: Rect,
        focused: bool,
        slot: usize,
        reveal: f32,
        overlay_opacity: u8,
    ) {
        let interactable = reveal >= REVEAL_INTERACTION_THRESHOLD;
        let retry = interactable && status == StatusCard::Failed;
        let response = ui
            .interact(
                rect,
                ui.id().with(("overlay-preview-carousel-card-slot", slot)),
                if interactable {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            )
            .on_hover_cursor(if retry {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Default
            });
        let hovered = retry && response.hovered();
        let visual_rect = rect.translate(Vec2::new(0.0, (1.0 - reveal) * REVEAL_OFFSET));
        if reveal > 0.01 {
            self.visible_card_rects.push(visual_rect);
        }
        let fill = if status == StatusCard::Loading {
            let now = ui.input(|input| input.time);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
            (70.0 + 40.0 * (0.5 + 0.5 * (now * 2.5).sin())) as u8
        } else {
            70
        };
        ui.painter().rect_filled(
            visual_rect,
            CornerRadius::ZERO,
            scale_color_alpha(rgba_alpha(65, 65, 65, fill, overlay_opacity), reveal),
        );
        let border = if focused && self.row == Row::Mods {
            content_gray(232, overlay_opacity)
        } else if focused || hovered {
            content_gray(180, overlay_opacity)
        } else {
            content_gray(82, overlay_opacity)
        };
        ui.painter().rect_stroke(
            visual_rect.shrink(0.5),
            CornerRadius::ZERO,
            Stroke::new(
                if focused { 1.5 } else { 1.0 },
                scale_color_alpha(border, reveal),
            ),
            StrokeKind::Inside,
        );
        let text = match status {
            StatusCard::Loading => {
                // Where the name will be.
                let bottom = visual_rect.max.y - CARD_TITLE_BOTTOM_PADDING;
                let height = if focused { 9.0 } else { 7.0 };
                let width = visual_rect.width() - 24.0;
                for (line, share) in [(1.0, 0.8), (0.0, 0.5)] {
                    let bar = Rect::from_min_size(
                        egui::pos2(
                            visual_rect.min.x + 12.0,
                            bottom - height - line * (height + 6.0),
                        ),
                        Vec2::new(width * share, height),
                    );
                    ui.painter().rect_filled(
                        bar,
                        CornerRadius::same(2),
                        scale_color_alpha(white_alpha(34, overlay_opacity), reveal),
                    );
                }
                return;
            }
            StatusCard::Failed => text(TextKey::GameOverlayGameBananaFailed).to_owned(),
            StatusCard::Empty => {
                if self
                    .gamebanana_key(category_index)
                    .is_some_and(|key| key.query.is_empty())
                {
                    text(TextKey::GameOverlayGameBananaNothingFor).replace(
                        "{name}",
                        character_name(&self.catalog.categories[category_index].name),
                    )
                } else {
                    text(TextKey::GameOverlayGameBananaNoMatches).to_owned()
                }
            }
        };
        let color = scale_color_alpha(content_gray(200, overlay_opacity), reveal);
        let mut job = egui::text::LayoutJob::simple(
            text,
            FontId::proportional(if focused { 13.0 } else { 11.0 }),
            color,
            visual_rect.width() - 24.0,
        );
        job.halign = Align::Center;
        let galley = ui.painter().layout_job(job);
        let position = egui::pos2(
            visual_rect.center().x,
            visual_rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(position, galley, color);
    }

    /// The line before a category's GameBanana mods, while the first one
    /// shows: between them and the category's own, or first in the row when
    /// none of its own show.
    fn paint_gamebanana_divider(
        &mut self,
        ui: &Ui,
        placements: &[CarouselCardPlacement],
        carousel_rect: Rect,
        overlay_opacity: u8,
    ) {
        let Some(category) = self.catalog.categories.get(self.selected_category) else {
            return;
        };
        let own = category.costumes.len();
        let visible = self.visible_mods();
        let Some(&first) = visible.iter().find(|&&index| index >= own) else {
            return;
        };
        let Some(card) = placements.iter().find(|placement| placement.index == first) else {
            return;
        };
        let x = card.rect.min.x - (CAROUSEL_CARD_GAP + DIVIDER_ZONE) * 0.5;
        let center_y = carousel_rect.center().y;
        let half = (NEIGHBOR_CARD_SIZE.y - 12.0) * 0.5;
        let reveal = self.reveal_progress;
        // A dark backing like the header's, so the line reads over the game.
        // The window shows only what's listed, and the gap between cards
        // isn't.
        let backing = Rect::from_min_max(
            egui::pos2(x - 6.0, center_y - half - 6.0),
            egui::pos2(x + 20.0, center_y + half + 6.0),
        );
        ui.painter().rect_filled(
            backing,
            4,
            scale_color_alpha(
                Color32::from_rgba_unmultiplied(32, 32, 32, super::base_alpha(overlay_opacity)),
                reveal,
            ),
        );
        if reveal > 0.01 {
            self.visible_card_rects.push(backing);
        }
        ui.painter().line_segment(
            [
                egui::pos2(x, center_y - half),
                egui::pos2(x, center_y + half),
            ],
            Stroke::new(
                1.0,
                scale_color_alpha(
                    Color32::from_white_alpha(content_alpha(71, overlay_opacity)),
                    reveal,
                ),
            ),
        );
        let color = scale_color_alpha(content_gray(200, overlay_opacity), reveal);
        let galley = ui
            .painter()
            .layout_job(egui::text::LayoutJob::single_section(
                "GAMEBANANA".to_owned(),
                egui::TextFormat {
                    font_id: FontId::proportional(10.0),
                    color,
                    extra_letter_spacing: 1.5,
                    ..Default::default()
                },
            ));
        // Turned to read upwards, the label's top edge faces the line.
        let position = egui::pos2(x + 4.0, center_y + galley.size().x * 0.5);
        ui.painter().add(
            egui::epaint::TextShape::new(position, galley, color)
                .with_angle(-std::f32::consts::FRAC_PI_2),
        );
    }

    fn consume_carousel_wheel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        card_count: usize,
    ) -> Option<i32> {
        if card_count == 0 {
            return None;
        }
        let hovered = ui.rect_contains_pointer(rect);
        let now = ui.input(|input| input.time);
        let gesture_blocked = self.pointer_gesture_active();
        let mut direction = None;
        let mut consumed_wheel = false;
        ui.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    phase,
                } if hovered && !gesture_blocked && !modifiers.ctrl && !modifiers.command => {
                    consumed_wheel = true;
                    if *phase == egui::TouchPhase::Move {
                        self.carousel_wheel_accum += wheel_units(*unit, delta.y);
                    }
                    false
                }
                _ => true,
            });
            input.raw.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::MouseWheel { modifiers, .. }
                        if hovered && !modifiers.ctrl && !modifiers.command
                )
            });
            if hovered && (consumed_wheel || now < self.carousel_wheel_suppress_until) {
                input.smooth_scroll_delta = Vec2::ZERO;
            }
        });

        if !hovered {
            // A fractional notch belongs to the region where it started. Do not let it
            // complete later after the pointer has moved to the rail or game window.
            self.carousel_wheel_accum = 0.0;
        }
        if consumed_wheel {
            self.carousel_wheel_last_event_at = now;
            self.carousel_wheel_suppress_until = now + WHEEL_IDLE_RESET_SECS;
            suppress_tooltips(ui.ctx());
        }
        if now - self.carousel_wheel_last_event_at > WHEEL_IDLE_RESET_SECS {
            self.carousel_wheel_accum = 0.0;
        }
        if hovered && self.carousel_wheel_accum != 0.0 {
            if now - self.carousel_wheel_last_step_at >= WHEEL_STEP_COOLDOWN_SECS
                && self.carousel_wheel_accum.abs() >= 1.0
            {
                direction = Some(if self.carousel_wheel_accum > 0.0 {
                    -1
                } else {
                    1
                });
                self.carousel_wheel_accum = 0.0;
                self.carousel_wheel_last_step_at = now;
            } else if consumed_wheel {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        WHEEL_STEP_COOLDOWN_SECS,
                    ));
            }
        }
        direction
    }

    fn consume_category_wheel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        visible_indices: &[usize],
    ) -> Option<usize> {
        if visible_indices.is_empty() {
            return None;
        }
        if !ui.rect_contains_pointer(rect) {
            // A partial wheel notch must never leak into the category rail after leaving it.
            self.category_wheel_accum = 0.0;
            return None;
        }
        let now = ui.input(|input| input.time);
        let mut consumed_wheel = false;
        ui.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    phase,
                } = event
                else {
                    return true;
                };
                if modifiers.ctrl || modifiers.command {
                    return true;
                }
                consumed_wheel = true;
                if *phase == egui::TouchPhase::Move {
                    self.category_wheel_accum += wheel_units(*unit, delta.y);
                }
                false
            });
            input.raw.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::MouseWheel { modifiers, .. }
                        if !modifiers.ctrl && !modifiers.command
                )
            });
            if consumed_wheel || now < self.category_wheel_suppress_until {
                input.smooth_scroll_delta = Vec2::ZERO;
            }
        });

        if consumed_wheel {
            self.category_wheel_last_event_at = now;
            self.category_wheel_suppress_until = now + WHEEL_IDLE_RESET_SECS;
            suppress_tooltips(ui.ctx());
        }
        if now - self.category_wheel_last_event_at > WHEEL_IDLE_RESET_SECS {
            self.category_wheel_accum = 0.0;
        }
        if self.category_wheel_accum.abs() < 1.0
            || now - self.category_wheel_last_step_at < WHEEL_STEP_COOLDOWN_SECS
        {
            if consumed_wheel {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        WHEEL_STEP_COOLDOWN_SECS,
                    ));
            }
            return None;
        }
        let direction = if self.category_wheel_accum > 0.0 {
            -1
        } else {
            1
        };
        self.category_wheel_accum = 0.0;
        self.category_wheel_last_step_at = now;
        next_visible_category(visible_indices, self.selected_category, direction)
    }

    fn apply_focused_costume_action(&mut self, action: ModAction, now: f64) {
        let category_index = self.selected_category;
        let costume_index = self.carousel_focus;
        if !self.visible_mods().contains(&costume_index) {
            return;
        }
        if !matches!(self.card(category_index, costume_index), Some(Card::Own(_))) {
            self.activate_gamebanana(
                category_index,
                costume_index,
                action == ModAction::Exclusive,
            );
            return;
        }
        let mut changed = false;
        if let Some(category) = self.catalog.categories.get_mut(category_index) {
            let Some(costume) = category.costumes.get(costume_index) else {
                return;
            };
            let action = match action {
                ModAction::Exclusive => ChangeAction::Use,
                // Enabling through the toggle keeps the others enabled.
                ModAction::Toggle if costume.active => ChangeAction::TurnOff,
                ModAction::Toggle => ChangeAction::TurnOn,
            };
            let request = change_costumes(category, costume_index, action);
            self.active_images[category_index] = active_image(category);
            if let Some(request) = request {
                self.ask(request);
                changed = true;
            }
        }
        if changed {
            self.forget_used_new_mods();
            self.active_feedback_until = now + ACTIVE_FEEDBACK_SECS;
            self.active_feedback_costume = Some(costume_index);
        }
    }

    fn texture_for(&mut self, ui: &Ui, path: Option<&Path>) -> Option<egui::TextureHandle> {
        self.thumbnails.get(ui.ctx(), path?)
    }
}

/// Queue the images of the mods the search shows around `focus`.
fn append_nearby_mod_images(
    paths: &mut Vec<PathBuf>,
    visible: &[usize],
    focus: usize,
    radius: usize,
    image_of: impl Fn(usize) -> Option<PathBuf>,
) {
    let count = visible.len();
    if count == 0 {
        return;
    }
    let focus = visible
        .iter()
        .position(|&index| index == focus)
        .unwrap_or(0);
    paths.extend(image_of(visible[focus]));
    for distance in 1..=radius.min(count - 1) {
        for position in [
            (focus + distance) % count,
            (focus + count - distance) % count,
        ] {
            if let Some(path) = image_of(visible[position])
                && !paths.contains(&path)
            {
                paths.push(path);
            }
        }
    }
}

fn wheel_units(unit: egui::MouseWheelUnit, delta_y: f32) -> f32 {
    if delta_y == 0.0 {
        return 0.0;
    }
    match unit {
        egui::MouseWheelUnit::Point => (delta_y / WHEEL_POINTS_PER_STEP).clamp(-1.0, 1.0),
        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta_y.signum(),
    }
}

fn next_visible_category(
    visible_indices: &[usize],
    selected_category: usize,
    direction: i32,
) -> Option<usize> {
    if visible_indices.is_empty() {
        return None;
    }
    let current = visible_indices
        .iter()
        .position(|&index| index == selected_category);
    let target = match (current, direction.signum()) {
        (Some(position), -1) => position.checked_sub(1),
        (Some(position), 1) => (position + 1 < visible_indices.len()).then_some(position + 1),
        (None, -1) => visible_indices
            .iter()
            .rposition(|&index| index < selected_category)
            .or_else(|| Some(0)),
        (None, 1) => visible_indices
            .iter()
            .position(|&index| index > selected_category)
            .or_else(|| Some(visible_indices.len() - 1)),
        _ => None,
    }?;
    visible_indices.get(target).copied()
}

/// The mod `direction` steps to among the mods a search shows.  Stops at
/// either end, like the carousel without a search.
fn next_visible_mod(visible: &[usize], focus: usize, direction: i32) -> Option<usize> {
    let first = *visible.first()?;
    let Some(position) = visible.iter().position(|&index| index == focus) else {
        return Some(first);
    };
    Some(visible[next_focus_index(visible.len(), position, direction)])
}

fn next_focus_index(len: usize, current: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let current = current.min(len - 1);
    match direction.signum() {
        -1 if current == 0 => 0,
        -1 => current - 1,
        1 => (current + 1).min(len - 1),
        _ => current,
    }
}

fn card_reveal_progress(
    placement: &CarouselCardPlacement,
    progress: f32,
    cards: &[CarouselCardPlacement],
) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    // Reveal the focused card first, then bring in its neighbors with a small stagger.
    // A pair of cards has no center slot, so its other card still appears promptly.
    let delay = if placement.focused {
        0.0
    } else if !cards.iter().any(|card| card.slot == 1) {
        0.10
    } else if placement.slot == 0 {
        0.22
    } else {
        0.40
    };
    let local = ((progress - delay) / (1.0 - delay)).clamp(0.0, 1.0);
    local * local * (3.0 - 2.0 * local)
}

fn carousel_card_placements(
    carousel_rect: Rect,
    costume_count: usize,
    focus: usize,
    grows: bool,
) -> Vec<CarouselCardPlacement> {
    if costume_count == 0 {
        return Vec::new();
    }
    let focus = focus.min(costume_count - 1);
    // Two cards sit as a pair, unless more can come: then the focused card
    // stays in the middle, so it doesn't move when they do.
    let pair = costume_count == 2 && !grows;
    let center_y = carousel_rect.center().y;
    let center_x = carousel_rect.center().x;
    let focused_center_x = if pair {
        // Keep a two-card group centered as a whole, regardless of which card is focused.
        let offset = (CAROUSEL_CARD_GAP + NEIGHBOR_CARD_SIZE.x) * 0.5;
        if focus == 0 {
            center_x - offset
        } else {
            center_x + offset
        }
    } else {
        center_x
    };
    let focused_rect =
        Rect::from_center_size(egui::pos2(focused_center_x, center_y), FOCUSED_CARD_SIZE);
    let mut placements = Vec::with_capacity(costume_count.min(3));
    if focus > 0 {
        placements.push(CarouselCardPlacement {
            index: focus - 1,
            rect: Rect::from_center_size(
                egui::pos2(
                    focused_rect.min.x - CAROUSEL_CARD_GAP - NEIGHBOR_CARD_SIZE.x * 0.5,
                    center_y,
                ),
                NEIGHBOR_CARD_SIZE,
            ),
            focused: false,
            slot: 0,
        });
    }
    if focus + 1 < costume_count {
        placements.push(CarouselCardPlacement {
            index: focus + 1,
            rect: Rect::from_center_size(
                egui::pos2(
                    focused_rect.max.x + CAROUSEL_CARD_GAP + NEIGHBOR_CARD_SIZE.x * 0.5,
                    center_y,
                ),
                NEIGHBOR_CARD_SIZE,
            ),
            focused: false,
            slot: 2,
        });
    }
    placements.push(CarouselCardPlacement {
        index: focus,
        rect: focused_rect,
        focused: true,
        slot: if pair {
            if focus == 0 { 0 } else { 2 }
        } else {
            1
        },
    });
    placements
}

/// Card placements for the mods a search shows.  They sit side by side as if
/// the hidden mods were not installed.
fn visible_card_placements(
    carousel_rect: Rect,
    visible: &[usize],
    focus: usize,
    grows: bool,
) -> Vec<CarouselCardPlacement> {
    let position = visible
        .iter()
        .position(|&index| index == focus)
        .unwrap_or(0);
    let mut placements = carousel_card_placements(carousel_rect, visible.len(), position, grows);
    for placement in &mut placements {
        placement.index = visible[placement.index];
    }
    placements
}

fn lerp_rect(from: Rect, to: Rect, t: f32) -> Rect {
    Rect::from_min_max(lerp_pos(from.min, to.min, t), lerp_pos(from.max, to.max, t))
}

fn lerp_pos(from: egui::Pos2, to: egui::Pos2, t: f32) -> egui::Pos2 {
    from + (to - from) * t
}

fn same_card_geometry(from: &[CarouselCardPlacement], to: &[CarouselCardPlacement]) -> bool {
    from.len() == to.len()
        && to.iter().all(|target| {
            from.iter().any(|source| {
                source.index == target.index
                    && source.rect.min.distance(target.rect.min) < 0.5
                    && source.rect.max.distance(target.rect.max) < 0.5
                    && source.focused == target.focused
            })
        })
}

fn valid_carousel_release(
    press: &CarouselPointerPress,
    category_index: usize,
    pointer_pos: Option<egui::Pos2>,
    can_click: bool,
) -> Option<usize> {
    (press.category_index == category_index)
        .then_some(press)
        .filter(|press| can_click && pointer_pos.is_some_and(|pos| press.rect.contains(pos)))
        .map(|press| press.costume_index)
}

fn active_costume_index(category: &Category) -> usize {
    category
        .costumes
        .iter()
        .position(|costume| costume.active)
        .unwrap_or(0)
}

/// The pictures of the mods the library censors.
fn censored_images(catalog: &Catalog) -> HashSet<PathBuf> {
    catalog
        .categories
        .iter()
        .flat_map(|category| &category.costumes)
        .filter(|costume| costume.censored)
        .filter_map(|costume| costume.image.clone())
        .collect()
}

/// The rail's picture for a category without its own: the active mod's, else
/// the first mod's that has one.
fn active_image(category: &Category) -> Option<PathBuf> {
    category
        .costumes
        .iter()
        .find(|costume| costume.active)
        .and_then(|costume| costume.image.clone())
        .or_else(|| category.image.clone())
        .or_else(|| {
            category
                .costumes
                .iter()
                .find_map(|costume| costume.image.clone())
        })
}

fn character_name(name: &str) -> &str {
    name.strip_prefix("Operators: ").unwrap_or(name)
}

/// Marks a mod the overlay installed, until it's used.
/// `room` is what the keys button beside it takes.
fn paint_new_tag(ui: &Ui, card: Rect, room: f32, reveal: f32, overlay_opacity: u8) {
    let color = scale_color_alpha(content_gray(27, overlay_opacity), reveal);
    let label = ui
        .painter()
        .layout_job(egui::text::LayoutJob::single_section(
            text(TextKey::GameOverlayNewTag).to_owned(),
            egui::TextFormat {
                font_id: FontId::proportional(10.0),
                color,
                extra_letter_spacing: 0.6,
                ..Default::default()
            },
        ));
    let tag = Rect::from_min_size(
        egui::pos2(
            card.max.x - 8.0 - room - label.size().x - 14.0,
            card.min.y + 8.0,
        ),
        Vec2::new(label.size().x + 14.0, 18.0),
    );
    ui.painter().rect_filled(
        tag,
        CornerRadius::same(9),
        scale_color_alpha(content_color(ACCENT, overlay_opacity), reveal),
    );
    ui.painter()
        .galley(tag.center() - label.size() * 0.5, label, color);
}

/// Marks a GameBanana card, which isn't installed, or has an empty tick box
/// once it is, since installs land turned off.  The focused card says where
/// it's from.
fn paint_gamebanana_badge(
    ui: &Ui,
    card: Rect,
    focused: bool,
    installed: bool,
    reveal: f32,
    overlay_opacity: u8,
) {
    let color = scale_color_alpha(content_gray(230, overlay_opacity), reveal);
    let label = focused.then(|| {
        ui.painter()
            .layout_no_wrap("GameBanana".to_owned(), FontId::proportional(10.5), color)
    });
    let width = 21.0 + label.as_ref().map_or(0.0, |label| label.size().x + 6.0);
    let badge = Rect::from_min_size(card.min + Vec2::splat(8.0), Vec2::new(width, 21.0));
    ui.painter().rect_filled(
        badge,
        CornerRadius::same(4),
        scale_color_alpha(
            Color32::from_black_alpha(chrome_alpha(185, overlay_opacity)),
            reveal,
        ),
    );
    ui.painter().text(
        egui::pos2(badge.min.x + 10.5, badge.center().y),
        Align2::CENTER_CENTER,
        char::from(if installed {
            lucide_icons::Icon::Square
        } else {
            lucide_icons::Icon::Download
        })
        .to_string(),
        FontId::new(12.0, egui::FontFamily::Name("preview-icons".into())),
        color,
    );
    if let Some(label) = label {
        let position = egui::pos2(badge.min.x + 21.0, badge.center().y - label.size().y * 0.5);
        ui.painter().galley(position, label, color);
    }
}

/// What a GameBanana card says under its name while Hestia installs it.
struct InstallStatus {
    text: String,
    color: Color32,
    bar: InstallBar,
}

/// The bar along the bottom of a card Hestia installs.
enum InstallBar {
    Hidden,
    Moving,
    Filled(f32),
}

fn install_status(stage: &InstallStage, focused: bool) -> Option<InstallStatus> {
    const WORKING: Color32 = Color32::from_rgb(240, 168, 120);
    const DONE: Color32 = Color32::from_rgb(201, 208, 213);
    const FAILED: Color32 = Color32::from_rgb(230, 140, 130);
    let (label, color, bar) = match stage {
        InstallStage::Waiting => (
            text(TextKey::GameOverlayWaiting).to_owned(),
            WORKING,
            InstallBar::Moving,
        ),
        InstallStage::Downloading {
            percent: Some(percent),
        } => (
            text(TextKey::GameOverlayDownloadingPercent).replace("{percent}", &percent.to_string()),
            WORKING,
            InstallBar::Filled(f32::from(*percent) / 100.0),
        ),
        InstallStage::Downloading { percent: None } => (
            text(TextKey::GameOverlayDownloading).to_owned(),
            WORKING,
            InstallBar::Moving,
        ),
        InstallStage::Installing => (
            text(TextKey::GameOverlayInstalling).to_owned(),
            WORKING,
            InstallBar::Moving,
        ),
        InstallStage::ChooseFile { .. } | InstallStage::SameName { .. } => (
            text(TextKey::GameOverlayNeedsYourAnswer).to_owned(),
            WORKING,
            InstallBar::Hidden,
        ),
        InstallStage::Installed { .. } => (
            text(TextKey::GameOverlayInstalledOff).to_owned(),
            DONE,
            InstallBar::Filled(1.0),
        ),
        InstallStage::Failed if focused => (
            text(TextKey::GameOverlayCouldNotInstallTryAgain).to_owned(),
            FAILED,
            InstallBar::Hidden,
        ),
        InstallStage::Failed => (
            text(TextKey::GameOverlayCouldNotInstall).to_owned(),
            FAILED,
            InstallBar::Hidden,
        ),
        InstallStage::Canceled => return None,
    };
    Some(InstallStatus {
        text: label,
        color,
        bar,
    })
}

fn paint_install_bar(
    ui: &Ui,
    card: Rect,
    bar: InstallBar,
    now: f64,
    reveal: f32,
    overlay_opacity: u8,
) {
    let track = Rect::from_min_max(egui::pos2(card.min.x, card.max.y - 3.0), card.max);
    let span = match bar {
        InstallBar::Hidden => return,
        InstallBar::Filled(share) => (0.0, share.clamp(0.0, 1.0)),
        InstallBar::Moving => {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
            let start = ((now / 1.2).fract() * 1.35 - 0.35) as f32;
            (start.max(0.0), (start + 0.35).min(1.0))
        }
    };
    ui.painter().rect_filled(
        track,
        CornerRadius::ZERO,
        scale_color_alpha(
            Color32::from_black_alpha(chrome_alpha(150, overlay_opacity)),
            reveal,
        ),
    );
    let filled = Rect::from_min_max(
        egui::pos2(track.min.x + track.width() * span.0, track.min.y),
        egui::pos2(track.min.x + track.width() * span.1, track.max.y),
    );
    ui.painter().rect_filled(
        filled,
        CornerRadius::ZERO,
        scale_color_alpha(content_color(ACCENT, overlay_opacity), reveal),
    );
}

/// `text` on one line, cut with an ellipsis to fit `width`.
fn elided_galley(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, width.max(1.0));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

/// A download's size, as the question panel shows it.
fn file_size_label(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let size = size as f64;
    if size >= MB {
        format!("{:.1} MB", size / MB)
    } else {
        format!("{:.0} KB", (size / KB).max(1.0))
    }
}

fn paint_dashed_rect(ui: &Ui, rect: Rect, stroke: Stroke) {
    let points = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    ui.painter()
        .extend(egui::Shape::dashed_line(&points, stroke, 3.0, 2.0));
}

fn paint_category_name(ui: &Ui, rect: Rect, name: &str, font: FontId, color: Color32) -> bool {
    let mut job = egui::text::LayoutJob::simple(name.to_owned(), font, color, rect.width());
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    let position = egui::pos2(
        rect.center().x - galley.size().x * 0.5,
        rect.center().y - galley.size().y * 0.5,
    );
    let elided = galley.elided;
    ui.painter().galley(position, galley, color);
    elided
}

fn clean_display_name(name: &str) -> String {
    static VERSION_SUFFIX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)(?:\s+v(?:ersion)?\s*\d+(?:\.\d+)*|\s*\d+(?:\.\d+)+)\s*$")
            .expect("valid preview version suffix")
    });
    let readable = name
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = VERSION_SUFFIX.replace(&readable, "");
    if cleaned.trim().is_empty() {
        readable
    } else {
        cleaned.trim().to_owned()
    }
}

fn opacity_unit(overlay_opacity: u8) -> f32 {
    f32::from(overlay_opacity.clamp(OVERLAY_OPACITY_MIN, OVERLAY_OPACITY_MAX)) / 100.0
}

fn image_alpha(overlay_opacity: u8) -> u8 {
    (opacity_unit(overlay_opacity) * 255.0).round() as u8
}

fn scaled_alpha(alpha: u8, factor: f32) -> u8 {
    (f32::from(alpha) * factor.clamp(0.0, 1.0)).round() as u8
}

fn scale_color_alpha(color: Color32, factor: f32) -> Color32 {
    // Color32 already stores premultiplied channels. Scale them together so
    // a completed reveal is exactly the original color, without a second tint.
    color.gamma_multiply(factor.clamp(0.0, 1.0))
}

fn chrome_opacity_unit(overlay_opacity: u8) -> f32 {
    opacity_unit(overlay_opacity).powf(1.3)
}

fn content_opacity_unit(overlay_opacity: u8) -> f32 {
    0.90 + 0.10 * (opacity_unit(overlay_opacity) - 0.50) / 0.44
}

fn chrome_alpha(base_alpha: u8, overlay_opacity: u8) -> u8 {
    (f32::from(base_alpha) * chrome_opacity_unit(overlay_opacity)).round() as u8
}

fn content_alpha(base_alpha: u8, overlay_opacity: u8) -> u8 {
    (f32::from(base_alpha) * content_opacity_unit(overlay_opacity)).round() as u8
}

fn chrome_alpha_from_image_alpha(base_alpha: u8, image_alpha: u8) -> u8 {
    let opacity = f32::from(image_alpha) / 255.0;
    (f32::from(base_alpha) * opacity.powf(1.3)).round() as u8
}

fn content_gray(value: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(value, value, value, content_alpha(255, overlay_opacity))
}

fn chrome_gray_from_image_alpha(value: u8, image_alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        value,
        value,
        value,
        chrome_alpha_from_image_alpha(255, image_alpha),
    )
}

fn content_color(color: Color32, overlay_opacity: u8) -> Color32 {
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, content_alpha(a, overlay_opacity))
}

fn white_alpha(base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_white_alpha(chrome_alpha(base_alpha, overlay_opacity))
}

fn rgba_alpha(r: u8, g: u8, b: u8, base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, chrome_alpha(base_alpha, overlay_opacity))
}

fn content_black_alpha(base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_black_alpha(content_alpha(base_alpha, overlay_opacity))
}

fn content_rgba(r: u8, g: u8, b: u8, base_alpha: u8, overlay_opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, content_alpha(base_alpha, overlay_opacity))
}

fn paint_thumbnail_tinted(
    ui: &mut Ui,
    rect: Rect,
    texture: Option<&egui::TextureHandle>,
    crop: bool,
    show_missing_text: bool,
    tint_alpha: u8,
) {
    let Some(texture) = texture else {
        ui.painter().rect_filled(
            rect,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(
                65,
                65,
                65,
                chrome_alpha_from_image_alpha(110, tint_alpha),
            ),
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            char::from(lucide_icons::Icon::Image).to_string(),
            FontId::new(18.0, egui::FontFamily::Name("preview-icons".into())),
            chrome_gray_from_image_alpha(145, tint_alpha),
        );
        if show_missing_text {
            ui.painter().text(
                egui::pos2(rect.center().x, rect.center().y + 18.0),
                Align2::CENTER_CENTER,
                text(TextKey::GameOverlayNoPreview),
                FontId::proportional(9.0),
                chrome_gray_from_image_alpha(139, tint_alpha),
            );
        }
        return;
    };

    if crop {
        egui::Image::from_texture(texture)
            .uv(cover_uv(rect.size(), texture.size_vec2()))
            .tint(Color32::from_white_alpha(tint_alpha))
            .corner_radius(CornerRadius::ZERO)
            .paint_at(ui, rect);
        return;
    }
    paint_fitted_image_tinted(ui, rect, Some(texture), tint_alpha);
}

/// The part of a picture that covers `size`.
fn cover_uv(size: Vec2, source_size: Vec2) -> Rect {
    let scale = (size.x / source_size.x).max(size.y / source_size.y);
    let uv_size = size / (source_size * scale);
    // Portrait covers tend to place faces above center; keep that region visible.
    let center_y = uv_size.y * 0.5 + (1.0 - uv_size.y) * 0.08;
    Rect::from_center_size(egui::pos2(0.5, center_y), uv_size)
}

/// A card's picture cropped for `full` and squeezed into `shown`, so a
/// turning card keeps showing the same part of it.
fn paint_thumbnail_squeezed(
    ui: &mut Ui,
    full: Rect,
    shown: Rect,
    texture: Option<&egui::TextureHandle>,
    tint_alpha: u8,
) {
    let Some(texture) = texture else {
        paint_thumbnail_tinted(ui, shown, None, true, false, tint_alpha);
        return;
    };
    egui::Image::from_texture(texture)
        .uv(cover_uv(full.size(), texture.size_vec2()))
        .tint(Color32::from_white_alpha(tint_alpha))
        .corner_radius(CornerRadius::ZERO)
        .paint_at(ui, shown);
}

fn paint_fitted_image_tinted(
    ui: &Ui,
    rect: Rect,
    texture: Option<&egui::TextureHandle>,
    tint_alpha: u8,
) {
    let Some(texture) = texture else {
        return;
    };
    let source_size = texture.size_vec2();
    let image_rect = fitted_image_rect(rect, source_size);
    egui::Image::from_texture(texture)
        .tint(Color32::from_white_alpha(tint_alpha))
        .corner_radius(CornerRadius::ZERO)
        .paint_at(ui, image_rect);
}

fn fitted_image_rect(rect: Rect, source_size: Vec2) -> Rect {
    if source_size.x <= 0.0 || source_size.y <= 0.0 {
        return rect;
    }
    let scale = (rect.width() / source_size.x).min(rect.height() / source_size.y);
    Rect::from_center_size(rect.center(), source_size * scale)
}

fn paint_bottom_gradient(
    ui: &mut Ui,
    rect: Rect,
    overlay_opacity: u8,
    requested_height: f32,
    reveal: f32,
) {
    let gradient_height = (requested_height + 22.0).clamp(24.0, rect.height() * 0.56);
    // Fade above the title block, keeping the title itself on a dark base.
    let fade_end = ((gradient_height - requested_height) / gradient_height).max(0.01);
    let stops: [(f32, u8); 4] = [(0.0, 0), (fade_end * 0.4, 20), (fade_end, 150), (1.0, 218)];
    let start_y = rect.max.y - gradient_height;
    let mut mesh = egui::Mesh::default();
    for row in 0..=64 {
        let stop = row as f32 / 64.0;
        let interval = stops.windows(2).find(|pair| stop <= pair[1].0).unwrap();
        let blend = (stop - interval[0].0) / (interval[1].0 - interval[0].0);
        let alpha = (f32::from(interval[0].1)
            + blend * (f32::from(interval[1].1) - f32::from(interval[0].1)))
            as u8;
        let y = start_y + gradient_height * stop;
        let base = mesh.vertices.len() as u32;
        let color = scale_color_alpha(content_black_alpha(alpha, overlay_opacity), reveal);
        mesh.vertices.push(egui::epaint::Vertex::untextured(
            egui::pos2(rect.min.x, y),
            color,
        ));
        mesh.vertices.push(egui::epaint::Vertex::untextured(
            egui::pos2(rect.max.x, y),
            color,
        ));
        if row > 0 {
            mesh.indices
                .extend_from_slice(&[base - 2, base, base - 1, base - 1, base, base + 1]);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn card_name_rect(image_rect: Rect, focused: bool) -> Rect {
    let max_text_height = if focused { 34.0 } else { 29.0 };
    Rect::from_min_max(
        egui::pos2(
            image_rect.min.x + 8.0,
            image_rect.max.y - CARD_TITLE_BOTTOM_PADDING - max_text_height,
        ),
        egui::pos2(
            image_rect.max.x - 8.0,
            image_rect.max.y - CARD_TITLE_BOTTOM_PADDING,
        ),
    )
}

fn measure_card_name(
    ui: &Ui,
    rect: Rect,
    name: &str,
    overlay_opacity: u8,
    focused: bool,
) -> (Vec2, bool) {
    let galley = ui
        .painter()
        .layout_job(card_name_job(rect, name, overlay_opacity, focused));
    (galley.size(), galley.elided)
}

fn card_name_job(
    rect: Rect,
    name: &str,
    overlay_opacity: u8,
    focused: bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple(
        clean_display_name(name),
        FontId::proportional(if focused { 12.5 } else { 10.5 }),
        content_gray(255, overlay_opacity),
        rect.width(),
    );
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    job
}

fn paint_card_name_revealed(
    ui: &Ui,
    rect: Rect,
    name: &str,
    overlay_opacity: u8,
    focused: bool,
    reveal: f32,
) -> bool {
    let galley = ui
        .painter()
        .layout_job(card_name_job(rect, name, overlay_opacity, focused));
    let elided = galley.elided;
    let position = egui::pos2(rect.min.x, rect.max.y - galley.size().y);
    ui.painter().galley(
        position,
        galley,
        scale_color_alpha(content_gray(255, overlay_opacity), reveal),
    );
    elided
}

/// What a click on a costume does: one that's on turns off.  One that's off
/// is used while at most one is on, otherwise it's turned on next to them.
fn select_costume(category: &mut Category, index: usize) -> Option<ModRequest> {
    let costume = category.costumes.get(index)?;
    let active_count = category
        .costumes
        .iter()
        .filter(|costume| costume.active)
        .count();
    let action = if costume.active {
        ChangeAction::TurnOff
    } else if active_count <= 1 {
        ChangeAction::Use
    } else {
        ChangeAction::TurnOn
    };
    change_costumes(category, index, action)
}

/// Makes a change on screen as Hestia will make it, and returns the request
/// for Hestia when something changed.
fn change_costumes(
    category: &mut Category,
    index: usize,
    action: ChangeAction,
) -> Option<ModRequest> {
    let loose = is_loose(category);
    let before: Vec<bool> = category
        .costumes
        .iter()
        .map(|costume| costume.active)
        .collect();
    for (costume_index, costume) in category.costumes.iter_mut().enumerate() {
        let target = costume_index == index;
        costume.active = match action {
            ChangeAction::Use if target => true,
            ChangeAction::Use if loose => costume.active,
            ChangeAction::Use => false,
            ChangeAction::TurnOn if target => true,
            ChangeAction::TurnOff if target => false,
            ChangeAction::TurnOn | ChangeAction::TurnOff => costume.active,
        };
    }
    let states: Vec<(String, bool)> = category
        .costumes
        .iter()
        .zip(before)
        .filter(|(costume, was)| costume.active != *was)
        .map(|(costume, _)| (costume.id.clone(), costume.active))
        .collect();
    (!states.is_empty()).then(|| ModRequest {
        mod_id: category.costumes[index].id.clone(),
        action,
        states,
    })
}

/// Uncategorized holds mods that have nothing to do with each other, so
/// using one leaves the rest on.
fn is_loose(category: &Category) -> bool {
    category.id == UNCATEGORIZED_ID
}

#[cfg(test)]
mod tests {
    use super::super::data::Costume;
    use super::*;

    #[test]
    fn display_names_remove_versions_but_preserve_meaningful_numbers() {
        assert_eq!(clean_display_name("Dawn1.8"), "Dawn");
        assert_eq!(clean_display_name("Summer_Outfit_v2"), "Summer Outfit");
        assert_eq!(
            clean_display_name("Summer 2024 Recolor"),
            "Summer 2024 Recolor"
        );
        assert_eq!(clean_display_name("Outfit 2 Blue"), "Outfit 2 Blue");
        assert_eq!(clean_display_name("2B"), "2B");
    }

    #[test]
    fn artwork_opacity_uses_the_clamped_slider_directly() {
        assert_eq!(image_alpha(40), 128);
        assert_eq!(image_alpha(50), 128);
        assert_eq!(image_alpha(55), 140);
        assert_eq!(image_alpha(70), 179);
        assert_eq!(image_alpha(90), 230);
        assert_eq!(image_alpha(94), 240);
        assert_eq!(image_alpha(100), 240);
    }

    #[test]
    fn wheel_units_clamp_each_line_event_to_one_step() {
        assert_eq!(wheel_units(egui::MouseWheelUnit::Line, -4.0), -1.0);
        assert_eq!(wheel_units(egui::MouseWheelUnit::Line, 3.0), 1.0);
        assert_eq!(wheel_units(egui::MouseWheelUnit::Point, -25.0), -0.5);
    }

    #[test]
    fn wheel_selection_skips_filtered_categories_and_clamps_at_boundaries() {
        let visible = vec![1, 4, 8];
        assert_eq!(next_visible_category(&visible, 4, 1), Some(8));
        assert_eq!(next_visible_category(&visible, 4, -1), Some(1));
        assert_eq!(next_visible_category(&visible, 8, 1), None);
        assert_eq!(next_visible_category(&visible, 1, -1), None);
        assert_eq!(next_visible_category(&visible, 3, 1), Some(4));
        assert_eq!(next_visible_category(&visible, 3, -1), Some(1));
    }

    #[test]
    fn carousel_focus_stops_at_boundaries_without_wrapping() {
        assert_eq!(next_focus_index(3, 0, -1), 0);
        assert_eq!(next_focus_index(3, 2, 1), 2);
        assert_eq!(next_focus_index(1, 0, -1), 0);
        assert_eq!(next_focus_index(2, 0, 1), 1);
        assert_eq!(next_focus_index(2, 1, -1), 0);
    }

    #[test]
    fn endpoint_carousel_placements_show_only_real_neighbors() {
        let carousel = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(560.0, 266.0));
        let first = carousel_card_placements(carousel, 4, 0, false);
        let last = carousel_card_placements(carousel, 4, 3, false);
        assert_eq!(
            first.iter().map(|card| card.index).collect::<Vec<_>>(),
            vec![1, 0]
        );
        assert_eq!(
            last.iter().map(|card| card.index).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(first.iter().all(|card| card.index != 3));
        assert!(last.iter().all(|card| card.index != 0));
    }

    #[test]
    fn staged_reveal_makes_focused_card_available_before_neighbors() {
        let carousel = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(560.0, 266.0));
        let cards = carousel_card_placements(carousel, 3, 1, false);
        let focused = cards.iter().find(|card| card.focused).unwrap();
        let left = cards.iter().find(|card| card.slot == 0).unwrap();
        let right = cards.iter().find(|card| card.slot == 2).unwrap();
        assert!(card_reveal_progress(focused, 0.15, &cards) > 0.0);
        assert_eq!(card_reveal_progress(left, 0.15, &cards), 0.0);
        assert_eq!(card_reveal_progress(right, 0.30, &cards), 0.0);
        assert!(card_reveal_progress(left, 0.30, &cards) > 0.0);
        assert!(card_reveal_progress(right, 0.50, &cards) > 0.0);
    }

    #[test]
    fn two_card_carousel_stays_balanced_when_focus_changes() {
        let carousel = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(560.0, 266.0));
        for focus in [0, 1] {
            let cards = carousel_card_placements(carousel, 2, focus, false);
            let min_x = cards
                .iter()
                .map(|card| card.rect.min.x)
                .fold(f32::INFINITY, f32::min);
            let max_x = cards
                .iter()
                .map(|card| card.rect.max.x)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(((min_x + max_x) * 0.5 - carousel.center().x).abs() < 0.01);
        }
    }

    #[test]
    fn focused_and_neighbor_cards_keep_the_same_aspect_ratio() {
        let focused_ratio = FOCUSED_CARD_SIZE.x / FOCUSED_CARD_SIZE.y;
        let neighbor_ratio = NEIGHBOR_CARD_SIZE.x / NEIGHBOR_CARD_SIZE.y;
        assert!((focused_ratio - neighbor_ratio).abs() < 0.01);
    }

    #[test]
    fn rapid_carousel_retarget_keeps_current_positions() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false, false])],
            note: None,
        });
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        let start = layouts.current_carousel_placements(rect, 1.0);
        layouts.carousel_focus = 1;
        layouts.begin_carousel_transition(start, rect, 1.0);
        let midway = layouts.current_carousel_placements(rect, 1.07);
        layouts.carousel_focus = 2;
        layouts.begin_carousel_transition(midway.clone(), rect, 1.07);
        let retargeted = layouts.current_carousel_placements(rect, 1.07);
        for before in &midway {
            if let Some(after) = retargeted.iter().find(|card| card.index == before.index) {
                assert_eq!(before.rect, after.rect);
            }
        }
        let settled = layouts.current_carousel_placements(rect, 2.0);
        assert!(same_card_geometry(
            &settled,
            &carousel_card_placements(rect, 3, 2, false)
        ));
        assert!(layouts.carousel_transition.is_none());
    }

    #[test]
    fn carousel_widget_ids_follow_visual_slots_across_category_changes() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false]), category(&[true, false])],
            note: None,
        });
        let context = egui::Context::default();
        let size = Vec2::new(560.0, 266.0);

        // Different categories reuse the same two-card layout.
        run_layout_frame(&context, &mut layouts, size, 1.0, Vec::new(), false);
        let left_before = context
            .read_response(carousel_slot_id(0))
            .expect("left carousel slot should be registered")
            .rect;
        let right_before = context
            .read_response(carousel_slot_id(2))
            .expect("right carousel slot should be registered")
            .rect;

        layouts.select_category(1);
        run_layout_frame(&context, &mut layouts, size, 1.1, Vec::new(), false);

        // If IDs were based on category/costume indices, these responses would
        // disappear when a different category replaced the cards in-place. This
        // is the same stable-rect condition egui uses for its warning.
        let left_after = context
            .read_response(carousel_slot_id(0))
            .expect("left carousel slot id must survive category changes")
            .rect;
        let right_after = context
            .read_response(carousel_slot_id(2))
            .expect("right carousel slot id must survive category changes")
            .rect;
        assert_eq!(left_before, left_after);
        assert_eq!(right_before, right_after);
    }

    #[test]
    fn carousel_slots_stay_unique_during_wrapping_animations() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        for count in 1..=5 {
            let mut layouts = Layouts::new(Catalog {
                game: "Test".into(),
                categories: vec![category(&vec![false; count])],
                note: None,
            });
            for focus in 0..count {
                let from = carousel_card_placements(rect, count, focus, false);
                layouts.carousel_focus = (focus + 1) % count;
                layouts.begin_carousel_transition(from, rect, 1.0);
                for step in 0..=14 {
                    let cards = layouts.current_carousel_placements(rect, 1.0 + step as f64 * 0.01);
                    let slots: std::collections::HashSet<_> =
                        cards.iter().map(|card| card.slot).collect();
                    assert_eq!(
                        slots.len(),
                        cards.len(),
                        "duplicate ids with {count} cards at step {step}"
                    );
                }
            }
        }
    }

    #[test]
    fn direct_card_activation_keeps_focus_and_transition_in_place() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false, false])],
            note: None,
        });
        layouts.carousel_focus = 0;
        layouts.carousel_transition = Some(CarouselTransition {
            from: carousel_card_placements(
                Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
                3,
                0,
                false,
            ),
            started_at: 1.0,
            seconds: CAROUSEL_TRANSITION_SECS,
        });

        layouts.activate_costume_in_place(0, 2, 1.05);

        assert_eq!(layouts.carousel_focus, 0);
        assert!(layouts.carousel_transition.is_some());
        assert_eq!(layouts.catalog.categories[0].costumes[2].active, true);
        assert_eq!(layouts.catalog.categories[0].costumes[0].active, false);
        assert_eq!(layouts.active_feedback_costume, Some(2));
    }

    #[test]
    fn release_uses_the_pressed_card_even_if_current_geometry_moved() {
        let press = CarouselPointerPress {
            category_index: 0,
            costume_index: 2,
            rect: Rect::from_min_size(egui::pos2(20.0, 30.0), Vec2::new(80.0, 90.0)),
            placements: Vec::new(),
        };
        assert_eq!(
            valid_carousel_release(&press, 0, Some(egui::pos2(35.0, 45.0)), true),
            Some(2)
        );
        assert_eq!(
            valid_carousel_release(&press, 0, Some(egui::pos2(200.0, 200.0)), true),
            None
        );
        assert_eq!(
            valid_carousel_release(&press, 0, Some(egui::pos2(35.0, 45.0)), false),
            None
        );
        assert_eq!(
            valid_carousel_release(&press, 1, Some(egui::pos2(35.0, 45.0)), true),
            None
        );
    }

    #[test]
    fn frame_click_activates_side_card_without_recentering() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let side = carousel_card_placements(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
            3,
            0,
            false,
        )
        .into_iter()
        .find(|card| card.index == 1)
        .expect("right side card");

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            vec![
                pointer_button_event(side.rect.center(), true),
                pointer_button_event(side.rect.center(), false),
            ],
            false,
        );

        assert_eq!(layouts.carousel_focus, 0);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn frame_double_click_turns_a_mod_on_once() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let cards = carousel_card_placements(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
            3,
            0,
            false,
        );
        let click = |layouts: &mut Layouts, index: usize, time: f64| {
            let pos = cards
                .iter()
                .find(|card| card.index == index)
                .expect("visible card")
                .rect
                .center();
            run_layout_frame(
                &context,
                layouts,
                Vec2::new(560.0, 266.0),
                time,
                vec![
                    pointer_button_event(pos, true),
                    pointer_button_event(pos, false),
                ],
                false,
            );
        };

        click(&mut layouts, 1, 1.0);
        click(&mut layouts, 1, 1.2);
        click(&mut layouts, 1, 1.4);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
        click(&mut layouts, 1, 2.0);
        assert!(active_indices(&layouts.catalog.categories[0]).is_empty());
        // Another card is a click of its own.
        click(&mut layouts, 0, 2.1);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
    }

    #[test]
    fn frame_release_resumes_an_animation_from_the_frozen_press_geometry() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let carousel = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        let start = carousel_card_placements(carousel, 3, 0, false);
        layouts.carousel_focus = 1;
        layouts.begin_carousel_transition(start, carousel, 1.0);
        let midway = layouts.current_carousel_placements(carousel, 1.12);
        let side = midway
            .iter()
            .find(|card| card.index == 2)
            .expect("visible side card");
        // At this point the side card has cleared the focused card's hit rectangle. This
        // keeps the fixture about frozen geometry rather than the topmost-overlap rule.
        let side_pos = side.rect.right_center() - Vec2::new(4.0, 0.0);

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.12,
            vec![pointer_button_event(side_pos, true)],
            false,
        );
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.25,
            vec![pointer_button_event(side_pos, false)],
            false,
        );

        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![2]);
        let transition = layouts
            .carousel_transition
            .as_ref()
            .expect("the prior transition should continue");
        assert_eq!(
            transition
                .from
                .iter()
                .find(|card| card.index == 2)
                .unwrap()
                .rect,
            side.rect
        );
        assert_eq!(transition.started_at, 1.25);
    }

    #[test]
    fn frame_drag_cancels_a_pending_card_activation() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let side = carousel_card_placements(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
            3,
            0,
            false,
        )
        .into_iter()
        .find(|card| card.index == 1)
        .expect("right side card");

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            vec![pointer_button_event(side.rect.center(), true)],
            false,
        );
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.05,
            vec![
                egui::Event::PointerMoved(egui::pos2(40.0, 40.0)),
                pointer_button_event(egui::pos2(40.0, 40.0), false),
            ],
            false,
        );

        assert_eq!(layouts.carousel_focus, 0);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
    }

    #[test]
    fn frame_secondary_hold_does_not_activate_or_follow_the_pointer_target() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        let side = carousel_card_placements(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0)),
            3,
            0,
            false,
        )
        .into_iter()
        .find(|card| card.index == 1)
        .expect("right side card");

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            vec![pointer_button_event_with_button(
                side.rect.center(),
                egui::PointerButton::Secondary,
                true,
            )],
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
        assert_eq!(
            layouts.held_preview.map(|preview| preview.costume_index),
            Some(1)
        );

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.1,
            vec![pointer_button_event_with_button(
                egui::pos2(540.0, 260.0),
                egui::PointerButton::Secondary,
                false,
            )],
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
        assert!(layouts.held_preview.is_none());
    }

    #[test]
    fn frame_wheel_routes_to_the_region_under_the_pointer() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.0,
            wheel_events(egui::pos2(280.0, 133.0), -1.0),
            true,
        );
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(layouts.selected_category, 0);

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.2,
            wheel_events(egui::pos2(280.0, 300.0), -1.0),
            true,
        );
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(
            layouts.carousel_focus,
            active_costume_index(&layouts.catalog.categories[1])
        );
    }

    #[test]
    fn fractional_wheel_input_does_not_leak_after_leaving_its_region() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.0,
            wheel_events_with_unit(egui::pos2(280.0, 133.0), -25.0, egui::MouseWheelUnit::Point),
            true,
        );
        assert_eq!(layouts.carousel_focus, 0);

        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.2,
            vec![egui::Event::PointerMoved(egui::pos2(280.0, 300.0))],
            true,
        );
        assert_eq!(layouts.carousel_focus, 0);
        assert_eq!(layouts.selected_category, 0);
    }

    fn test_layouts(category_count: usize) -> Layouts {
        Layouts::new(Catalog {
            game: "Test".into(),
            categories: (0..category_count)
                .map(|index| Category {
                    id: format!("category-{index}"),
                    ..category(&[index == 0, false, false])
                })
                .collect(),
            note: None,
        })
    }

    fn run_layout_frame(
        context: &egui::Context,
        layouts: &mut Layouts,
        size: Vec2,
        time: f64,
        events: Vec<egui::Event>,
        show_category_strip: bool,
    ) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(egui::Pos2::ZERO, size));
        input.time = Some(time);
        input.events = events;
        super::super::apply_preview_style(context);
        let _ = context.run_ui(input, |ui| {
            layouts.show_carousel(ui, 94);
            if show_category_strip {
                layouts.show_category_strip(ui, 94);
            }
        });
    }

    fn pointer_button_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        pointer_button_event_with_button(pos, egui::PointerButton::Primary, pressed)
    }

    fn pointer_button_event_with_button(
        pos: egui::Pos2,
        button: egui::PointerButton,
        pressed: bool,
    ) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn carousel_slot_id(slot: usize) -> egui::Id {
        egui::Id::new((egui::ViewportId::ROOT, "__top_ui"))
            .with(("overlay-preview-carousel-card-slot", slot))
    }

    fn wheel_events(pos: egui::Pos2, delta_y: f32) -> Vec<egui::Event> {
        wheel_events_with_unit(pos, delta_y, egui::MouseWheelUnit::Line)
    }

    fn wheel_events_with_unit(
        pos: egui::Pos2,
        delta_y: f32,
        unit: egui::MouseWheelUnit,
    ) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            // Keep the constructor in one place so the routing test exercises the same event
            // shape produced by winit, including a touch phase.
            egui::Event::MouseWheel {
                unit,
                delta: Vec2::new(0.0, delta_y),
                modifiers: egui::Modifiers::default(),
                phase: egui::TouchPhase::Move,
            },
        ]
    }

    fn active_indices(category: &Category) -> Vec<usize> {
        category
            .costumes
            .iter()
            .enumerate()
            .filter_map(|(index, costume)| costume.active.then_some(index))
            .collect()
    }

    fn category(active: &[bool]) -> Category {
        Category {
            id: "Ardelia".into(),
            name: "Ardelia".into(),
            image: None,
            character: None,
            extra: false,
            costumes: active
                .iter()
                .enumerate()
                .map(|(index, &active)| Costume {
                    id: format!("costume-{index}"),
                    name: format!("Costume {index}"),
                    image: None,
                    active,
                    censored: false,
                    gamebanana_id: None,
                })
                .collect(),
        }
    }

    #[test]
    fn selecting_with_exactly_one_active_switches_exclusively() {
        let mut character = category(&[true, false, false]);
        select_costume(&mut character, 2);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![false, false, true]
        );
    }

    #[test]
    fn selecting_with_no_active_costumes_activates_only_the_clicked_costume() {
        let mut character = category(&[false, false, false]);
        select_costume(&mut character, 1);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![false, true, false]
        );
    }

    #[test]
    fn selecting_with_multiple_active_costumes_preserves_the_other_active_costumes() {
        let mut character = category(&[true, true, false]);
        select_costume(&mut character, 2);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![true, true, true]
        );
    }

    #[test]
    fn selecting_an_active_costume_with_multiple_active_costumes_toggles_only_it() {
        let mut character = category(&[true, true, false]);
        select_costume(&mut character, 1);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![true, false, false]
        );
    }

    #[test]
    fn selecting_the_only_active_costume_turns_it_off() {
        let mut character = category(&[false, true, false]);
        select_costume(&mut character, 1);
        assert_eq!(
            character
                .costumes
                .iter()
                .map(|costume| costume.active)
                .collect::<Vec<_>>(),
            vec![false, false, false]
        );
    }

    #[test]
    fn switching_categories_restores_each_category_focus() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![
                category(&[true, false, false]),
                category(&[true, false, false]),
            ],
            note: None,
        });
        layouts.carousel_focus = 2;
        layouts.select_category(1);
        assert_eq!(layouts.carousel_focus, 0);
        layouts.carousel_focus = 1;
        layouts.select_category(0);
        assert_eq!(layouts.carousel_focus, 2);
        layouts.select_category(1);
        assert_eq!(layouts.carousel_focus, 1);
    }

    #[test]
    fn queued_mod_navigation_moves_focus_without_activation() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        layouts.navigate_mod(&context, 1);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0]);
    }

    #[test]
    fn direct_focused_exclusive_action_uses_the_same_selection_rule() {
        let mut layouts = test_layouts(1);
        let context = egui::Context::default();
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn exclusive_action_enables_focused_mod_and_disables_the_rest() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 2;
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![2]);
    }

    #[test]
    fn toggle_action_can_remove_the_last_active_mod() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[false, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 1;
        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert!(active_indices(&layouts.catalog.categories[0]).is_empty());
    }

    #[test]
    fn exclusive_action_is_a_noop_when_only_the_focused_mod_is_enabled() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[false, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 1;
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn toggle_action_enables_an_inactive_mod_without_disabling_others() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[false, true, false])],
            note: None,
        });
        let context = egui::Context::default();
        layouts.carousel_focus = 0;
        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![0, 1]);

        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.1,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![1]);
    }

    #[test]
    fn queued_navigation_and_action_are_applied_in_fifo_order() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();
        layouts.navigate_category(&context, 1);
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(active_indices(&layouts.catalog.categories[1]), vec![1]);
    }

    #[test]
    fn shortcut_availability_reflects_focused_mod_state() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, false, false]), category(&[false, false])],
            note: None,
        });
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: true,
                mods: true,
                exclusive: false,
                toggle: true,
                space: Space::Exclusive,
                hotkeys: false,
            }
        );

        layouts.carousel_focus = 1;
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: true,
                mods: true,
                exclusive: true,
                toggle: true,
                space: Space::Exclusive,
                hotkeys: false,
            }
        );
    }

    #[test]
    fn shortcut_availability_handles_empty_category() {
        let layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[])],
            note: None,
        });
        let availability = layouts.shortcut_availability();
        assert!(!availability.categories);
        assert!(!availability.mods);
        assert!(!availability.exclusive);
        assert!(!availability.toggle);
    }

    #[test]
    fn shortcut_availability_marks_multiple_enabled_focus_as_actionable() {
        let mut layouts = Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![category(&[true, true, false])],
            note: None,
        });
        layouts.carousel_focus = 1;
        let availability = layouts.shortcut_availability();
        assert!(availability.mods);
        assert!(availability.exclusive);
        assert!(availability.toggle);
    }

    #[test]
    fn queued_category_mod_exclusive_action_applies_in_order() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();
        layouts.navigate_category(&context, 1);
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.carousel_focus, 1);
        assert_eq!(active_indices(&layouts.catalog.categories[1]), vec![1]);
    }

    #[test]
    fn rows_switch_up_and_down_and_reset_to_categories() {
        let mut layouts = test_layouts(2);
        assert_eq!(layouts.row(), Row::Categories);
        assert_eq!(layouts.selected_category, 0);
        layouts.switch_row(1);
        assert_eq!(layouts.row(), Row::Categories);
        layouts.switch_row(-1);
        assert_eq!(layouts.row(), Row::Mods);
        layouts.switch_row(-1);
        layouts.switch_row(0);
        assert_eq!(layouts.row(), Row::Mods);
        layouts.switch_row(1);
        assert_eq!(layouts.row(), Row::Categories);
        layouts.switch_row(-1);
        layouts.reset_row();
        assert_eq!(layouts.row(), Row::Categories);
    }

    #[test]
    fn mod_and_category_steps_and_actions_work_from_either_row() {
        let mut layouts = test_layouts(2);
        let context = egui::Context::default();
        layouts.switch_row(1);
        layouts.navigate_mod(&context, 1);
        layouts.navigate_category(&context, 1);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.row(), Row::Categories);
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.carousel_focus_by_category[0], 1);

        // Enabling acts on the focused mod while the categories row is current.
        layouts.navigate_mod(&context, 1);
        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.1,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.row(), Row::Categories);
        assert_eq!(active_indices(&layouts.catalog.categories[1]), vec![1]);

        layouts.switch_row(-1);
        layouts.navigate_category(&context, -1);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.2,
            Vec::new(),
            false,
        );
        assert_eq!(layouts.row(), Row::Mods);
        assert_eq!(layouts.selected_category, 0);
        assert_eq!(layouts.carousel_focus, 1);
    }

    fn named(name: &str, mods: &[&str]) -> Category {
        Category {
            id: name.into(),
            name: name.into(),
            image: None,
            character: None,
            extra: false,
            costumes: mods
                .iter()
                .map(|name| Costume {
                    id: (*name).into(),
                    name: (*name).into(),
                    image: None,
                    active: false,
                    censored: false,
                    gamebanana_id: None,
                })
                .collect(),
        }
    }

    fn search_layouts() -> Layouts {
        Layouts::new(Catalog {
            game: "Test".into(),
            categories: vec![
                named(
                    "Operators: Ardelia",
                    &["Summer Vow", "Beach_Day", "vow_of_dawn", "Classic"],
                ),
                named("Endministrator", &[]),
                named("Perlica", &["Night  Vow v1.2", "Classic"]),
                named("Vow Keepers", &["Alpha", "Beta"]),
            ],
            note: None,
        })
    }

    #[test]
    fn search_matches_names_loosely() {
        let layouts = search_layouts();
        let filter = |query: &str| Filter::new(&layouts.catalog, query, &layouts.gamebanana);

        // Case, underscores and repeated spaces do not matter.
        let vow = filter("VOW");
        assert_eq!(vow.categories, vec![0, 2, 3]);
        assert_eq!(vow.mods, vec![vec![0, 2], vec![], vec![0], vec![0, 1]]);
        assert_eq!(filter("beach day").mods[0], vec![1]);
        assert_eq!(filter("vow of").mods[0], vec![2]);
        assert_eq!(filter("night vow v1.2").mods[2], vec![0]);

        // A matching category shows all its mods, even when it has none.
        let keepers = filter("keep");
        assert_eq!(keepers.categories, vec![3]);
        assert_eq!(keepers.mods[3], vec![0, 1]);
        assert_eq!(filter("endmin").categories, vec![1]);

        // A mod name can match in several categories.
        let classic = filter("classic");
        assert_eq!(classic.categories, vec![0, 2]);
        assert_eq!(classic.mods, vec![vec![3], vec![], vec![1], vec![]]);

        // The rail shows the character name, so the group prefix does not match.
        assert!(filter("operators").categories.is_empty());

        // An empty search, or one of only spaces, shows everything.
        for query in ["", "   "] {
            let all = filter(query);
            assert_eq!(all.categories, vec![0, 1, 2, 3]);
            assert_eq!(
                all.mods,
                vec![vec![0, 1, 2, 3], vec![], vec![0, 1], vec![0, 1]]
            );
        }
    }

    #[test]
    fn search_keeps_a_selection_it_shows_and_otherwise_moves_to_the_first_match() {
        let mut layouts = search_layouts();
        assert_eq!(layouts.selected_category, 0);
        layouts.carousel_focus = 2;

        // Ardelia still shows the focused mod.
        layouts.set_search("vow");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (0, 2));

        // Ardelia still shows, but not the focused mod.
        layouts.set_search("beach");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (0, 1));
        assert_eq!(layouts.carousel_focus_by_category[0], 1);

        // Ardelia is hidden, so the first match takes over, and Perlica's
        // remembered mod gives way to the one that matches.
        layouts.carousel_focus_by_category[2] = 1;
        layouts.set_search("night");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (2, 0));

        // Clearing the search stays where the search led.
        layouts.set_search("");
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (2, 0));
    }

    #[test]
    fn keys_step_over_what_the_search_hides() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        let size = Vec2::new(560.0, 266.0);
        layouts.set_search("vow");

        layouts.navigate_mod(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.0, Vec::new(), false);
        assert_eq!(layouts.carousel_focus, 2);

        // The last match is an end, like the last mod.
        layouts.navigate_mod(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.1, Vec::new(), false);
        assert_eq!(layouts.carousel_focus, 2);
        layouts.navigate_mod(&context, -1);
        run_layout_frame(&context, &mut layouts, size, 1.2, Vec::new(), false);
        assert_eq!(layouts.carousel_focus, 0);

        // Endministrator is hidden.
        layouts.navigate_category(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.3, Vec::new(), false);
        assert_eq!(layouts.selected_category, 2);
        layouts.navigate_category(&context, 1);
        layouts.navigate_category(&context, 1);
        run_layout_frame(&context, &mut layouts, size, 1.4, Vec::new(), false);
        assert_eq!(layouts.selected_category, 3);
        layouts.navigate_category(&context, -1);
        layouts.navigate_category(&context, -1);
        run_layout_frame(&context, &mut layouts, size, 1.5, Vec::new(), false);
        assert_eq!(layouts.selected_category, 0);
    }

    #[test]
    fn search_shows_only_matching_cards() {
        let mut layouts = search_layouts();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        layouts.set_search("vow");
        // Two matches lay out like a two-mod carousel.
        let cards = layouts.current_carousel_placements(rect, 1.0);
        let mut shown = cards.iter().map(|card| card.index).collect::<Vec<_>>();
        shown.sort_unstable();
        assert_eq!(shown, vec![0, 2]);
        let two = carousel_card_placements(rect, 2, 0, false);
        for (card, plain) in cards.iter().zip(&two) {
            assert_eq!((card.rect, card.slot), (plain.rect, plain.slot));
        }
    }

    #[test]
    fn enabling_a_search_result_still_disables_the_hidden_mods() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        layouts.catalog.categories[0].costumes[1].active = true;
        layouts.catalog.categories[0].costumes[3].active = true;
        layouts.set_search("dawn");
        assert_eq!(layouts.carousel_focus, 2);

        layouts.apply_focused_action(&context, ModAction::Exclusive);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert_eq!(active_indices(&layouts.catalog.categories[0]), vec![2]);
    }

    #[test]
    fn a_mod_the_search_hides_cannot_be_enabled() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        layouts.set_search("dawn");
        layouts.carousel_focus = 1;

        layouts.apply_focused_action(&context, ModAction::Toggle);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 266.0),
            1.0,
            Vec::new(),
            false,
        );
        assert!(active_indices(&layouts.catalog.categories[0]).is_empty());
    }

    #[test]
    fn shortcut_availability_follows_the_search() {
        let mut layouts = search_layouts();
        // One category and one mod show, so there is nowhere to step.
        layouts.set_search("dawn");
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: false,
                mods: false,
                exclusive: true,
                toggle: true,
                space: Space::Exclusive,
                hotkeys: false,
            }
        );

        layouts.set_search("vow");
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: true,
                mods: true,
                exclusive: true,
                toggle: true,
                space: Space::Exclusive,
                hotkeys: false,
            }
        );

        layouts.carousel_focus = 1;
        let hidden = layouts.shortcut_availability();
        assert!(!hidden.exclusive && !hidden.toggle);

        layouts.set_search("zzz");
        assert_eq!(
            layouts.shortcut_availability(),
            ShortcutAvailability {
                categories: false,
                mods: false,
                exclusive: false,
                toggle: false,
                space: Space::Exclusive,
                hotkeys: false,
            }
        );
    }

    #[test]
    fn the_gamebanana_line_is_part_of_what_the_window_shows() {
        let mut ardelia = named("Ardelia", &["Vow", "Beach"]);
        ardelia.character = Some(7);
        let mut layouts = Layouts::new(live_catalog(vec![ardelia]));
        let context = egui::Context::default();
        layouts.set_gamebanana(true, false);
        layouts.update_gamebanana(&context, 0.0);
        layouts.update_gamebanana(&context, 1.0);
        layouts.receive_gamebanana(vec![gamebanana_page(7, &[1, 3], false)]);
        layouts.carousel_focus = 1;
        layouts.set_reveal(1.0);
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            2.0,
            Vec::new(),
            false,
        );
        // The window shows only these, and the line sits in the gap between
        // Beach and Mod 1.
        let rects = layouts.visible_card_rects();
        let line = rects
            .iter()
            .find(|rect| rect.width() < 40.0)
            .expect("the line's area");
        assert!(rects.iter().any(|card| card.max.x <= line.min.x));
        assert!(rects.iter().any(|card| card.min.x >= line.max.x));
    }

    #[test]
    fn a_search_without_matches_says_so() {
        let mut layouts = search_layouts();
        let context = egui::Context::default();
        layouts.set_search("zzz");
        assert_eq!(layouts.empty_carousel_text(), "No matches");

        // The selection stays put, the rail is empty, and the carousel shows
        // only the notice.
        run_layout_frame(
            &context,
            &mut layouts,
            Vec2::new(560.0, 344.0),
            1.0,
            Vec::new(),
            true,
        );
        assert_eq!(layouts.selected_category, 0);
        assert_eq!(layouts.visible_card_rects().len(), 1);

        // A category that matches by name but has no mods is still empty.
        layouts.set_search("endmin");
        assert_eq!(layouts.selected_category, 1);
        assert_eq!(layouts.empty_carousel_text(), "No installed mods");
    }

    fn with_active(mut category: Category, active: &str) -> Category {
        for costume in &mut category.costumes {
            costume.active = costume.id == active;
        }
        category
    }

    fn live_catalog(categories: Vec<Category>) -> Catalog {
        Catalog {
            game: "Test".into(),
            categories,
            note: None,
        }
    }

    #[test]
    fn a_new_library_keeps_the_selection_by_id() {
        let mut layouts = Layouts::new(live_catalog(vec![
            named("Akekuri", &["A1", "A2"]),
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            named("Perlica", &["P1", "P2"]),
        ]));
        layouts.select_category(1);
        layouts.carousel_focus = 2;
        layouts.carousel_focus_by_category[2] = 1;
        assert_eq!(
            layouts.selection(),
            Selection {
                category_id: Some("Ardelia".into()),
                mods: [
                    ("Ardelia".to_owned(), "Classic".to_owned()),
                    ("Perlica".to_owned(), "P2".to_owned()),
                ]
                .into(),
            }
        );

        // A new category sorts first and Ardelia gains a mod before Classic.
        layouts.replace_catalog(live_catalog(vec![
            named("Aglina", &["G1"]),
            named("Akekuri", &["A1", "A2"]),
            named("Ardelia", &["Vow", "Beach", "Autumn", "Classic"]),
            named("Perlica", &["P1", "P2"]),
        ]));
        assert_eq!(layouts.selected_category, 2);
        assert_eq!(layouts.carousel_focus, 3);
        assert_eq!(layouts.carousel_focus_by_category[3], 1);
        assert!(layouts.reveal_category);

        // Without Ardelia the selection stays at its place in the rail.
        layouts.replace_catalog(live_catalog(vec![
            named("Aglina", &["G1"]),
            named("Akekuri", &["A1", "A2"]),
            named("Perlica", &["P1", "P2"]),
        ]));
        assert_eq!(layouts.selected_category, 2);
        assert_eq!(layouts.carousel_focus, 1);
    }

    #[test]
    fn a_category_left_on_its_active_mod_follows_the_active_mod() {
        let mut layouts = Layouts::new(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            "Vow",
        )]));
        assert_eq!(layouts.selection().mods.len(), 0);
        layouts.replace_catalog(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            "Classic",
        )]));
        assert_eq!(layouts.carousel_focus, 2);

        // Once moved away from the active mod, the focus stays put.
        layouts.carousel_focus = 1;
        layouts.replace_catalog(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            "Vow",
        )]));
        assert_eq!(layouts.carousel_focus, 1);
    }

    #[test]
    fn a_new_library_keeps_the_search() {
        let mut layouts = search_layouts();
        layouts.set_search("classic");
        layouts.select_category(2);
        layouts.carousel_focus = 1;
        let mut catalog = search_layouts().catalog;
        catalog.categories.remove(1);
        layouts.replace_catalog(catalog);
        assert_eq!(layouts.filter.query, "classic");
        assert_eq!(layouts.filter.categories, vec![0, 1]);
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (1, 1));
    }

    #[test]
    fn a_restored_selection_skips_what_is_gone() {
        let mut layouts = Layouts::new(live_catalog(vec![
            named("Akekuri", &["A1", "A2"]),
            named("Ardelia", &["Vow", "Beach"]),
        ]));
        layouts.restore_selection(&Selection {
            category_id: Some("Ardelia".into()),
            mods: [
                ("Ardelia".to_owned(), "Beach".to_owned()),
                ("Akekuri".to_owned(), "Removed".to_owned()),
            ]
            .into(),
        });
        assert_eq!((layouts.selected_category, layouts.carousel_focus), (1, 1));
        assert_eq!(layouts.carousel_focus_by_category[0], 0);
        layouts.restore_selection(&Selection {
            category_id: Some("Removed".into()),
            mods: Default::default(),
        });
        assert_eq!(layouts.selected_category, 0);
    }

    fn request(mod_id: &str, action: ChangeAction, states: &[(&str, bool)]) -> ModRequest {
        ModRequest {
            mod_id: mod_id.into(),
            action,
            states: states
                .iter()
                .map(|(id, active)| ((*id).into(), *active))
                .collect(),
        }
    }

    #[test]
    fn keys_and_clicks_ask_hestia_for_what_they_show() {
        let mut layouts = Layouts::new(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach", "Classic"]),
            "Vow",
        )]));
        layouts.carousel_focus = 1;
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        // Using the mod that is already the only one on changes nothing.
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        layouts.apply_focused_costume_action(ModAction::Toggle, 1.0);
        layouts.activate_costume_in_place(0, 2, 1.0);
        // A click on a mod that's on turns it off, even the only one.
        layouts.activate_costume_in_place(0, 2, 1.0);
        assert_eq!(
            layouts.take_requests(),
            [
                request(
                    "Beach",
                    ChangeAction::Use,
                    &[("Vow", false), ("Beach", true)]
                ),
                request("Beach", ChangeAction::TurnOff, &[("Beach", false)]),
                request("Classic", ChangeAction::Use, &[("Classic", true)]),
                request("Classic", ChangeAction::TurnOff, &[("Classic", false)]),
            ]
        );
        assert!(layouts.take_requests().is_empty());
    }

    #[test]
    fn in_the_game_a_change_shows_that_it_waits_in_the_frame_it_is_made() {
        let mut layouts = Layouts::new(live_catalog(vec![with_active(
            named("Ardelia", &["Vow", "Beach"]),
            "Vow",
        )]));
        layouts.carousel_focus = 1;
        // The preview makes changes on screen only.
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        assert_eq!(layouts.waiting, None);
        layouts.set_waiting(HashSet::new());
        layouts.apply_focused_costume_action(ModAction::Toggle, 1.0);
        layouts.activate_costume_in_place(0, 0, 1.0);
        assert_eq!(
            layouts.waiting,
            Some(HashSet::from(["Beach".to_owned(), "Vow".to_owned()]))
        );
    }

    #[test]
    fn using_an_uncategorized_mod_leaves_the_others_on() {
        let mut loose = with_active(named("Uncategorized", &["UI", "Shader"]), "UI");
        loose.id = UNCATEGORIZED_ID.into();
        let mut layouts = Layouts::new(live_catalog(vec![loose]));
        assert!(!layouts.shortcut_availability().exclusive);
        layouts.carousel_focus = 1;
        assert!(layouts.shortcut_availability().exclusive);
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        assert_eq!(
            layouts.take_requests(),
            [request("Shader", ChangeAction::Use, &[("Shader", true)])]
        );
        // Both on now, so there is nothing left to use.
        assert!(!layouts.shortcut_availability().exclusive);
        // A click turns one off and leaves the other on.
        layouts.activate_costume_in_place(0, 0, 1.0);
        assert_eq!(
            layouts.take_requests(),
            [request("UI", ChangeAction::TurnOff, &[("UI", false)])]
        );
    }

    fn gamebanana_page(character: u64, ids: &[u64], more: bool) -> ToOverlay {
        ToOverlay::BrowsePage(crate::overlay_protocol::BrowsePage {
            character,
            query: String::new(),
            page: 1,
            mods: ids
                .iter()
                .map(|&id| BrowseMod {
                    id,
                    name: format!("Mod {id}"),
                    ..Default::default()
                })
                .collect(),
            more,
            error: None,
        })
    }

    #[test]
    fn gamebanana_mods_follow_the_own_ones_without_the_installed() {
        let mut ardelia = named("Ardelia", &["Vow", "Beach"]);
        ardelia.character = Some(7);
        ardelia.costumes[1].gamebanana_id = Some(2);
        let mut layouts = Layouts::new(live_catalog(vec![ardelia]));
        let context = egui::Context::default();
        layouts.update_gamebanana(&context, 0.0);
        assert_eq!(layouts.visible_mods(), [0, 1], "off outside the overlay");
        layouts.set_gamebanana(true, false);
        layouts.update_gamebanana(&context, 0.0);
        assert_eq!(layouts.visible_mods(), [0, 1, 2], "a loading card");
        assert_eq!(
            layouts.take_gamebanana_requests(),
            [FromOverlay::ListCharacters],
            "only the characters, for their pictures"
        );
        layouts.update_gamebanana(&context, 1.0);
        assert_eq!(layouts.take_gamebanana_requests().len(), 1);
        layouts.receive_gamebanana(vec![gamebanana_page(7, &[1, 2, 3], false)]);
        // Mod 2 is Beach, so the cards are Vow, Beach, Mod 1 and Mod 3.
        assert_eq!(layouts.visible_mods(), [0, 1, 2, 4]);
        layouts.carousel_focus = 4;
        let selection = layouts.selection();
        assert_eq!(selection.mods["Ardelia"], "gamebanana:3");
        layouts.carousel_focus = 0;
        layouts.restore_selection(&selection);
        assert_eq!(layouts.carousel_focus, 4);

        layouts.set_gamebanana(false, false);
        assert_eq!(layouts.visible_mods(), [0, 1], "off in Settings");
        assert!(layouts.visible_mods().contains(&layouts.carousel_focus));
        layouts.set_gamebanana(true, false);
        assert_eq!(layouts.visible_mods(), [0, 1, 2, 4], "the pages it had");
    }

    #[test]
    fn a_gamebanana_list_loading_leaves_the_cards_in_place() {
        let carousel = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        let cards = |layouts: &Layouts| {
            layouts
                .target_placements(carousel)
                .into_iter()
                .map(|card| (card.index, card.rect, card.slot, card.focused))
                .collect::<Vec<_>>()
        };
        let mut akekuri = named("Akekuri", &["Flame"]);
        akekuri.character = Some(7);
        let mut layouts = Layouts::new(live_catalog(vec![akekuri]));
        let context = egui::Context::default();
        let alone = cards(&layouts);
        layouts.set_gamebanana(true, false);
        layouts.update_gamebanana(&context, 0.0);
        assert_eq!(layouts.visible_mods(), [0, 1], "a loading card");
        let loading = cards(&layouts);
        assert_eq!(loading.iter().find(|card| card.3), alone.first());
        layouts.update_gamebanana(&context, 1.0);
        layouts.receive_gamebanana(vec![gamebanana_page(7, &[1], false)]);
        assert_eq!(layouts.visible_mods(), [0, 1], "the mod in its place");
        assert_eq!(cards(&layouts), loading);
    }

    #[test]
    fn show_all_characters_adds_the_unlinked_ones_after_the_library() {
        let mut ardelia = named("Ardelia", &["Vow"]);
        ardelia.character = Some(7);
        let mut layouts = Layouts::new(live_catalog(vec![ardelia]));
        layouts.set_gamebanana(true, false);
        layouts.set_show_all_characters(true);
        assert_eq!(
            layouts.take_gamebanana_requests(),
            [FromOverlay::ListCharacters]
        );
        let character = |id, name: &str| crate::overlay_protocol::Character {
            id,
            name: name.into(),
            image: None,
        };
        layouts.receive_gamebanana(vec![ToOverlay::Characters {
            characters: vec![
                character(9, "perlica"),
                character(7, "Ardelia"),
                character(8, "Endministrator"),
            ],
            error: None,
        }]);
        let names = |layouts: &Layouts| {
            layouts
                .catalog
                .categories
                .iter()
                .map(|category| category.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&layouts), ["Ardelia", "Endministrator", "perlica"]);
        assert_eq!(layouts.catalog.categories[1].id, "gamebanana:character:8");
        // An extra character without mods yet still shows, for its list.
        assert_eq!(layouts.filter.categories, [0, 1, 2]);
        layouts.set_show_all_characters(false);
        assert_eq!(names(&layouts), ["Ardelia"]);
    }

    #[test]
    fn a_category_without_a_link_takes_the_character_its_name_stands_for() {
        let mut ardelia = named("Operators: Ardelia", &["Vow"]);
        ardelia.costumes[0].image = Some(PathBuf::from("vow.png"));
        let mut layouts = Layouts::new(live_catalog(vec![ardelia]));
        layouts.set_gamebanana(true, true);
        assert_eq!(
            layouts.take_gamebanana_requests(),
            [FromOverlay::ListCharacters]
        );
        layouts.receive_gamebanana(vec![ToOverlay::Characters {
            characters: vec![
                crate::overlay_protocol::Character {
                    id: 7,
                    name: "Ardelia".into(),
                    image: Some(PathBuf::from("ardelia-icon.png")),
                },
                crate::overlay_protocol::Character {
                    id: 8,
                    name: "Endministrator".into(),
                    image: None,
                },
            ],
            error: None,
        }]);
        let categories = &layouts.catalog.categories;
        assert_eq!(categories.len(), 2, "Ardelia isn't added a second time");
        assert_eq!(categories[0].character, Some(7));
        assert_eq!(
            categories[0].image.as_deref(),
            Some(Path::new("ardelia-icon.png")),
            "the character's picture, not the mod's"
        );
        assert_eq!(categories[1].id, "gamebanana:character:8");

        // A new library from Hestia keeps both.
        layouts.replace_catalog(layouts.own_catalog());
        assert_eq!(layouts.catalog.categories[0].character, Some(7));
        assert_eq!(layouts.catalog.categories.len(), 2);
    }

    fn installed_from_gamebanana(name: &str, gamebanana_id: u64) -> Costume {
        Costume {
            id: name.into(),
            name: name.into(),
            image: None,
            active: false,
            censored: false,
            gamebanana_id: Some(gamebanana_id),
        }
    }

    fn install_update(mod_id: u64, stage: InstallStage) -> ToOverlay {
        ToOverlay::Install(crate::overlay_protocol::InstallUpdate {
            mod_id,
            stage,
            name: None,
        })
    }

    #[test]
    fn space_installs_a_gamebanana_mod_and_the_focus_follows_it_in() {
        let mut ardelia = named("Ardelia", &["Vow", "Zest"]);
        ardelia.character = Some(7);
        let mut layouts = Layouts::new(live_catalog(vec![ardelia.clone()]));
        layouts.set_gamebanana(true, false);
        let context = egui::Context::default();
        layouts.update_gamebanana(&context, 0.0);
        layouts.update_gamebanana(&context, 1.0);
        layouts.take_gamebanana_requests();
        layouts.receive_gamebanana(vec![gamebanana_page(7, &[1, 2], false)]);
        // Vow, Zest, Mod 1, Mod 2.
        layouts.carousel_focus = 3;
        assert!(layouts.shortcut_availability().exclusive);
        assert_eq!(layouts.shortcut_availability().space, Space::Install);
        layouts.apply_focused_costume_action(ModAction::Toggle, 1.0);
        assert!(layouts.take_gamebanana_requests().is_empty(), "only Space");
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        assert_eq!(
            layouts.take_gamebanana_requests(),
            [FromOverlay::Install(crate::overlay_protocol::Install {
                mod_id: 2,
                name: "Mod 2".into(),
                category_id: Some("Ardelia".into()),
            })]
        );
        assert!(!layouts.shortcut_availability().exclusive, "installing");
        layouts.receive_gamebanana(vec![install_update(2, InstallStage::Failed)]);
        assert_eq!(layouts.shortcut_availability().space, Space::TryAgain);
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.5);
        assert_eq!(layouts.take_gamebanana_requests().len(), 1, "tried again");

        // Hestia installs it in its sorted place, turned off.
        ardelia
            .costumes
            .insert(1, installed_from_gamebanana("Mod 2", 2));
        layouts.replace_catalog(live_catalog(vec![ardelia]));
        // It waits behind its card: Vow, Zest, Mod 1, Mod 2.
        assert_eq!(layouts.visible_mods(), [0, 2, 3, 4]);
        assert_eq!(layouts.carousel_focus, 4);
        let news = layouts.receive_gamebanana(vec![install_update(
            2,
            InstallStage::Installed {
                mods: vec!["Mod 2".into()],
            },
        )]);
        assert_eq!(news, [InstallNews::Installed("Mod 2".into())]);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        layouts.cross_installs(&context, rect, 2.0);
        assert_eq!(layouts.carousel_focus, 4, "it finishes in place first");
        layouts.cross_installs(&context, rect, 2.7);
        assert_eq!(layouts.visible_mods(), [0, 1, 2, 3]);
        assert_eq!(layouts.carousel_focus, 1);
        assert!(layouts.take_arrivals().is_empty(), "it stays in sight");
        assert!(layouts.new_mods.contains("Mod 2"));
        layouts.apply_focused_costume_action(ModAction::Exclusive, 3.0);
        assert!(layouts.new_mods.is_empty(), "used, so no longer new");
    }

    #[test]
    fn a_download_hestias_window_started_holds_its_card_like_an_install() {
        let mut ardelia = named("Ardelia", &["Vow", "Zest"]);
        ardelia.character = Some(7);
        let mut layouts = Layouts::new(live_catalog(vec![ardelia.clone()]));
        layouts.set_gamebanana(true, false);
        let context = egui::Context::default();
        layouts.update_gamebanana(&context, 0.0);
        layouts.update_gamebanana(&context, 1.0);
        layouts.take_gamebanana_requests();
        layouts.receive_gamebanana(vec![gamebanana_page(7, &[1, 2], false)]);
        let window = |mod_id: u64, stage: InstallStage| {
            ToOverlay::Install(crate::overlay_protocol::InstallUpdate {
                mod_id,
                stage,
                name: Some(format!("Mod {mod_id}")),
            })
        };
        let news = layouts.receive_gamebanana(vec![
            window(1, InstallStage::Downloading { percent: Some(30) }),
            window(2, InstallStage::Waiting),
        ]);
        assert!(news.is_empty());
        // Vow, Zest, Mod 1, Mod 2.
        layouts.carousel_focus = 3;
        assert!(
            !layouts.shortcut_availability().exclusive,
            "Hestia installs it"
        );
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        assert!(layouts.take_gamebanana_requests().is_empty());

        // The window installed Mod 1 turned on, and Mod 2 turned off.
        let mut on = installed_from_gamebanana("Mod 1", 1);
        on.active = true;
        ardelia.costumes.push(on);
        ardelia.costumes.push(installed_from_gamebanana("Mod 2", 2));
        layouts.replace_catalog(live_catalog(vec![ardelia]));
        let news = layouts.receive_gamebanana(vec![
            window(
                1,
                InstallStage::Installed {
                    mods: vec!["Mod 1".into()],
                },
            ),
            window(
                2,
                InstallStage::Installed {
                    mods: vec!["Mod 2".into()],
                },
            ),
        ]);
        assert_eq!(
            news,
            [
                InstallNews::InstalledOn("Mod 1".into()),
                InstallNews::Installed("Mod 2".into())
            ]
        );
        assert!(!layouts.new_mods.contains("Mod 1"), "in use");
        assert!(layouts.new_mods.contains("Mod 2"));
    }

    #[test]
    fn an_extra_characters_install_asks_then_lands_in_its_new_category() {
        let mut layouts = Layouts::new(live_catalog(vec![named("Ardelia", &["Vow"])]));
        layouts.set_gamebanana(true, false);
        layouts.set_show_all_characters(true);
        layouts.receive_gamebanana(vec![ToOverlay::Characters {
            characters: vec![crate::overlay_protocol::Character {
                id: 8,
                name: "Endministrator".into(),
                image: None,
            }],
            error: None,
        }]);
        layouts.select_category(1);
        let context = egui::Context::default();
        layouts.update_gamebanana(&context, 0.0);
        layouts.update_gamebanana(&context, 1.0);
        layouts.take_gamebanana_requests();
        layouts.receive_gamebanana(vec![gamebanana_page(8, &[5], false)]);
        layouts.activate_costume_in_place(1, 0, 1.0);
        assert!(matches!(
            &layouts.take_gamebanana_requests()[..],
            [FromOverlay::Install(crate::overlay_protocol::Install {
                mod_id: 5,
                category_id: None,
                ..
            })]
        ));

        let file = |id| crate::overlay_protocol::InstallFile {
            id,
            name: format!("{id}.zip"),
            size: 1,
            description: None,
        };
        let news = layouts.receive_gamebanana(vec![install_update(
            5,
            InstallStage::ChooseFile {
                files: vec![file(30), file(31)],
            },
        )]);
        assert_eq!(news, [InstallNews::Question("Mod 5".into())]);
        assert!(layouts.question_open());
        // Down past Cancel stays on it, then back up to the second file.
        for _ in 0..3 {
            layouts.move_question(1);
        }
        layouts.move_question(-1);
        layouts.answer_question();
        assert_eq!(
            layouts.take_gamebanana_requests(),
            [FromOverlay::PickFile {
                mod_id: 5,
                file_id: 31
            }]
        );
        assert!(!layouts.question_open());

        // Hestia made a category for the character.
        let mut endministrator = named("Endministrator", &[]);
        endministrator.id = "new-category".into();
        endministrator.character = Some(8);
        endministrator
            .costumes
            .push(installed_from_gamebanana("Mod 5", 5));
        layouts.replace_catalog(live_catalog(vec![
            named("Ardelia", &["Vow"]),
            endministrator,
        ]));
        assert_eq!(layouts.catalog.categories.len(), 2);
        assert_eq!(
            layouts.selection().category_id.as_deref(),
            Some("new-category")
        );
        // Still on its card, after the mod that waits behind it.
        assert_eq!(layouts.carousel_focus, 1);
        layouts.receive_gamebanana(vec![install_update(
            5,
            InstallStage::Installed {
                mods: vec!["Mod 5".into()],
            },
        )]);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        layouts.cross_installs(&context, rect, 2.0);
        layouts.cross_installs(&context, rect, 3.0);
        assert_eq!(layouts.carousel_focus, 0);
        assert_eq!(layouts.visible_mods(), [0]);
    }

    #[test]
    fn a_mod_that_crosses_out_of_sight_says_where_it_went() {
        let mut ardelia = named("Ardelia", &["A", "B", "C", "D", "E"]);
        ardelia.character = Some(7);
        let mut layouts = Layouts::new(live_catalog(vec![ardelia.clone()]));
        layouts.set_gamebanana(true, false);
        let context = egui::Context::default();
        layouts.carousel_focus = 4;
        layouts.update_gamebanana(&context, 0.0);
        layouts.update_gamebanana(&context, 1.0);
        layouts.take_gamebanana_requests();
        layouts.receive_gamebanana(vec![gamebanana_page(7, &[1, 2, 3, 4], false)]);
        layouts.carousel_focus = 5;
        layouts.apply_focused_costume_action(ModAction::Exclusive, 1.0);
        // On to the last GameBanana card while it installs.
        layouts.carousel_focus = 8;
        ardelia
            .costumes
            .insert(0, installed_from_gamebanana("0 First", 1));
        layouts.replace_catalog(live_catalog(vec![ardelia]));
        layouts.receive_gamebanana(vec![install_update(
            1,
            InstallStage::Installed {
                mods: vec!["0 First".into()],
            },
        )]);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(560.0, 266.0));
        layouts.cross_installs(&context, rect, 2.0);
        layouts.cross_installs(&context, rect, 3.0);
        assert_eq!(layouts.take_arrivals(), ["0 First"]);
        // The focus stays on the card it was on.
        assert_eq!(
            layouts.card_id(0, layouts.carousel_focus).as_deref(),
            Some("gamebanana:4")
        );
    }
}
