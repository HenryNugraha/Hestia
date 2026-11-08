use std::{
    collections::{HashMap, hash_map::Entry},
    path::PathBuf,
};

use crate::overlay_protocol::{
    self, Browse, BrowseMod, BrowsePage, Character, FromOverlay, InstallStage, InstallUpdate,
    Pictures, SameNameChoice,
};

const SETTLE_SECS: f64 = 0.3;

pub(super) const LOAD_AHEAD: usize = 3;

const HOLD_SECS: f64 = 0.6;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ListKey {
    pub character: u64,
    pub query: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Status {
    Settling(f64),
    Loading(u32),
    More,
    End,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StatusCard {
    Loading,
    Failed,
    Empty,
}

#[derive(Debug)]
pub(super) struct List {
    pub mods: Vec<BrowseMod>,
    pages: u32,
    status: Status,
}

impl List {
    pub(super) fn status_card(&self) -> Option<StatusCard> {
        match self.status {
            Status::Settling(_) | Status::Loading(_) | Status::More => Some(StatusCard::Loading),
            Status::Failed => Some(StatusCard::Failed),
            Status::End if self.mods.is_empty() => Some(StatusCard::Empty),
            Status::End => None,
        }
    }
}

#[derive(Debug, Default)]
enum Characters {
    #[default]
    NotAsked,
    Asking,
    Have(Vec<Character>),
    Failed,
}

#[derive(Clone, Debug)]
pub(super) struct Install {
    pub name: String,
    pub stage: InstallStage,
    asked: u64,
    hold_until: Option<f64>,
    crossed: bool,
}

impl Install {
    pub(super) fn holds(&self) -> bool {
        self.running() || matches!(self.stage, InstallStage::Installed { .. }) && !self.crossed
    }

    fn running(&self) -> bool {
        runs(&self.stage)
    }

    fn asking(&self) -> bool {
        matches!(
            self.stage,
            InstallStage::ChooseFile { .. } | InstallStage::SameName { .. }
        )
    }
}

fn runs(stage: &InstallStage) -> bool {
    !matches!(
        stage,
        InstallStage::Installed { .. } | InstallStage::Failed | InstallStage::Canceled
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InstallNews {
    Installed(String),
    InstalledOn(String),
    Failed(String),
    Question(String),
}

#[derive(Debug, Default)]
pub(super) struct GameBanana {
    enabled: bool,
    lists: HashMap<ListKey, List>,
    characters: Characters,
    installs: HashMap<u64, Install>,
    questions_asked: u64,
    requests: Vec<FromOverlay>,
}

impl GameBanana {
    pub(super) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn list(&self, key: &ListKey) -> Option<&List> {
        self.lists.get(key).filter(|_| self.enabled)
    }

    pub(super) fn want(&mut self, key: &ListKey, now: f64) -> bool {
        if !self.enabled {
            return false;
        }
        let Some(list) = self.lists.get_mut(key) else {
            self.lists.insert(
                key.clone(),
                List {
                    mods: Vec::new(),
                    pages: 0,
                    status: Status::Settling(now + SETTLE_SECS),
                },
            );
            return true;
        };
        match list.status {
            Status::Settling(at) if now >= at => {}
            Status::More => {}
            _ => return false,
        }
        let page = list.pages + 1;
        list.status = Status::Loading(page);
        self.requests.push(FromOverlay::Browse(Browse {
            character: key.character,
            query: key.query.clone(),
            page,
        }));
        false
    }

    pub(super) fn settles_at(&self, key: &ListKey) -> Option<f64> {
        match self.lists.get(key)?.status {
            Status::Settling(at) => Some(at),
            _ => None,
        }
    }

    pub(super) fn receive_page(&mut self, page: BrowsePage) -> bool {
        let key = ListKey {
            character: page.character,
            query: page.query,
        };
        let Some(list) = self.lists.get_mut(&key) else {
            return false;
        };
        if list.status != Status::Loading(page.page) {
            return false;
        }
        if page.error.is_some() {
            list.status = Status::Failed;
            return true;
        }
        for item in page.mods {
            if !list.mods.iter().any(|known| known.id == item.id) {
                list.mods.push(item);
            }
        }
        list.pages = page.page;
        list.status = if page.more { Status::More } else { Status::End };
        true
    }

    pub(super) fn retry(&mut self, key: &ListKey) {
        if let Some(list) = self.lists.get_mut(key)
            && list.status == Status::Failed
        {
            let page = list.pages + 1;
            list.status = Status::Loading(page);
            self.requests.push(FromOverlay::Browse(Browse {
                character: key.character,
                query: key.query.clone(),
                page,
            }));
        }
    }

    pub(super) fn apply_pictures(&mut self, pictures: &Pictures) -> bool {
        let mods: HashMap<u64, &PathBuf> =
            pictures.mods.iter().map(|(id, path)| (*id, path)).collect();
        for item in self.lists.values_mut().flat_map(|list| &mut list.mods) {
            if let Some(path) = mods.get(&item.id) {
                item.image = Some(PathBuf::clone(path));
            }
        }
        let mut characters_changed = false;
        if let Characters::Have(characters) = &mut self.characters {
            for (id, path) in &pictures.characters {
                if let Some(character) = characters.iter_mut().find(|character| character.id == *id)
                {
                    character.image = Some(path.clone());
                    characters_changed = true;
                }
            }
        }
        characters_changed
    }

    pub(super) fn ask_characters(&mut self) {
        if self.enabled && matches!(self.characters, Characters::NotAsked | Characters::Failed) {
            self.characters = Characters::Asking;
            self.requests.push(FromOverlay::ListCharacters);
        }
    }

    pub(super) fn set_characters(&mut self, characters: Vec<Character>, error: Option<String>) {
        self.characters = match error {
            Some(error) => {
                tracing::warn!(%error, "Hestia couldn't list the GameBanana characters");
                Characters::Failed
            }
            None => Characters::Have(characters),
        };
    }

    pub(super) fn characters(&self) -> &[Character] {
        match &self.characters {
            Characters::Have(characters) => characters,
            _ => &[],
        }
    }

    pub(super) fn censored_images(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.lists
            .values()
            .flat_map(|list| &list.mods)
            .filter(|item| item.censored)
            .filter_map(|item| item.image.clone())
    }

    pub(super) fn install(&mut self, item: &BrowseMod, category_id: Option<String>) -> bool {
        if !self.enabled || self.installs.get(&item.id).is_some_and(Install::holds) {
            return false;
        }
        self.installs.insert(
            item.id,
            Install {
                name: item.name.clone(),
                stage: InstallStage::Waiting,
                asked: 0,
                hold_until: None,
                crossed: false,
            },
        );
        self.requests
            .push(FromOverlay::Install(overlay_protocol::Install {
                mod_id: item.id,
                name: item.name.clone(),
                category_id,
            }));
        true
    }

    pub(super) fn install_state(&self, mod_id: u64) -> Option<&Install> {
        self.installs.get(&mod_id)
    }

    pub(super) fn holds(&self, mod_id: u64) -> bool {
        self.installs.get(&mod_id).is_some_and(Install::holds)
    }

    pub(super) fn cross_finished(&mut self, now: f64) -> (Vec<u64>, Option<f64>) {
        let mut crossed = Vec::new();
        let mut next: Option<f64> = None;
        for (&mod_id, install) in &mut self.installs {
            if !matches!(install.stage, InstallStage::Installed { .. }) || install.crossed {
                continue;
            }
            let until = *install.hold_until.get_or_insert(now + HOLD_SECS);
            if now >= until {
                install.crossed = true;
                crossed.push(mod_id);
            } else {
                next = Some(next.map_or(until, |next| next.min(until)));
            }
        }
        crossed.sort_unstable();
        (crossed, next)
    }

    pub(super) fn receive_install(&mut self, update: InstallUpdate) -> Option<InstallNews> {
        let InstallUpdate {
            mod_id,
            stage,
            name,
        } = update;
        let install = match self.installs.entry(mod_id) {
            Entry::Occupied(entry) if entry.get().running() || !runs(&stage) => entry.into_mut(),
            Entry::Vacant(_) if !runs(&stage) => return None,
            entry => entry
                .insert_entry(Install {
                    name: name.filter(|_| self.enabled)?,
                    stage: InstallStage::Waiting,
                    asked: 0,
                    hold_until: None,
                    crossed: false,
                })
                .into_mut(),
        };
        if install.stage == stage {
            return None;
        }
        install.stage = stage;
        let name = install.name.clone();
        match install.stage {
            InstallStage::Installed { .. } => Some(InstallNews::Installed(name)),
            InstallStage::Failed => Some(InstallNews::Failed(name)),
            InstallStage::ChooseFile { .. } | InstallStage::SameName { .. } => {
                self.questions_asked += 1;
                install.asked = self.questions_asked;
                Some(InstallNews::Question(name))
            }
            _ => None,
        }
    }

    pub(super) fn question(&self) -> Option<(u64, &Install)> {
        self.installs
            .iter()
            .filter(|(_, install)| install.asking())
            .min_by_key(|(_, install)| install.asked)
            .map(|(&mod_id, install)| (mod_id, install))
    }

    pub(super) fn pick_file(&mut self, mod_id: u64, file_id: u64) {
        if let Some(install) = self.installs.get_mut(&mod_id)
            && let InstallStage::ChooseFile { files } = &install.stage
            && files.iter().any(|file| file.id == file_id)
        {
            install.stage = InstallStage::Waiting;
            self.requests
                .push(FromOverlay::PickFile { mod_id, file_id });
        }
    }

    pub(super) fn choose_same_name(&mut self, mod_id: u64, choice: SameNameChoice) {
        if let Some(install) = self.installs.get_mut(&mod_id)
            && matches!(install.stage, InstallStage::SameName { .. })
        {
            install.stage = InstallStage::Installing;
            self.requests.push(FromOverlay::SameName { mod_id, choice });
        }
    }

    pub(super) fn cancel_install(&mut self, mod_id: u64) {
        if let Some(install) = self.installs.get_mut(&mod_id)
            && install.asking()
        {
            install.stage = InstallStage::Canceled;
            self.requests.push(FromOverlay::CancelInstall { mod_id });
        }
    }

    pub(super) fn take_requests(&mut self) -> Vec<FromOverlay> {
        std::mem::take(&mut self.requests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(query: &str) -> ListKey {
        ListKey {
            character: 7,
            query: query.to_owned(),
        }
    }

    fn page(query: &str, page: u32, ids: &[u64], more: bool) -> BrowsePage {
        BrowsePage {
            character: 7,
            query: query.to_owned(),
            page,
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
        }
    }

    #[test]
    fn a_list_settles_before_asking_then_asks_page_by_page() {
        let mut gamebanana = GameBanana::default();
        assert!(!gamebanana.want(&key(""), 0.0), "off outside the overlay");
        gamebanana.set_enabled(true);
        assert!(gamebanana.want(&key(""), 0.0));
        assert_eq!(
            gamebanana.list(&key("")).unwrap().status_card(),
            Some(StatusCard::Loading)
        );
        assert!(!gamebanana.want(&key(""), 0.1));
        assert!(gamebanana.take_requests().is_empty(), "still settling");
        assert!(!gamebanana.want(&key(""), 0.5));
        assert_eq!(
            gamebanana.take_requests(),
            [FromOverlay::Browse(Browse {
                character: 7,
                query: String::new(),
                page: 1,
            })]
        );
        assert!(!gamebanana.want(&key(""), 0.6));
        assert!(gamebanana.take_requests().is_empty());
        assert!(gamebanana.receive_page(page("", 1, &[1, 2], true)));
        gamebanana.want(&key(""), 0.7);
        assert!(matches!(
            gamebanana.take_requests()[..],
            [FromOverlay::Browse(Browse { page: 2, .. })]
        ));
        assert!(gamebanana.receive_page(page("", 2, &[2, 3], false)));
        let list = gamebanana.list(&key("")).unwrap();
        assert_eq!(
            list.mods.iter().map(|item| item.id).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(list.status_card(), None);
        gamebanana.want(&key(""), 0.8);
        assert!(gamebanana.take_requests().is_empty(), "no more pages");
    }

    #[test]
    fn a_failed_page_is_asked_again_and_stale_pages_are_ignored() {
        let mut gamebanana = GameBanana::default();
        gamebanana.set_enabled(true);
        gamebanana.want(&key("swim"), 0.0);
        gamebanana.want(&key("swim"), 1.0);
        gamebanana.take_requests();
        assert!(!gamebanana.receive_page(page("swim", 3, &[1], false)));
        assert!(!gamebanana.receive_page(page("other", 1, &[1], false)));
        let mut failed = page("swim", 1, &[], false);
        failed.error = Some("offline".into());
        assert!(gamebanana.receive_page(failed));
        assert_eq!(
            gamebanana.list(&key("swim")).unwrap().status_card(),
            Some(StatusCard::Failed)
        );
        gamebanana.retry(&key("swim"));
        assert!(matches!(
            gamebanana.take_requests()[..],
            [FromOverlay::Browse(Browse { page: 1, .. })]
        ));
        assert!(gamebanana.receive_page(page("swim", 1, &[], false)));
        assert_eq!(
            gamebanana.list(&key("swim")).unwrap().status_card(),
            Some(StatusCard::Empty)
        );
    }

    #[test]
    fn pictures_and_characters_arrive_later() {
        let mut gamebanana = GameBanana::default();
        gamebanana.set_enabled(true);
        gamebanana.want(&key(""), 0.0);
        gamebanana.want(&key(""), 1.0);
        gamebanana.receive_page(page("", 1, &[5], false));
        gamebanana.ask_characters();
        gamebanana.ask_characters();
        assert_eq!(
            gamebanana
                .take_requests()
                .into_iter()
                .filter(|request| *request == FromOverlay::ListCharacters)
                .count(),
            1
        );
        gamebanana.set_characters(
            vec![Character {
                id: 9,
                name: "Perlica".into(),
                image: None,
            }],
            None,
        );
        let pictures = Pictures {
            mods: [(5, PathBuf::from("five.bin"))].into(),
            characters: [(9, PathBuf::from("nine.bin"))].into(),
        };
        assert!(gamebanana.apply_pictures(&pictures));
        assert_eq!(
            gamebanana.list(&key("")).unwrap().mods[0].image,
            Some(PathBuf::from("five.bin"))
        );
        assert_eq!(
            gamebanana.characters()[0].image,
            Some(PathBuf::from("nine.bin"))
        );
    }

    fn update(mod_id: u64, stage: InstallStage) -> InstallUpdate {
        InstallUpdate {
            mod_id,
            stage,
            name: None,
        }
    }

    #[test]
    fn installs_ask_questions_oldest_first_and_can_run_again_after_failing() {
        let mut gamebanana = GameBanana::default();
        let item = |id: u64| BrowseMod {
            id,
            name: format!("Mod {id}"),
            ..Default::default()
        };
        assert!(
            !gamebanana.install(&item(1), None),
            "off outside the overlay"
        );
        gamebanana.set_enabled(true);
        assert!(gamebanana.install(&item(1), Some("category".into())));
        assert!(!gamebanana.install(&item(1), None), "already installing");
        assert!(gamebanana.install(&item(2), None));
        assert_eq!(
            gamebanana.take_requests()[0],
            FromOverlay::Install(overlay_protocol::Install {
                mod_id: 1,
                name: "Mod 1".into(),
                category_id: Some("category".into()),
            })
        );

        let files = vec![overlay_protocol::InstallFile {
            id: 30,
            name: "main.zip".into(),
            size: 10,
            description: None,
        }];
        assert_eq!(
            gamebanana.receive_install(update(2, InstallStage::ChooseFile { files })),
            Some(InstallNews::Question("Mod 2".into()))
        );
        assert_eq!(
            gamebanana.receive_install(update(
                1,
                InstallStage::SameName {
                    folder: "Mod 1".into()
                }
            )),
            Some(InstallNews::Question("Mod 1".into()))
        );
        assert_eq!(gamebanana.question().map(|(id, _)| id), Some(2));
        gamebanana.pick_file(2, 99);
        assert!(
            gamebanana.take_requests().is_empty(),
            "not one of its files"
        );
        gamebanana.pick_file(2, 30);
        assert_eq!(
            gamebanana.take_requests(),
            [FromOverlay::PickFile {
                mod_id: 2,
                file_id: 30
            }]
        );
        assert_eq!(gamebanana.question().map(|(id, _)| id), Some(1));
        gamebanana.cancel_install(1);
        assert_eq!(
            gamebanana.take_requests(),
            [FromOverlay::CancelInstall { mod_id: 1 }]
        );
        assert!(gamebanana.question().is_none());

        assert_eq!(
            gamebanana.receive_install(update(2, InstallStage::Failed)),
            Some(InstallNews::Failed("Mod 2".into()))
        );
        assert_eq!(
            gamebanana.receive_install(update(2, InstallStage::Failed)),
            None
        );
        assert!(gamebanana.install(&item(2), None), "tries again");
        assert_eq!(
            gamebanana.receive_install(update(7, InstallStage::Installing)),
            None,
            "not the overlay's"
        );
    }

    #[test]
    fn a_download_hestias_window_started_shows_like_an_install() {
        let mut gamebanana = GameBanana::default();
        let window = |stage: InstallStage| InstallUpdate {
            mod_id: 3,
            stage,
            name: Some("Mod 3".into()),
        };
        assert_eq!(
            gamebanana.receive_install(window(InstallStage::Waiting)),
            None
        );
        assert!(gamebanana.install_state(3).is_none(), "GameBanana is off");
        gamebanana.set_enabled(true);
        assert_eq!(
            gamebanana.receive_install(window(InstallStage::Canceled)),
            None
        );
        assert!(
            gamebanana.install_state(3).is_none(),
            "over before the overlay heard of it"
        );
        assert_eq!(
            gamebanana.receive_install(window(InstallStage::Downloading { percent: Some(10) })),
            None
        );
        assert!(gamebanana.holds(3));
        let item = BrowseMod {
            id: 3,
            name: "Mod 3".into(),
            ..Default::default()
        };
        assert!(
            !gamebanana.install(&item, None),
            "Hestia installs it already"
        );
        assert!(gamebanana.take_requests().is_empty());
        assert_eq!(
            gamebanana.receive_install(window(InstallStage::Installed {
                mods: vec!["three".into()]
            })),
            Some(InstallNews::Installed("Mod 3".into()))
        );
        assert_eq!(gamebanana.cross_finished(0.0).0, Vec::<u64>::new());
        assert_eq!(gamebanana.cross_finished(1.0).0, [3]);
        assert!(!gamebanana.holds(3));

        assert_eq!(
            gamebanana.receive_install(window(InstallStage::Waiting)),
            None
        );
        let install = gamebanana.install_state(3).unwrap();
        assert_eq!(install.stage, InstallStage::Waiting);
        assert!(!install.crossed);
    }
}
