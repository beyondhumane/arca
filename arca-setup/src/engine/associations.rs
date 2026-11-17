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
            .join("..")
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
        assert_eq!(extensions(true), BTreeSet::from([".iso".to_string()]));
        for never in [".cbr", ".jar", ".apk", ".epub", ".cbz", ".docx"] {
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
        let mut on_request = BTreeSet::new();
        for line in script.lines().filter(|line| line.starts_with("Root: HKCU")) {
            let parts = quoted(line);
            let (Some(key), Some(name)) = (parts.first(), parts.get(1)) else {
                continue;
            };
            if line.contains(r"Tasks: fileassoc\iso") {
                on_request.insert(key.clone());
            }
            if let Some(ext) = key
                .strip_prefix(r"Software\Classes\")
                .and_then(|rest| rest.strip_suffix(r"\OpenWithProgids"))
            {
                open_with.insert((ext.to_string(), name.clone()));
            } else if key.ends_with(r"arca-gui.exe\SupportedTypes") {
                supported.insert(name.clone());
            } else if key.ends_with(r"Capabilities\FileAssociations") {
                capabilities.insert((name.clone(), parts.get(2).cloned().unwrap_or_default()));
            }
        }
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
        for kind in file_types().iter().filter(|kind| kind.on_request) {
            assert!(on_request.contains(&format!(r"Software\Classes\{}", kind.progid)));
            assert!(on_request.contains(&format!(
                r"Software\Classes\{}\OpenWithProgids",
                kind.extension
            )));
        }
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
    }
}
