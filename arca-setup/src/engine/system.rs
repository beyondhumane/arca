use super::{
    encoded_command, path_with, path_without, quote_ps, Choices, GUI_EXE, MANIFEST, SHELL_DLL,
    UNINSTALLER, VERSION,
};
use std::{
    fmt::Display,
    fs, io,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use winreg::{enums::*, RegKey, RegValue};

type Res<T> = Result<T, String>;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CLASSIC_CLSID: &str = "{B528A7F3-C889-4C98-B052-5D7F7F778E14}";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{7C4E0E4A-6C0D-4C21-9E0B-2B5D0F1A9C77}_is1";
const MODERN_PACKAGE: &str = "Arca.Archivador";
const PUBLISHER: &str = "Arca Project";
const URL: &str = "https://github.com/THIONG/arca";
const POWERSHELL_TIMEOUT: Duration = Duration::from_secs(90);

const ASSOCIATIONS: [(&str, &str, &str); 4] = [
    (".zip", "Arca.zip", "ZIP archive"),
    (".tar", "Arca.tar", "TAR archive"),
    (".gz", "Arca.targz", "Compressed TAR archive"),
    (".tgz", "Arca.tgz", "Compressed TAR archive"),
];

pub fn ctx<E: Display>(what: impl Into<String>) -> impl FnOnce(E) -> String {
    let what = what.into();
    move |error| format!("{what}: {error}")
}

fn hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

fn set(path: &str, name: &str, value: &str) -> Res<()> {
    let (key, _) = hkcu()
        .create_subkey(path)
        .map_err(ctx(format!("registry key {path}")))?;
    key.set_value(name, &value)
        .map_err(ctx(format!("registry value {path}\\{name}")))
}

fn set_dword(path: &str, name: &str, value: u32) -> Res<()> {
    let (key, _) = hkcu()
        .create_subkey(path)
        .map_err(ctx(format!("registry key {path}")))?;
    key.set_value(name, &value)
        .map_err(ctx(format!("registry value {path}\\{name}")))
}

fn delete_tree(path: &str) -> Res<()> {
    match hkcu().delete_subkey_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("registry key {path}: {error}")),
    }
}

fn delete_value(path: &str, name: &str) -> Res<()> {
    let Ok(key) = hkcu().open_subkey_with_flags(path, KEY_WRITE) else {
        return Ok(());
    };
    match key.delete_value(name) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("registry value {path}\\{name}: {error}")),
    }
}

fn exists(path: &str) -> bool {
    hkcu().open_subkey(path).is_ok()
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn text_of(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_string()
}

fn env_path() -> Res<(RegKey, String)> {
    let (key, _) = hkcu()
        .create_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .map_err(ctx("registry key Environment"))?;
    let current = key
        .get_raw_value("Path")
        .map(|value| text_of(&value.bytes))
        .unwrap_or_default();
    Ok((key, current))
}

fn write_env_path(key: &RegKey, text: &str) -> Res<()> {
    key.set_raw_value(
        "Path",
        &RegValue {
            bytes: utf16_bytes(text),
            vtype: REG_EXPAND_SZ,
        },
    )
    .map_err(ctx("registry value Environment\\Path"))
}

pub fn add_to_path(dir: &Path) -> Res<()> {
    let (key, current) = env_path()?;
    let updated = path_with(&current, &dir.display().to_string());
    if updated == current {
        return Ok(());
    }
    write_env_path(&key, &updated)
}

pub fn remove_from_path(dir: &Path) -> Res<()> {
    let (key, current) = env_path()?;
    let updated = path_without(&current, &dir.display().to_string());
    if updated == current {
        return Ok(());
    }
    write_env_path(&key, &updated)
}

pub fn register_classic_menu(dir: &Path) -> Res<()> {
    let server = format!(r"Software\Classes\CLSID\{CLASSIC_CLSID}\InprocServer32");
    set(&server, "", &dir.join(SHELL_DLL).display().to_string())?;
    set(&server, "ThreadingModel", "Apartment")?;
    for kind in ["*", "Directory"] {
        set(
            &format!(r"Software\Classes\{kind}\shellex\ContextMenuHandlers\Arca"),
            "",
            CLASSIC_CLSID,
        )?;
    }
    Ok(())
}

pub fn unregister_classic_menu() -> Res<()> {
    for kind in ["*", "Directory"] {
        delete_tree(&format!(
            r"Software\Classes\{kind}\shellex\ContextMenuHandlers\Arca"
        ))?;
    }
    delete_tree(&format!(r"Software\Classes\CLSID\{CLASSIC_CLSID}"))
}

pub fn register_associations(dir: &Path) -> Res<()> {
    let exe = dir.join(GUI_EXE).display().to_string();
    let command = format!("\"{exe}\" \"%1\"");
    let application = format!(r"Software\Classes\Applications\{GUI_EXE}");
    let associations = r"Software\Arca\Capabilities\FileAssociations";
    for (extension, progid, description) in ASSOCIATIONS {
        let class = format!(r"Software\Classes\{progid}");
        set(&class, "", description)?;
        set(&format!(r"{class}\DefaultIcon"), "", &format!("{exe},0"))?;
        set(&format!(r"{class}\shell\open\command"), "", &command)?;
        set(
            &format!(r"Software\Classes\{extension}\OpenWithProgids"),
            progid,
            "",
        )?;
        set(&format!(r"{application}\SupportedTypes"), extension, "")?;
        set(associations, extension, progid)?;
    }
    set(&application, "FriendlyAppName", "Arca")?;
    set(&format!(r"{application}\shell\open\command"), "", &command)?;
    set(r"Software\Arca\Capabilities", "ApplicationName", "Arca")?;
    set(
        r"Software\Arca\Capabilities",
        "ApplicationDescription",
        "Fast, safe archiver",
    )?;
    set(
        r"Software\RegisteredApplications",
        "Arca",
        r"Software\Arca\Capabilities",
    )
}

pub fn unregister_associations() -> Res<()> {
    for (extension, progid, _) in ASSOCIATIONS {
        delete_tree(&format!(r"Software\Classes\{progid}"))?;
        delete_value(
            &format!(r"Software\Classes\{extension}\OpenWithProgids"),
            progid,
        )?;
    }
    delete_tree(&format!(r"Software\Classes\Applications\{GUI_EXE}"))?;
    delete_tree(r"Software\Arca")?;
    delete_value(r"Software\RegisteredApplications", "Arca")
}

pub fn forget_defaults() {
    for (extension, _, _) in ASSOCIATIONS {
        let choice = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{extension}\UserChoice"
        );
        let ours = hkcu()
            .open_subkey(&choice)
            .and_then(|key| key.get_value::<String, _>("ProgId"))
            .is_ok_and(|progid| progid.starts_with("Arca."));
        if ours {
            let _ = delete_tree(&choice);
        }
    }
}

pub fn write_uninstall_entry(dir: &Path, size_kb: u32) -> Res<()> {
    let uninstaller = dir.join(UNINSTALLER).display().to_string();
    let values = [
        ("DisplayName", format!("Arca {VERSION}")),
        ("DisplayVersion", VERSION.to_string()),
        ("Publisher", PUBLISHER.to_string()),
        ("URLInfoAbout", URL.to_string()),
        ("HelpLink", URL.to_string()),
        ("DisplayIcon", dir.join(GUI_EXE).display().to_string()),
        ("InstallLocation", format!("{}\\", dir.display())),
        ("UninstallString", format!("\"{uninstaller}\" --uninstall")),
        (
            "QuietUninstallString",
            format!("\"{uninstaller}\" --uninstall /VERYSILENT"),
        ),
    ];
    for (name, value) in values {
        set(UNINSTALL_KEY, name, &value)?;
    }
    set_dword(UNINSTALL_KEY, "NoModify", 1)?;
    set_dword(UNINSTALL_KEY, "NoRepair", 1)?;
    set_dword(UNINSTALL_KEY, "EstimatedSize", size_kb)
}

pub fn remove_uninstall_entry() -> Res<()> {
    delete_tree(UNINSTALL_KEY)
}

pub fn existing_location() -> Option<PathBuf> {
    let key = hkcu().open_subkey(UNINSTALL_KEY).ok()?;
    let recorded: String = key.get_value("InstallLocation").ok()?;
    let dir = PathBuf::from(recorded.trim_end_matches(['\\', '/']));
    dir.join(GUI_EXE).exists().then_some(dir)
}

pub fn installed_choices(dir: &Path) -> Choices {
    let current = env_path().map(|(_, text)| text).unwrap_or_default();
    Choices {
        menu: exists(&format!(r"Software\Classes\CLSID\{CLASSIC_CLSID}")),
        assoc: exists(r"Software\Arca\Capabilities"),
        path: super::path_has(&current, &dir.display().to_string()),
    }
}

pub fn default_location() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
    base.join("Programs").join("Arca")
}

fn system32(program: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join("System32").join(program)
}

fn powershell_command(script: &str) -> Command {
    let mut command = Command::new(system32(r"WindowsPowerShell\v1.0\powershell.exe"));
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &encoded_command(script),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    command
}

fn wait_for(mut command: Command, timeout: Duration) -> Res<()> {
    let mut child = command.spawn().map_err(ctx("could not start a helper"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("a helper failed with {status}")),
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                return Err("a helper took too long".to_string());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(format!("a helper: {error}")),
        }
    }
}

fn powershell(script: &str) -> Res<()> {
    wait_for(powershell_command(script), POWERSHELL_TIMEOUT)
}

pub fn register_modern_menu(dir: &Path) {
    let script = format!(
        "Add-AppxPackage -Register {} -ExternalLocation {} -ErrorAction SilentlyContinue",
        quote_ps(&dir.join(MANIFEST).display().to_string()),
        quote_ps(&dir.display().to_string()),
    );
    let _ = powershell(&script);
}

pub fn unregister_modern_menu() {
    let script = format!(
        "Get-AppxPackage {MODERN_PACKAGE} | Remove-AppxPackage -ErrorAction SilentlyContinue"
    );
    let _ = powershell(&script);
}

fn shortcut_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|base| {
        PathBuf::from(base)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Arca.lnk")
    })
}

pub fn create_shortcut(dir: &Path) -> Res<()> {
    let Some(link) = shortcut_path() else {
        return Ok(());
    };
    let exe = dir.join(GUI_EXE).display().to_string();
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({}); \
         $s.TargetPath = {}; $s.WorkingDirectory = {}; $s.IconLocation = {}; $s.Save()",
        quote_ps(&link.display().to_string()),
        quote_ps(&exe),
        quote_ps(&dir.display().to_string()),
        quote_ps(&format!("{exe},0")),
    );
    powershell(&script)
}

pub fn remove_shortcut() {
    if let Some(link) = shortcut_path() {
        let _ = fs::remove_file(link);
    }
}

pub fn move_aside(target: &Path) -> io::Result<PathBuf> {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    for number in 1..=99 {
        let aside = target.with_file_name(format!("{name}.old-{number}"));
        if aside.exists() {
            continue;
        }
        fs::rename(target, &aside)?;
        return Ok(aside);
    }
    Err(io::Error::other("no free name for the old copy"))
}

pub fn replace_file(target: &Path, bytes: &[u8]) -> Res<bool> {
    match fs::write(target, bytes) {
        Ok(()) => Ok(false),
        Err(first) if !target.exists() => Err(format!("{}: {first}", target.display())),
        Err(_) => {
            move_aside(target).map_err(ctx(format!("{} is in use", target.display())))?;
            fs::write(target, bytes).map_err(ctx(target.display().to_string()))?;
            Ok(true)
        }
    }
}

pub fn sweep_leftovers(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().contains(".old-") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

pub fn install_uninstaller(dir: &Path) -> Res<()> {
    let own = std::env::current_exe().map_err(ctx("the installer's own path"))?;
    let target = dir.join(UNINSTALLER);
    if own == target {
        return Ok(());
    }
    let bytes = fs::read(&own).map_err(ctx("reading the installer to keep it as uninstaller"))?;
    replace_file(&target, &bytes).map(|_| ())
}

fn explorer_running() -> bool {
    Command::new(system32("tasklist.exe"))
        .args(["/FI", "IMAGENAME eq explorer.exe", "/NH"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("explorer.exe"))
}

pub fn restart_explorer() {
    let mut kill = Command::new(system32("taskkill.exe"));
    kill.args(["/F", "/IM", "explorer.exe"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    let _ = wait_for(kill, Duration::from_secs(15));
    thread::sleep(Duration::from_millis(1500));
    if !explorer_running() {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let _ = Command::new(PathBuf::from(root).join("explorer.exe")).spawn();
    }
}

pub fn launch(exe: &Path, dir: &Path) {
    let _ = Command::new(exe).current_dir(dir).spawn();
}

pub fn relaunch_from_temp(args: &[String]) -> Res<()> {
    let own = std::env::current_exe().map_err(ctx("the installer's own path"))?;
    let copy = std::env::temp_dir().join(format!("arca-uninstall-{}.exe", std::process::id()));
    fs::copy(&own, &copy).map_err(ctx("copying the uninstaller out of the folder"))?;
    Command::new(&copy)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(ctx("starting the uninstaller"))
}

pub fn delete_own_copy_later() {
    let Ok(own) = std::env::current_exe() else {
        return;
    };
    let is_temp_copy = own
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("arca-uninstall-"));
    if !is_temp_copy {
        return;
    }
    let script = format!(
        "Wait-Process -Id {} -ErrorAction SilentlyContinue; \
         Remove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue",
        std::process::id(),
        quote_ps(&own.display().to_string()),
    );
    let _ = powershell_command(&script).spawn();
}

pub fn notify_changes() {
    arca_notify::associations_changed();
    arca_notify::environment_changed();
}
