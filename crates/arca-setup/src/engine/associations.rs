use arca_core::{Association, Container, Format};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileType {
    pub extension: String,
    pub progid: String,
    pub description: String,
    pub on_request: bool,
}

pub fn file_types() -> Vec<FileType> {
    Format::desktop_extensions()
        .into_iter()
        .map(|(ext, format)| FileType {
            extension: format!(".{ext}"),
            progid: progid(ext),
            description: if format.container() == Container::Iso {
                "ISO disc image".to_string()
            } else {
                format!("{} archive", format.label())
            },
            on_request: format.association() == Association::OnRequest,
        })
        .collect()
}

// Installs made before the list came from arca-core registered .gz under this
// name, and upgrading or uninstalling them has to find it again.
fn progid(ext: &str) -> String {
    if ext == "gz" {
        "Arca.targz".to_string()
    } else {
        format!("Arca.{ext}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, fs, path::PathBuf};

    fn repo(path: &str) -> String {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path);
        fs::read_to_string(&file).unwrap_or_else(|error| panic!("{}: {error}", file.display()))
    }

    fn extensions(on_request: bool) -> BTreeSet<String> {
        file_types()
            .into_iter()
            .filter(|kind| kind.on_request == on_request)
            .map(|kind| kind.extension)
            .collect()
    }

    fn quoted(text: &str) -> Vec<String> {
        text.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn earlier_progids_are_kept_and_iso_waits_to_be_asked() {
        let kinds = file_types();
        let find = |ext: &str| kinds.iter().find(|kind| kind.extension == ext).unwrap();
        assert_eq!(find(".zip").progid, "Arca.zip");
        assert_eq!(find(".7z").progid, "Arca.7z");
        assert_eq!(find(".tar").progid, "Arca.tar");
        assert_eq!(find(".gz").progid, "Arca.targz");
        assert_eq!(find(".tgz").progid, "Arca.tgz");
        assert_eq!(find(".xz").description, "XZ archive");
        assert_eq!(find(".rar").description, "RAR archive");
        assert_eq!(find(".iso").description, "ISO disc image");
        assert_eq!(find(".cab").description, "CAB archive");
        assert_eq!(find(".lha").description, "LZH archive");
        assert_eq!(extensions(true), BTreeSet::from([".iso".to_string()]));
        for never in [
            ".cbr", ".jar", ".apk", ".epub", ".cbz", ".docx", ".deb", ".rpm", ".a", ".ar", ".vhd",
            ".vhdx", ".vmdk", ".dmg", ".img",
        ] {
            assert!(kinds.iter().all(|kind| kind.extension != never), "{never}");
        }
        let progids: BTreeSet<&str> = kinds.iter().map(|kind| kind.progid.as_str()).collect();
        assert_eq!(progids.len(), kinds.len());
    }

    // windows/arca-shell is outside the workspace; this is what keeps its copy
    // of the suffix list from drifting.
    #[test]
    fn the_explorer_extension_knows_every_suffix_in_order() {
        let source = repo("windows/arca-shell/src/lib.rs");
        let start = source
            .find("const SUFFIXES")
            .expect("arca-shell declares SUFFIXES");
        let list = &source[start..];
        let list = &list[list.find('=').unwrap()..list.find("];").unwrap()];
        let core: Vec<String> = Format::SUFFIXES
            .iter()
            .map(|(suffix, _)| suffix.to_string())
            .collect();
        assert_eq!(quoted(list), core);
    }

    #[test]
    fn the_inno_installer_registers_the_same_types() {
        let script = repo("windows/arca.iss");
        let mut open_with = BTreeSet::new();
        let mut supported = BTreeSet::new();
        let mut capabilities = BTreeSet::new();
        let mut classes = BTreeSet::new();
        let asked_for: Vec<FileType> = file_types()
            .into_iter()
            .filter(|kind| kind.on_request)
            .collect();
        let mut removed = BTreeSet::new();
        for line in script.lines().filter(|line| line.starts_with("Root: HKCU")) {
            if line.contains("Tasks: not ") {
                removed.insert(line.to_string());
                continue;
            }
            let parts = quoted(line);
            let (Some(key), Some(name)) = (parts.first(), parts.get(1)) else {
                continue;
            };
            let about_asked_for = asked_for.iter().any(|kind| {
                line.contains(&kind.progid)
                    || line.contains(&format!("\"{}\"", kind.extension))
                    || line.contains(&format!(r"\{}\", kind.extension))
            });
            assert_eq!(
                line.contains(r"Tasks: fileassoc\iso"),
                about_asked_for,
                "{line}"
            );
            if let Some(ext) = key
                .strip_prefix(r"Software\Classes\")
                .and_then(|rest| rest.strip_suffix(r"\OpenWithProgids"))
            {
                open_with.insert((ext.to_string(), name.clone()));
            } else if key.ends_with(r"arca-gui.exe\SupportedTypes") {
                supported.insert(name.clone());
            } else if key.ends_with(r"Capabilities\FileAssociations") {
                capabilities.insert((name.clone(), parts.get(2).cloned().unwrap_or_default()));
            } else if key.starts_with(r"Software\Classes\Arca.") {
                classes.insert(key.clone());
            }
        }
        let mut expected_classes = BTreeSet::new();
        for kind in file_types() {
            let task = if kind.on_request {
                r"fileassoc\iso"
            } else {
                "fileassoc"
            };
            let class = format!(r"Software\Classes\{}", kind.progid);
            let open = format!(r"{class}\shell\open\command");
            let icon = format!(r"{class}\DefaultIcon");
            for wanted in [
                format!(
                    r#"Root: HKCU; Subkey: "{class}"; ValueType: string; ValueName: ""; ValueData: "{}"; Flags: uninsdeletekey; Tasks: {task}"#,
                    kind.description
                ),
                format!(
                    r#"Root: HKCU; Subkey: "{icon}"; ValueType: string; ValueName: ""; ValueData: "{{app}}\arca-gui.exe,0"; Tasks: {task}"#
                ),
                format!(
                    r#"Root: HKCU; Subkey: "{open}"; ValueType: string; ValueName: ""; ValueData: """{{app}}\arca-gui.exe"" ""%1"""; Tasks: {task}"#
                ),
            ] {
                assert!(script.lines().any(|line| line == wanted), "{wanted}");
            }
            expected_classes.extend([class, icon, open]);
        }
        assert_eq!(classes, expected_classes);
        let expected: BTreeSet<(String, String)> = file_types()
            .into_iter()
            .map(|kind| (kind.extension, kind.progid))
            .collect();
        let all = extensions(false)
            .union(&extensions(true))
            .cloned()
            .collect();
        assert_eq!(open_with, expected);
        assert_eq!(capabilities, expected);
        assert_eq!(supported, all);
        assert!(!asked_for.is_empty());
        let unless = r"Tasks: not fileassoc\iso";
        let mut taken_back = BTreeSet::new();
        for kind in &asked_for {
            let (ext, progid) = (&kind.extension, &kind.progid);
            taken_back.extend([
                format!(
                    r#"Root: HKCU; Subkey: "Software\Classes\{progid}"; ValueType: none; Flags: deletekey; {unless}"#
                ),
                format!(
                    r#"Root: HKCU; Subkey: "Software\Classes\{ext}\OpenWithProgids"; ValueType: none; ValueName: "{progid}"; Flags: deletevalue; {unless}"#
                ),
                format!(
                    r#"Root: HKCU; Subkey: "Software\Classes\Applications\arca-gui.exe\SupportedTypes"; ValueType: none; ValueName: "{ext}"; Flags: deletevalue; {unless}"#
                ),
                format!(
                    r#"Root: HKCU; Subkey: "Software\Arca\Capabilities\FileAssociations"; ValueType: none; ValueName: "{ext}"; Flags: deletevalue; {unless}"#
                ),
            ]);
            assert!(
                script.contains(&format!("ForgetDefault('{ext}');")),
                "{ext}"
            );
        }
        assert_eq!(removed, taken_back);
        let forget = script
            .lines()
            .filter_map(|line| line.trim().strip_prefix("Exts["))
            .filter_map(|line| line.split('\'').nth(1).map(str::to_string))
            .collect::<BTreeSet<_>>();
        let last = file_types().len() - 1;
        assert!(script.contains(&format!("Exts: array[0..{last}] of string;")));
        assert!(script.contains(&format!("for I := 0 to {last} do")));
        assert_eq!(forget, all);
    }

    #[test]
    fn the_development_install_script_registers_the_same_types() {
        let script = repo("windows/install.ps1");
        let table = |name: &str| -> BTreeSet<(String, String)> {
            let start = script.find(&format!("${name} = @{{")).expect(name);
            script[start..]
                .lines()
                .skip(1)
                .take_while(|line| line.trim() != "}")
                .map(|line| {
                    let parts = quoted(line);
                    (parts[0].clone(), parts[1].clone())
                })
                .collect()
        };
        let expected = |on_request: bool| -> BTreeSet<(String, String)> {
            file_types()
                .into_iter()
                .filter(|kind| kind.on_request == on_request)
                .map(|kind| (kind.extension, kind.progid))
                .collect()
        };
        assert_eq!(table("Formats"), expected(false));
        assert_eq!(table("OnRequest"), expected(true));
        assert!(script.contains("$Known = $Formats + $OnRequest"));
        assert!(script.contains(
            "if (-not $Registered.ContainsKey($ext)) { Unregister-Type $ext $Known[$ext].ProgId }"
        ));
        let start = script.find("function Unregister-Type(").unwrap();
        let body = &script[start..start + script[start..].find("\n}").unwrap()];
        for place in [
            r#""Software\Classes\$progid""#,
            r"OpenWithProgids",
            r"arca-gui.exe\SupportedTypes",
            r"Capabilities\FileAssociations",
            r"UserChoice",
        ] {
            assert!(body.contains(place), "{place}");
        }
    }

    fn mime_types() -> BTreeSet<String> {
        Format::SUFFIXES
            .iter()
            .flat_map(|(_, format)| format.mime_types())
            .map(|mime| mime.to_string())
            .collect()
    }

    fn desktop_key(text: &str, key: &str) -> Vec<String> {
        text.lines()
            .filter_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(str::to_string)
            .collect()
    }

    fn list(value: &str) -> BTreeSet<String> {
        value
            .split(';')
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_linux_desktop_entry_and_dolphin_menu_list_every_offered_type() {
        for file in [
            "packaging/linux/arca.desktop",
            "packaging/linux/arca-extract.desktop",
        ] {
            let entry = repo(file);
            let lines = desktop_key(&entry, "MimeType");
            assert_eq!(lines.len(), 1, "{file}");
            assert_eq!(list(&lines[0]), mime_types(), "{file}");
            assert!(lines[0].ends_with(';'), "{file}");
        }
    }

    #[test]
    fn the_dolphin_menus_run_flags_the_window_understands() {
        let startup = repo("crates/arca-gui/src/archive_ops/mod.rs");
        for file in [
            "packaging/linux/arca-extract.desktop",
            "packaging/linux/arca-compress.desktop",
        ] {
            let menu = repo(file);
            let commands = desktop_key(&menu, "Exec");
            assert!(!commands.is_empty(), "{file}");
            for command in commands {
                let flag = command
                    .strip_prefix("arca-gui ")
                    .and_then(|rest| rest.strip_suffix(" %F"))
                    .unwrap_or_else(|| panic!("{file}: {command}"));
                assert!(startup.contains(&format!("\"{flag}\" if")), "{flag}");
            }
        }
    }

    #[test]
    fn the_nix_package_installs_the_same_desktop_files() {
        let nix = repo("packaging/nix/package.nix");
        assert!(!nix.contains("mimeTypes"));
        for file in [
            "arca.desktop",
            "arca-extract.desktop",
            "arca-compress.desktop",
        ] {
            assert!(nix.contains(&format!("${{../linux/{file}}}")), "{file}");
        }
    }
}
