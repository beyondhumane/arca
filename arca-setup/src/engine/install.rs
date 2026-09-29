use super::{
    overall, payload, stamp_manifest, system, Event, Job, INNO_LEFTOVERS, MANIFEST, SHELL_DLL,
    VERSION,
};
use std::{fs, path::Path};
use system::ctx;

pub fn run(job: &Job, report: &mut dyn FnMut(Event)) -> Result<(), String> {
    let dir = job.dir.as_path();
    let files = payload::files()?;
    report(Event::Progress(0.));
    fs::create_dir_all(dir).map_err(ctx(format!("creating {}", dir.display())))?;
    system::sweep_leftovers(dir);

    let mut shell_moved = false;
    let mut size = 0u64;
    let total = files.len().max(1) as f32;
    for (index, file) in files.iter().enumerate() {
        let target = dir.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(ctx(format!("creating {}", parent.display())))?;
        }
        let stamped;
        let bytes: &[u8] = if file.path == Path::new(MANIFEST) {
            stamped = stamp_manifest(&String::from_utf8_lossy(&file.bytes), VERSION).into_bytes();
            &stamped
        } else {
            &file.bytes
        };
        let moved = system::replace_file(&target, bytes)?;
        if moved && file.path == Path::new(SHELL_DLL) {
            shell_moved = true;
        }
        size += bytes.len() as u64;
        report(Event::Progress(overall(0, (index + 1) as f32 / total)));
    }

    system::install_uninstaller(dir)?;
    for name in INNO_LEFTOVERS {
        let _ = fs::remove_file(dir.join(name));
    }
    report(Event::Progress(overall(1, 0.4)));
    if !job.files_only {
        if job.choices.menu {
            system::register_modern_menu(dir);
        } else {
            system::unregister_modern_menu();
        }
    }
    report(Event::Progress(overall(1, 1.)));

    if !job.files_only {
        if job.choices.menu {
            system::register_classic_menu(dir)?;
        } else {
            system::unregister_classic_menu()?;
        }
        report(Event::Progress(overall(2, 0.25)));
        if job.choices.assoc {
            system::register_associations(dir)?;
        } else {
            system::unregister_associations()?;
        }
        report(Event::Progress(overall(2, 0.5)));
        if job.choices.path {
            system::add_to_path(dir)?;
        } else {
            system::remove_from_path(dir)?;
        }
        report(Event::Progress(overall(2, 0.7)));
        let _ = system::create_shortcut(dir);
        report(Event::Progress(overall(2, 0.85)));
        let size_kb = u32::try_from(size / 1024).unwrap_or(u32::MAX);
        system::write_uninstall_entry(dir, size_kb)?;
    }
    report(Event::Progress(overall(2, 1.)));

    if !job.files_only {
        system::notify_changes();
        if shell_moved && !job.quiet_update {
            system::restart_explorer();
        }
    }
    report(Event::Progress(overall(3, 0.8)));
    if job.quiet_update && !job.files_only {
        super::launch_app(dir);
    }
    report(Event::Progress(100.));
    Ok(())
}
