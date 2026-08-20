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

/// One `[Constants]` variable declaration, e.g. `global persist $swapvar = 0`.
///
/// Only the flags that decide whether a variable can be read back live matter here:
/// `global` (addressable cross-namespace, so the live-state helper can mirror it) and
/// `persist` (3DMigoto already saves it to `d3dx_user.ini`, so no mirror is needed).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModConfigConstant {
    /// Variable name without the leading `$`, as written.
    name: String,
    /// Declared `global` — required for the live-state helper to read it via `$\ns\var`.
    global: bool,
    /// Declared `persist` — 3DMigoto saves it to `d3dx_user.ini` without a mirror.
    persist: bool,
}

/// The config-relevant sections found in a single `.ini` file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModConfigIni {
    /// Path relative to the mod root, using `/` separators.
    rel_path: String,
    /// Explicit `namespace = …` from the ini preamble (before the first section),
    /// verbatim-trimmed. `None` means the default namespace (the ini's relative path).
    namespace: Option<String>,
    /// `[Constants]` variable declarations found in this ini, used to resolve a shown
    /// hotkey variable's `global`/`persist` flags for the live-state helper.
    constants: Vec<ModConfigConstant>,
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

/// Parse an ini's preamble `namespace = …` (if any) and its `[Constants]` variable
/// declarations. 3DMigoto lowercases keys at parse time, so keyword matching here is
/// case-insensitive; the variable name is kept as written (the live-state helper
/// lowercases it when it builds the mirror name).
fn parse_ini_namespace_and_constants(text: &str) -> (Option<String>, Vec<ModConfigConstant>) {
    let mut namespace: Option<String> = None;
    let mut constants: Vec<ModConfigConstant> = Vec::new();
    let mut before_first_section = true;
    let mut in_constants = false;

    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            before_first_section = false;
            let inner = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            in_constants = inner.eq_ignore_ascii_case("Constants");
            continue;
        }
        if before_first_section {
            if let Some((key, value)) = trimmed.split_once('=')
                && key.trim().eq_ignore_ascii_case("namespace")
            {
                let value = value.trim();
                if !value.is_empty() {
                    namespace = Some(value.to_string());
                }
            }
            continue;
        }
        if !in_constants {
            continue;
        }
        if let Some(decl) = parse_constant_declaration(trimmed) {
            constants.push(decl);
        }
    }

    (namespace, constants)
}

/// Parse one `[Constants]` line into a declaration. The recognized shape is
/// `[global] [persist] $name [= value]`; the keyword order is not fixed. Lines without a
/// `$name` token (blank assignments, `run =` plumbing, etc.) yield `None`.
fn parse_constant_declaration(line: &str) -> Option<ModConfigConstant> {
    let head = line.split_once('=').map_or(line, |(head, _)| head);
    let mut global = false;
    let mut persist = false;
    let mut name: Option<String> = None;
    for token in head.split_whitespace() {
        if let Some(var) = token.strip_prefix('$') {
            if !var.is_empty() {
                name = Some(var.to_string());
            }
        } else if token.eq_ignore_ascii_case("global") {
            global = true;
        } else if token.eq_ignore_ascii_case("persist") {
            persist = true;
        }
    }
    name.map(|name| ModConfigConstant {
        name,
        global,
        persist,
    })
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
        let (namespace, constants) = parse_ini_namespace_and_constants(&text);
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(ModConfigIni {
            rel_path,
            namespace,
            constants,
            sections,
        });
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

    #[test]
    fn parses_constants_flags_in_any_order() {
        let ini = "\
[Constants]
global persist $swapvar = 0
persist global $other = 1
global $volatile = 2
$plain = 3
local $ignored = 4
";
        let (_, constants) = parse_ini_namespace_and_constants(ini);
        assert_eq!(
            constants,
            vec![
                ModConfigConstant {
                    name: "swapvar".into(),
                    global: true,
                    persist: true,
                },
                ModConfigConstant {
                    name: "other".into(),
                    global: true,
                    persist: true,
                },
                ModConfigConstant {
                    name: "volatile".into(),
                    global: true,
                    persist: false,
                },
                ModConfigConstant {
                    name: "plain".into(),
                    global: false,
                    persist: false,
                },
                ModConfigConstant {
                    name: "ignored".into(),
                    global: false,
                    persist: false,
                },
            ]
        );
    }

    #[test]
    fn explicit_namespace_is_read_from_preamble_only() {
        let with_ns = "namespace = my\\space\n[Constants]\nglobal $x = 0\n";
        let (namespace, _) = parse_ini_namespace_and_constants(with_ns);
        assert_eq!(namespace.as_deref(), Some("my\\space"));

        // A `namespace =` after the first section is not a preamble directive.
        let after = "[Constants]\nnamespace = late\nglobal $x = 0\n";
        let (namespace, constants) = parse_ini_namespace_and_constants(after);
        assert_eq!(namespace, None);
        // `namespace = late` has no `$` token, so it is not a variable declaration.
        assert_eq!(constants.len(), 1);
        assert_eq!(constants[0].name, "x");
    }

    #[test]
    fn constants_only_come_from_the_constants_section() {
        let ini = "[Constants]\nglobal $a = 0\n[KeyA]\nkey = a\nglobal $b = 1\n";
        let (_, constants) = parse_ini_namespace_and_constants(ini);
        assert_eq!(constants.len(), 1);
        assert_eq!(constants[0].name, "a");
    }

    #[test]
    fn parse_mod_config_records_namespace_and_constants() {
        let dir = tempfile::tempdir().unwrap();
        let ini = "\
namespace = foo\\bar
[Constants]
global persist $swapvar = 0
[KeySwap]
key = VK_DOWN
$swapvar = 0,1,2
";
        fs::write(dir.path().join("merged.ini"), ini).unwrap();
        let inis = parse_mod_config_inis(dir.path());
        assert_eq!(inis.len(), 1);
        assert_eq!(inis[0].namespace.as_deref(), Some("foo\\bar"));
        assert_eq!(inis[0].constants.len(), 1);
        assert!(inis[0].constants[0].global && inis[0].constants[0].persist);
    }

    fn mirror_test_entry(root_path: std::path::PathBuf) -> ModEntry {
        ModEntry {
            id: "mod".to_string(),
            game_id: "test".to_string(),
            folder_name: "Arcane".to_string(),
            root_path,
            status: ModStatus::Active,
            metadata: crate::model::ModMetadata::default(),
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            source: None,
            update_state: crate::model::ModUpdateState::Unlinked,
        }
    }

    #[test]
    fn mod_mirror_set_resolves_only_shown_global_non_persist_vars() {
        let importer = tempfile::tempdir().unwrap();
        let mods_path = importer.path().join("Mods");
        let mod_root = mods_path.join("Arcane");
        fs::create_dir_all(&mod_root).unwrap();
        // `swapvar` is a shown, `global`, non-`persist` cycle var → mirrored.
        // `saved` is `global persist` → 3DMigoto already flushes it, so no mirror.
        fs::write(
            mod_root.join("mod.ini"),
            "\
[Constants]
global $swapvar = 0
global persist $saved = 0
[KeySwap]
key = VK_DOWN
type = cycle
$swapvar = 0,1,2
[KeySaved]
key = VK_UP
type = cycle
$saved = 0,1
",
        )
        .unwrap();
        let entry = mirror_test_entry(mod_root);

        let (mirrors, readbacks) = HestiaApp::mod_mirror_set(&entry, importer.path());

        let expected = xxmi_persist::MirrorVar {
            namespace_prefix: "$\\mods\\arcane\\mod.ini\\".to_string(),
            var_name: "swapvar".to_string(),
        };
        assert_eq!(mirrors, vec![expected.clone()]);
        assert_eq!(readbacks.len(), 1);
        assert_eq!(readbacks[0].insert_key, "mod.ini\\swapvar");
        let helper_prefix =
            xxmi_persist::hestia_helper_namespace_prefix(importer.path(), &entry.root_path)
                .unwrap();
        assert_eq!(
            readbacks[0].persist_key,
            xxmi_persist::mirror_persist_key(&helper_prefix, &expected)
        );
    }
}
