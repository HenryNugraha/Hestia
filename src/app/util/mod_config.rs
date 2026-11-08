#[derive(Debug, Clone, PartialEq, Eq)]
struct ModConfigSection {
    header: String,
    lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModConfigConstant {
    name: String,
    global: bool,
    persist: bool,
    value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModConfigIni {
    rel_path: String,
    namespace: Option<String>,
    constants: Vec<ModConfigConstant>,
    sections: Vec<ModConfigSection>,
}

fn ini_line_is_key_binding(line: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("key"))
}

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

fn parse_constant_declaration(line: &str) -> Option<ModConfigConstant> {
    let (head, value) = line
        .split_once('=')
        .map_or((line, None), |(head, value)| (head, Some(value)));
    let value = value
        .map(|value| value.split(';').next().unwrap_or("").trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
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
        value,
    })
}

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
                    value: Some("0".into()),
                },
                ModConfigConstant {
                    name: "other".into(),
                    global: true,
                    persist: true,
                    value: Some("1".into()),
                },
                ModConfigConstant {
                    name: "volatile".into(),
                    global: true,
                    persist: false,
                    value: Some("2".into()),
                },
                ModConfigConstant {
                    name: "plain".into(),
                    global: false,
                    persist: false,
                    value: Some("3".into()),
                },
                ModConfigConstant {
                    name: "ignored".into(),
                    global: false,
                    persist: false,
                    value: Some("4".into()),
                },
            ]
        );
    }

    #[test]
    fn explicit_namespace_is_read_from_preamble_only() {
        let with_ns = "namespace = my\\space\n[Constants]\nglobal $x = 0\n";
        let (namespace, _) = parse_ini_namespace_and_constants(with_ns);
        assert_eq!(namespace.as_deref(), Some("my\\space"));

        let after = "[Constants]\nnamespace = late\nglobal $x = 0\n";
        let (namespace, constants) = parse_ini_namespace_and_constants(after);
        assert_eq!(namespace, None);
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

    fn rows_from(ini: &str) -> Vec<HotkeyListRow> {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mod.ini"), ini).unwrap();
        hotkeys_list_rows(&parse_mod_config_inis(dir.path()))
    }

    #[test]
    fn toggles_that_run_a_command_list_are_still_listed() {
        let rows = rows_from(
            "\
[Constants]
global persist $earing = 0
global persist $body_set = 0
[KeySwapEaring]
condition = $object_detected == 1
key = ctrl no_shift no_alt VK_LEFT
type = cycle
$earing = 0,1
run = CommandListProcessToggles
[KeySwapBodySet]
condition = $object_detected == 1
key = ctrl no_shift no_alt VK_OEM_2
type = cycle
$body_set = 0,1
run = CommandListProcessToggles
",
        );
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| !row.is_action && row.values == ["0", "1"]));
        assert!(rows.iter().any(|row| row.key == "Ctrl+Left"));
        assert!(rows.iter().any(|row| row.key == "Ctrl+/"));
    }

    #[test]
    fn a_command_fed_by_one_fixed_value_is_an_action() {
        let rows = rows_from(
            "\
[KeyNextVariant]
condition = $\\TangtangUnified\\object_detected == 1
key = no_ctrl no_shift no_alt VK_OEM_2
type = press
$direction = 1
run = CommandListCycleVariant
[KeyMenuNavigateDown]
key = no_ctrl no_shift no_alt VK_DOWN
run = CommandListNavigate
",
        );
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_action);
        assert_eq!(rows[0].key, "/");
        assert_eq!(rows[0].label, "Next Variant");
        assert!(rows[0].conditions.is_empty());
    }

    #[test]
    fn rows_sharing_a_key_wait_on_the_toggle_their_condition_checks() {
        let ini = "\
[Constants]
global persist $body = 0
global persist $dressleotard = 0
global persist $dressnude = 0
[KeyToggleGilbertaBody]
condition = $object_detected == 1
key = no_shift no_alt ctrl VK_UP
type = cycle
$body = 0,1,2,3
[KeyToggleGilbertaDressLeotard]
condition = $object_detected == 1 && $body == 0
key = no_ctrl no_shift alt VK_UP
type = cycle
$dressleotard = 0,1,2
[KeyToggleGilbertaDressNude]
condition = $object_detected == 1 && ($body == 1)
key = no_ctrl no_shift alt VK_UP
type = cycle
$dressnude = 0,1,2
";
        let rows = rows_from(ini);
        let row = |var: &str| {
            rows.iter()
                .find(|row| row.var_name.as_deref() == Some(var))
                .unwrap()
        };
        let unavailable = |var: &str, values: &HashMap<String, String>| {
            hotkey_row_unavailable(row(var), &rows, values)
        };

        let unsaved = HashMap::new();
        assert!(!unavailable("dressleotard", &unsaved));
        assert!(unavailable("dressnude", &unsaved));
        assert!(!unavailable("body", &unsaved));

        let saved = HashMap::from([("mod.ini\\body".to_string(), "1".to_string())]);
        assert!(unavailable("dressleotard", &saved));
        assert!(!unavailable("dressnude", &saved));
    }

    #[test]
    fn condition_checks_skip_what_they_cannot_judge() {
        assert_eq!(
            hotkey_condition_checks("$object_detected && ($Lock == 0)"),
            vec![("Lock".to_string(), "0".to_string())]
        );
        assert!(hotkey_condition_checks("$a == 1 || $b == 2").is_empty());
        assert!(hotkey_condition_checks("$\\ns\\a == 1").is_empty());
        assert!(hotkey_condition_checks("$a == $b").is_empty());
        assert!(hotkey_values_match("1.0", "1"));
        assert!(!hotkey_values_match("1", "0"));
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
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
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
