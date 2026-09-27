//! Messages between Hestia and the in-game overlay (`hestia --overlay`).
//!
//! Hestia starts the overlay while a supported game runs and writes one JSON
//! object per line to its stdin.  The overlay answers the same way on stdout,
//! so its logs go to stderr.  Hestia closing the pipe tells the overlay to
//! exit.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

/// The command line flag that starts the overlay.
pub(crate) const OVERLAY_ARG: &str = "--overlay";

/// The category id Hestia gives its catch-all group of uncategorized mods.
pub(crate) const UNCATEGORIZED_ID: &str = "hestia:uncategorized";

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Start {
    pub host_pid: u32,
    /// Hestia's main window, for the overlay's "open Hestia" button.
    pub host_window: Option<i64>,
    /// The game's processes.  The overlay shows its strip once one of them
    /// has the foreground.
    pub game_pids: Vec<u32>,
    pub library: Library,
    /// Where the overlay was left the last time it ran in this Hestia
    /// session, if it did.
    pub selection: Option<Selection>,
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
}

/// One message as a line, without the line break.
pub(crate) fn encode<T: Serialize>(message: &T) -> serde_json::Result<String> {
    serde_json::to_string(message)
}

pub(crate) fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> serde_json::Result<T> {
    serde_json::from_str(line.trim_end())
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
                mods: vec![Mod {
                    id: "mod-1".into(),
                    name: "Vow \"Swimsuit\"\nline".into(),
                    image: None,
                    active: true,
                    censored: false,
                }],
            }],
        }
    }

    #[test]
    fn messages_round_trip_on_one_line() {
        let messages = [
            ToOverlay::Start(Start {
                host_pid: 42,
                host_window: Some(0x1234),
                game_pids: vec![7, 8],
                library: library(),
                selection: Some(Selection {
                    category_id: Some("ardelia".into()),
                    mods: BTreeMap::from([("ardelia".into(), "mod-1".into())]),
                }),
            }),
            ToOverlay::Library(library()),
            ToOverlay::GameProcesses { pids: vec![] },
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
    }

    #[test]
    fn unknown_lines_are_errors_not_panics() {
        assert!(decode::<ToOverlay>("").is_err());
        assert!(decode::<ToOverlay>("not json").is_err());
        assert!(decode::<ToOverlay>(r#"{"type":"later_message"}"#).is_err());
    }
}
