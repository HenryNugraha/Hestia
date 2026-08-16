// Read-only extraction of a mod's user-facing 3DMigoto config (keybinds/toggles).
//
// "Related lines" means the keybind sections of a mod's `.ini` files: the
// `[Key...]`-style sections 3DMigoto recognizes by the presence of a `key =`
// assignment, together with the lines inside them a user actually tweaks (the
// bound key, the cycled variable and its values, the activation condition).
// Everything else in the ini (`TextureOverride`/`Resource`/hash boilerplate) is
// deliberately dropped so the viewer shows only the lines that matter.

/// One keybind/toggle section worth surfacing to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModConfigSection {
    /// Section header exactly as written, e.g. `[KeySwapBody]`.
    header: String,
    /// The meaningful (non-comment, non-blank) lines inside the section.
    lines: Vec<String>,
}

/// The config-relevant sections found in a single `.ini` file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModConfigIni {
    /// Path relative to the mod root, using `/` separators.
    rel_path: String,
    sections: Vec<ModConfigSection>,
}

/// True when a section body line binds a key (`key = ...`). 3DMigoto identifies an
/// interactive key binding by this assignment, not by the section's name, so this is
/// what decides whether a section is "config" the user can change.
fn ini_line_is_key_binding(line: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("key"))
}

/// Extract keybind sections from one ini's text. A section is kept only when it
/// contains a `key =` line; within a kept section, blank lines and comments are
/// dropped and every other line is preserved verbatim (trimmed).
fn parse_ini_keybind_sections(text: &str) -> Vec<ModConfigSection> {
    let mut sections = Vec::new();
    let mut header: Option<String> = None;
    let mut lines: Vec<String> = Vec::new();
    let mut has_key = false;

    fn flush(
        sections: &mut Vec<ModConfigSection>,
        header: &mut Option<String>,
        lines: &mut Vec<String>,
        has_key: &mut bool,
    ) {
        if let Some(h) = header.take() {
            if *has_key && !lines.is_empty() {
                sections.push(ModConfigSection {
                    header: h,
                    lines: std::mem::take(lines),
                });
            }
        }
        lines.clear();
        *has_key = false;
    }

    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.len() >= 2 && trimmed.starts_with('[') && trimmed.ends_with(']') {
            flush(&mut sections, &mut header, &mut lines, &mut has_key);
            header = Some(trimmed.to_string());
            continue;
        }
        if header.is_none() || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if ini_line_is_key_binding(trimmed) {
            has_key = true;
        }
        lines.push(trimmed.to_string());
    }
    flush(&mut sections, &mut header, &mut lines, &mut has_key);
    sections
}

/// Walk a mod root and collect the keybind sections of every `.ini` it contains,
/// skipping Hestia's own metadata directory. Files with no keybind sections are
/// omitted entirely. Purely read-only; nothing is written back.
fn parse_mod_config_inis(root: &Path) -> Vec<ModConfigIni> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root).max_depth(8) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .components()
            .any(|part| part.as_os_str() == MOD_META_DIR)
        {
            continue;
        }
        let is_ini = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ini"));
        if !is_ini {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let sections = parse_ini_keybind_sections(&text);
        if sections.is_empty() {
            continue;
        }
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(ModConfigIni { rel_path, sections });
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

#[cfg(test)]
mod mod_config_tests {
    use super::*;

    #[test]
    fn extracts_keybind_sections_and_drops_boilerplate() {
        let ini = "\
; a comment
[Constants]
global persist $swapvar = 0

[KeySwap]
condition = $active == 1
key = VK_DOWN
back = VK_UP
type = cycle
$swapvar = 0,1,2

[TextureOverrideBody]
hash = abcdef01
this = ps-t0
";
        let sections = parse_ini_keybind_sections(ini);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].header, "[KeySwap]");
        assert!(sections[0].lines.iter().any(|l| l == "key = VK_DOWN"));
        assert!(sections[0].lines.iter().any(|l| l == "$swapvar = 0,1,2"));
        assert!(!sections[0].lines.iter().any(|l| l.starts_with("hash")));
    }

    #[test]
    fn section_without_key_line_is_dropped() {
        let ini = "[Constants]\nglobal persist $x = 0\n[KeyThing]\ncondition = 1\n";
        assert!(parse_ini_keybind_sections(ini).is_empty());
    }

    #[test]
    fn multiple_keybind_sections_are_all_kept_in_order() {
        let ini = "[KeyA]\nkey = a\n[KeyB]\nkey = b\n";
        let sections = parse_ini_keybind_sections(ini);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].header, "[KeyA]");
        assert_eq!(sections[1].header, "[KeyB]");
    }

    #[test]
    fn key_detection_is_case_insensitive_and_ignores_lookalikes() {
        assert!(ini_line_is_key_binding("  Key  =  h "));
        assert!(ini_line_is_key_binding("key=VK_F1"));
        assert!(!ini_line_is_key_binding("keyword = 3"));
        assert!(!ini_line_is_key_binding("hash = 3"));
    }
}
