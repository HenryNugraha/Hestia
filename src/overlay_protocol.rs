//! Messages between Hestia and the in-game overlay (`hestia --overlay`).
//!
//! Hestia starts the overlay while a supported game runs and writes one JSON
//! object per line to its stdin.  The overlay answers the same way on stdout,
//! so its logs go to stderr.  Hestia closing the pipe tells the overlay to
//! exit.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{AppLanguage, OverlayCharacterLayout, OverlayHotkey};

pub(crate) const OVERLAY_ARG: &str = "--overlay";

pub(crate) const UNCATEGORIZED_ID: &str = "hestia:uncategorized";

/// The overlay's window title.  XXMI takes keys only from the game or a
/// window titled `XXMI_TITLE`, so the overlay has that title just while Hestia
/// presses a key for the game through it, or while a key the overlay doesn't
/// use is down, then this one back.
pub(crate) const OVERLAY_TITLE: &str = "Hestia overlay";

/// The main window's title.  It borrows `XXMI_TITLE` just while Hestia presses
/// a key for the game, so the keys typed in it never reach the game.
pub(crate) const MAIN_WINDOW_TITLE: &str = "Hestia Mod Manager";

/// The title XXMI takes keys from, besides the game: Hestia writes it as
/// `additional_foreground_window` in d3dx.ini.  XXMI needs the whole title to
/// match, so `MAIN_WINDOW_TITLE` and `OVERLAY_TITLE` don't count.
pub(crate) const XXMI_TITLE: &str = "Hestia";

pub(crate) const HESTIA_INPUT_MARK: usize = 0x4845_5354;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ToOverlay {
    Start(Start),
    Library(Library),
    GameProcesses {
        pids: Vec<u32>,
    },
    Changed(Changed),
    BrowsePage(BrowsePage),
    Characters {
        characters: Vec<Character>,
        #[serde(default)]
        error: Option<String>,
    },
    Pictures(Pictures),
    Install(InstallUpdate),
    Hotkeys(ModHotkeys),
    Info(ModInfo),
    HotkeyPressed {
        id: u64,
        error: Option<String>,
    },
    Settings(Settings),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Start {
    pub host_pid: u32,
    pub host_window: Option<i64>,
    pub game_pids: Vec<u32>,
    pub settings: Settings,
    pub library: Library,
    pub selection: Option<Selection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub language: AppLanguage,
    pub opacity: Option<u8>,
    pub hotkey: OverlayHotkey,
    pub arrival_strip: bool,
    pub gamebanana: bool,
    pub all_characters: bool,
    pub key_hints: bool,
    pub opacity_slider: bool,
    pub pin_button: bool,
    pub character_layout: OverlayCharacterLayout,
    pub zoom: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: AppLanguage::default(),
            opacity: None,
            hotkey: OverlayHotkey::default(),
            arrival_strip: true,
            gamebanana: true,
            all_characters: false,
            key_hints: true,
            opacity_slider: true,
            pin_button: true,
            character_layout: OverlayCharacterLayout::default(),
            zoom: 1.0,
        }
    }
}

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
    pub image: Option<PathBuf>,
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
    #[serde(default)]
    pub censored: bool,
    #[serde(default)]
    pub gamebanana_id: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Selection {
    pub category_id: Option<String>,
    #[serde(default)]
    pub mods: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum FromOverlay {
    Selection(Selection),
    Change(Change),
    Typing {
        typing: bool,
    },
    Browse(Browse),
    ListCharacters,
    Install(Install),
    PickFile {
        mod_id: u64,
        file_id: u64,
    },
    SameName {
        mod_id: u64,
        choice: SameNameChoice,
    },
    CancelInstall {
        mod_id: u64,
    },
    Opacity {
        opacity: u8,
    },
    AllCharacters {
        show: bool,
    },
    Hotkeys {
        mod_id: String,
    },
    PressHotkey {
        id: u64,
        mod_id: String,
        index: usize,
        key: String,
    },
    Info {
        mod_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Browse {
    pub character: u64,
    pub query: String,
    pub page: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct BrowsePage {
    pub character: u64,
    pub query: String,
    pub page: u32,
    pub mods: Vec<BrowseMod>,
    pub more: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct BrowseMod {
    pub id: u64,
    pub name: String,
    pub image: Option<PathBuf>,
    #[serde(default)]
    pub censored: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Character {
    pub id: u64,
    pub name: String,
    pub image: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pictures {
    #[serde(default)]
    pub mods: Vec<(u64, PathBuf)>,
    #[serde(default)]
    pub characters: Vec<(u64, PathBuf)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModHotkeys {
    pub mod_id: String,
    pub hotkeys: Vec<ModHotkey>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModHotkey {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub unavailable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModInfo {
    pub mod_id: String,
    pub sources: Vec<InfoSource>,
    #[serde(default)]
    pub shown: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InfoSource {
    pub kind: InfoKind,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InfoKind {
    #[default]
    Description,
    Note,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Install {
    pub mod_id: u64,
    pub name: String,
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InstallUpdate {
    pub mod_id: u64,
    pub stage: InstallStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub(crate) enum InstallStage {
    Waiting,
    Downloading { percent: Option<u8> },
    Installing,
    ChooseFile { files: Vec<InstallFile> },
    SameName { folder: String },
    Installed { mods: Vec<String> },
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
    pub id: u64,
    pub mod_id: String,
    pub action: ChangeAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChangeAction {
    Use,
    TurnOn,
    TurnOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Changed {
    pub id: u64,
    pub error: Option<String>,
    #[serde(default)]
    pub press_key: Option<String>,
}

pub(crate) fn encode<T: Serialize>(message: &T) -> serde_json::Result<String> {
    serde_json::to_string(message)
}

pub(crate) fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> serde_json::Result<T> {
    serde_json::from_str(line.trim_end())
}

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
                    gamebanana: false,
                    all_characters: true,
                    key_hints: false,
                    opacity_slider: false,
                    pin_button: true,
                    character_layout: OverlayCharacterLayout::Grid,
                    zoom: 1.2,
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
                name: None,
            }),
            ToOverlay::Install(InstallUpdate {
                mod_id: 6_000,
                stage: InstallStage::Downloading { percent: Some(40) },
                name: None,
            }),
            ToOverlay::Install(InstallUpdate {
                mod_id: 6_000,
                stage: InstallStage::Waiting,
                name: Some("Summer".into()),
            }),
            ToOverlay::Hotkeys(ModHotkeys {
                mod_id: "mod-1".into(),
                hotkeys: vec![ModHotkey {
                    key: "Ctrl+H".into(),
                    label: "Hat".into(),
                    raw: "ctrl VK_H".into(),
                    unavailable: true,
                }],
            }),
            ToOverlay::Info(ModInfo {
                mod_id: "mod-1".into(),
                sources: vec![
                    InfoSource {
                        kind: InfoKind::Description,
                        path: String::new(),
                        label: String::new(),
                        text: "A swimsuit.\n\nPress H for the hat.".into(),
                    },
                    InfoSource {
                        kind: InfoKind::File,
                        path: "readme.txt".into(),
                        label: "readme.txt".into(),
                        text: "Thanks".into(),
                    },
                ],
                shown: 1,
            }),
            ToOverlay::HotkeyPressed { id: 4, error: None },
            ToOverlay::HotkeyPressed {
                id: 5,
                error: Some("Turn this mod on to use its hotkeys.".into()),
            },
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
            FromOverlay::Hotkeys {
                mod_id: "mod-1".into(),
            },
            FromOverlay::PressHotkey {
                id: 4,
                mod_id: "mod-1".into(),
                index: 0,
                key: "ctrl VK_H".into(),
            },
            FromOverlay::Info {
                mod_id: "mod-1".into(),
            },
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
