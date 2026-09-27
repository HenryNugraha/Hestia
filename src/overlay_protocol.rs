//! Messages between Hestia and the in-game overlay (`hestia --overlay`).
//!
//! Hestia starts the overlay while a supported game runs and writes one JSON
//! object per line to its stdin.  The overlay answers the same way on stdout,
//! so its logs go to stderr.  Hestia closing the pipe tells the overlay to
//! exit.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{AppLanguage, OverlayHotkey, OverlaySize};

/// The command line flag that starts the overlay.
pub(crate) const OVERLAY_ARG: &str = "--overlay";

/// The category id Hestia gives its catch-all group of uncategorized mods.
pub(crate) const UNCATEGORIZED_ID: &str = "hestia:uncategorized";

/// The overlay's window title.  XXMI takes keys only from the game or a
/// window titled "Hestia", so Hestia gives the overlay that title just while
/// it presses the reload key, then this one back.
pub(crate) const OVERLAY_TITLE: &str = "Hestia overlay";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ToOverlay {
    /// Always the first line.
    Start(Start),
    /// The library changed.
    Library(Library),
    /// The game's processes changed, for example once the game has finished
    /// starting.
    GameProcesses { pids: Vec<u32> },
    /// Hestia has made, or refused, a change the overlay asked for.  The
    /// library with the change comes first.
    Changed(Changed),
    /// One page of GameBanana mods the overlay asked for with
    /// `FromOverlay::Browse`.
    BrowsePage(BrowsePage),
    /// The game's GameBanana characters, which "Show all characters" adds to
    /// the characters row.
    Characters {
        characters: Vec<Character>,
        /// Why Hestia couldn't get them, so the overlay can ask again.
        #[serde(default)]
        error: Option<String>,
    },
    /// GameBanana pictures Hestia has downloaded since it sent their mods or
    /// characters, by GameBanana id.
    Pictures(Pictures),
    /// How an install the overlay asked for with `FromOverlay::Install` goes.
    Install(InstallUpdate),
    /// Hestia's settings changed.
    Settings(Settings),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Start {
    pub host_pid: u32,
    /// Hestia's main window, for the overlay's "open Hestia" button.
    pub host_window: Option<i64>,
    /// The game's processes.  The overlay shows its strip once one of them
    /// has the foreground.
    pub game_pids: Vec<u32>,
    pub settings: Settings,
    pub library: Library,
    /// Where the overlay was left the last time it ran in this Hestia
    /// session, if it did.
    pub selection: Option<Selection>,
}

/// The settings the overlay takes from Hestia.  Saved samples from before a
/// setting existed read it as its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub language: AppLanguage,
    /// The opacity the overlay was left at, once it was changed.
    pub opacity: Option<u8>,
    pub hotkey: OverlayHotkey,
    /// Show the strip for a moment when the game starts.
    pub arrival_strip: bool,
    /// Keep the strip for a moment after the overlay closes.
    pub close_strip: bool,
    pub gamebanana: bool,
    /// Also list GameBanana's characters that no category stands for.
    pub all_characters: bool,
    pub key_hints: bool,
    pub size: OverlaySize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: AppLanguage::default(),
            opacity: None,
            hotkey: OverlayHotkey::default(),
            arrival_strip: true,
            close_strip: true,
            gamebanana: true,
            all_characters: false,
            key_hints: true,
            size: OverlaySize::default(),
        }
    }
}

/// The game's library, in the main window's order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Library {
    pub game_id: String,
    pub game_name: String,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Category {
    pub id: String,
    pub name: String,
    /// The linked character's picture.  Without one, the overlay shows the
    /// active mod's picture.
    pub image: Option<PathBuf>,
    /// The GameBanana character the category stands for, whose GameBanana
    /// mods the overlay shows after the category's own.
    #[serde(default)]
    pub character: Option<u64>,
    pub mods: Vec<Mod>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Mod {
    pub id: String,
    pub name: String,
    pub image: Option<PathBuf>,
    pub active: bool,
    /// The library censors the mod's picture, so the overlay darkens it like
    /// the library's cards.
    #[serde(default)]
    pub censored: bool,
    /// The GameBanana mod it was installed from, so the overlay doesn't offer
    /// that one again.
    #[serde(default)]
    pub gamebanana_id: Option<u64>,
}

/// The overlay's place in the library, by ids so it survives changes to the
/// list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Selection {
    pub category_id: Option<String>,
    /// The focused mod of each category where it isn't the active one, by
    /// category id.
    #[serde(default)]
    pub mods: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum FromOverlay {
    Selection(Selection),
    /// Turn a mod on or off.  Hestia answers with `ToOverlay::Changed`.
    Change(Change),
    /// Whether the search takes the keys, so Hestia doesn't reload with the
    /// overlay titled "Hestia" meanwhile.
    Typing {
        typing: bool,
    },
    /// A page of a character's GameBanana mods.  Hestia answers with
    /// `ToOverlay::BrowsePage`.
    Browse(Browse),
    /// The game's GameBanana characters.  Hestia answers with
    /// `ToOverlay::Characters`.
    ListCharacters,
    /// Install a GameBanana mod, turned off.  Hestia answers with
    /// `ToOverlay::Install` as it goes.
    Install(Install),
    /// The file picked for an install that asked which one.
    PickFile {
        mod_id: u64,
        file_id: u64,
    },
    /// What to do about the folder that has the install's name.
    SameName {
        mod_id: u64,
        choice: SameNameChoice,
    },
    /// Stop an install that waits for an answer.
    CancelInstall {
        mod_id: u64,
    },
    /// The opacity the overlay was left at, for Hestia to save.
    Opacity {
        opacity: u8,
    },
    /// The header's "show all characters" button changed, for Hestia to save.
    AllCharacters {
        show: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Browse {
    pub character: u64,
    /// Search words, or empty for all the character's mods.
    pub query: String,
    /// From 1.
    pub page: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct BrowsePage {
    pub character: u64,
    pub query: String,
    pub page: u32,
    /// In GameBanana's order, which follows the sort of Hestia's Browse page.
    pub mods: Vec<BrowseMod>,
    /// Whether GameBanana has another page.
    pub more: bool,
    /// Why Hestia couldn't get the page.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct BrowseMod {
    pub id: u64,
    pub name: String,
    /// None until Hestia has downloaded it, which `ToOverlay::Pictures` says.
    pub image: Option<PathBuf>,
    /// GameBanana marks it for mature content, and the library censors that.
    #[serde(default)]
    pub censored: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Character {
    pub id: u64,
    pub name: String,
    pub image: Option<PathBuf>,
}

/// Pairs of GameBanana id and picture.  Not maps: a tagged message can't
/// read number keys back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pictures {
    #[serde(default)]
    pub mods: Vec<(u64, PathBuf)>,
    #[serde(default)]
    pub characters: Vec<(u64, PathBuf)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Install {
    /// The GameBanana mod.
    pub mod_id: u64,
    pub name: String,
    /// The library category the overlay showed it in, which gets it.  None
    /// for a character without one, which gets a new category.
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InstallUpdate {
    /// The GameBanana mod.
    pub mod_id: u64,
    pub stage: InstallStage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub(crate) enum InstallStage {
    /// Hestia looks up the mod's files, or waits for its turn.
    Waiting,
    /// In percent, when GameBanana gave the size.
    Downloading {
        percent: Option<u8>,
    },
    Installing,
    /// GameBanana has several files for the mod.  The overlay answers with
    /// `FromOverlay::PickFile` or `FromOverlay::CancelInstall`.
    ChooseFile {
        files: Vec<InstallFile>,
    },
    /// A folder already has the install's name.  The overlay answers with
    /// `FromOverlay::SameName` or `FromOverlay::CancelInstall`.
    SameName {
        folder: String,
    },
    /// Installed and turned off, as these library mods.
    Installed {
        mods: Vec<String>,
    },
    Failed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InstallFile {
    pub id: u64,
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SameNameChoice {
    Replace,
    Merge,
    KeepBoth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Change {
    /// The overlay's number for it, which the answer repeats.
    pub id: u64,
    pub mod_id: String,
    pub action: ChangeAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChangeAction {
    /// Turn it on and the other mods of its category off.  In Uncategorized,
    /// whose mods have nothing to do with each other, only turn it on.
    Use,
    TurnOn,
    TurnOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Changed {
    pub id: u64,
    /// Why Hestia didn't make the change, for the overlay's warning strip.
    pub error: Option<String>,
    /// The game's reload key, such as "F10", when the reload settings leave
    /// pressing it to the user.
    #[serde(default)]
    pub press_key: Option<String>,
}

/// One message as a line, without the line break.
pub(crate) fn encode<T: Serialize>(message: &T) -> serde_json::Result<String> {
    serde_json::to_string(message)
}

pub(crate) fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> serde_json::Result<T> {
    serde_json::from_str(line.trim_end())
}

/// Whether a category without a GameBanana link stands for `character` by
/// its name.  "Ardelia" and "Operators: Ardelia", the way Hestia's Browse page
/// names the categories it makes, both stand for "Ardelia".
pub(crate) fn names_character(category: &str, character: &str) -> bool {
    fn leaf(name: &str) -> String {
        let name = name.trim();
        name.rsplit_once(": ")
            .map_or(name, |(_, leaf)| leaf)
            .trim()
            .to_lowercase()
    }
    let character = leaf(character);
    !character.is_empty() && leaf(category) == character
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        Library {
            game_id: "endfield".into(),
            game_name: "Arknights Endfield".into(),
            categories: vec![Category {
                id: "ardelia".into(),
                name: "Ardelia".into(),
                image: Some(PathBuf::from(r"C:\cache\icon.bin")),
                character: Some(11_300),
                mods: vec![Mod {
                    id: "mod-1".into(),
                    name: "Vow \"Swimsuit\"\nline".into(),
                    image: None,
                    active: true,
                    censored: false,
                    gamebanana_id: Some(5_000),
                }],
            }],
        }
    }

    #[test]
    fn settings_missing_from_old_samples_read_as_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"opacity":70}"#).unwrap();
        assert_eq!(
            settings,
            Settings {
                opacity: Some(70),
                ..Settings::default()
            }
        );
    }

    #[test]
    fn messages_round_trip_on_one_line() {
        let messages = [
            ToOverlay::Start(Start {
                host_pid: 42,
                host_window: Some(0x1234),
                game_pids: vec![7, 8],
                settings: Settings {
                    language: AppLanguage::Russian,
                    opacity: Some(70),
                    hotkey: OverlayHotkey::parse("Ctrl+Shift+F7").unwrap(),
                    arrival_strip: false,
                    close_strip: true,
                    gamebanana: false,
                    all_characters: true,
                    key_hints: false,
                    size: OverlaySize::Large,
                },
                library: library(),
                selection: Some(Selection {
                    category_id: Some("ardelia".into()),
                    mods: BTreeMap::from([("ardelia".into(), "mod-1".into())]),
                }),
            }),
            ToOverlay::Library(library()),
            ToOverlay::GameProcesses { pids: vec![] },
            ToOverlay::Changed(Changed {
                id: 3,
                error: None,
                press_key: Some("F10".into()),
            }),
            ToOverlay::Changed(Changed {
                id: 4,
                error: Some("Mods are locked".into()),
                press_key: None,
            }),
            ToOverlay::BrowsePage(BrowsePage {
                character: 11_300,
                query: "swim".into(),
                page: 2,
                mods: vec![BrowseMod {
                    id: 6_000,
                    name: "Summer".into(),
                    image: None,
                    censored: true,
                }],
                more: true,
                error: None,
            }),
            ToOverlay::Characters {
                characters: vec![Character {
                    id: 11_301,
                    name: "Perlica".into(),
                    image: Some(PathBuf::from(r"C:\cache\perlica.bin")),
                }],
                error: None,
            },
            ToOverlay::Pictures(Pictures {
                mods: vec![(6_000, PathBuf::from(r"C:\cache\summer.bin"))],
                characters: Vec::new(),
            }),
            ToOverlay::Install(InstallUpdate {
                mod_id: 6_000,
                stage: InstallStage::ChooseFile {
                    files: vec![InstallFile {
                        id: 1,
                        name: "summer.zip".into(),
                        size: 2_048,
                        description: None,
                    }],
                },
            }),
            ToOverlay::Install(InstallUpdate {
                mod_id: 6_000,
                stage: InstallStage::Downloading { percent: Some(40) },
            }),
            ToOverlay::Install(InstallUpdate {
                mod_id: 6_000,
                stage: InstallStage::Waiting,
            }),
            ToOverlay::Settings(Settings::default()),
        ];
        for message in messages {
            let line = encode(&message).unwrap();
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(decode::<ToOverlay>(&(line + "\r\n")).unwrap(), message);
        }
        let selection = FromOverlay::Selection(Selection::default());
        let line = encode(&selection).unwrap();
        assert_eq!(line, r#"{"type":"selection","category_id":null,"mods":{}}"#);
        assert_eq!(decode::<FromOverlay>(&line).unwrap(), selection);
        let change = FromOverlay::Change(Change {
            id: 3,
            mod_id: "mod-1".into(),
            action: ChangeAction::TurnOff,
        });
        let line = encode(&change).unwrap();
        assert_eq!(
            line,
            r#"{"type":"change","id":3,"mod_id":"mod-1","action":"turn_off"}"#
        );
        assert_eq!(decode::<FromOverlay>(&line).unwrap(), change);
        let typing = FromOverlay::Typing { typing: true };
        let line = encode(&typing).unwrap();
        assert_eq!(line, r#"{"type":"typing","typing":true}"#);
        assert_eq!(decode::<FromOverlay>(&line).unwrap(), typing);
        let browse = FromOverlay::Browse(Browse {
            character: 11_300,
            query: String::new(),
            page: 1,
        });
        let line = encode(&browse).unwrap();
        assert_eq!(
            line,
            r#"{"type":"browse","character":11300,"query":"","page":1}"#
        );
        assert_eq!(decode::<FromOverlay>(&line).unwrap(), browse);
        let line = encode(&FromOverlay::ListCharacters).unwrap();
        assert_eq!(line, r#"{"type":"list_characters"}"#);
        assert_eq!(
            decode::<FromOverlay>(&line).unwrap(),
            FromOverlay::ListCharacters
        );
        for message in [
            FromOverlay::Install(Install {
                mod_id: 6_000,
                name: "Summer".into(),
                category_id: None,
            }),
            FromOverlay::PickFile {
                mod_id: 6_000,
                file_id: 1,
            },
            FromOverlay::SameName {
                mod_id: 6_000,
                choice: SameNameChoice::KeepBoth,
            },
            FromOverlay::CancelInstall { mod_id: 6_000 },
            FromOverlay::Opacity { opacity: 64 },
            FromOverlay::AllCharacters { show: true },
        ] {
            let line = encode(&message).unwrap();
            assert_eq!(decode::<FromOverlay>(&line).unwrap(), message);
        }
    }

    #[test]
    fn categories_stand_for_characters_by_name() {
        assert!(names_character("Ardelia", "Ardelia"));
        assert!(names_character("Operators: Ardelia", "ardelia"));
        assert!(names_character(" ardelia ", "Operators: Ardelia"));
        assert!(!names_character("Ardelia (swim)", "Ardelia"));
        assert!(!names_character("Other/Misc", "Perlica"));
        assert!(!names_character("", " "));
    }

    #[test]
    fn unknown_lines_are_errors_not_panics() {
        assert!(decode::<ToOverlay>("").is_err());
        assert!(decode::<ToOverlay>("not json").is_err());
        assert!(decode::<ToOverlay>(r#"{"type":"later_message"}"#).is_err());
    }
}
