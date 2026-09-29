use super::{overall, system, Event, Job, INNO_LEFTOVERS, KNOWN_FILES, SHELL_DLL, UNINSTALLER};
use std::{fs, path::Path};

const PACKAGE_METADATA: &str = "microsoft.system.package.metadata";

pub fn run(job: &Job, report: &mut dyn FnMut(Event)) -> Result<(), String> {
    let dir = job.dir.as_path();
    report(Event::Progress(0.));
    if !job.files_only {
        system::unregister_modern_menu();
        report(Event::Progress(overall(0, 0.7)));
        system::unregister_classic_menu()?;
    }
    report(Event::Progress(overall(0, 1.)));

    if !job.files_only {
        system::forget_defaults();
        system::unregister_associations()?;
        report(Event::Progress(overall(1, 0.5)));
        system::remove_from_path(dir)?;
        system::remove_shortcut();
    }
    report(Event::Progress(overall(1, 1.)));

    remove_files(dir, job.files_only, report);
    report(Event::Progress(overall(2, 1.)));

    if !job.files_only {
        system::remove_uninstall_entry()?;
        system::notify_changes();
    }
    system::delete_own_copy_later();
    report(Event::Progress(100.));
    Ok(())
}

fn remove_files(dir: &Path, files_only: bool, report: &mut dyn FnMut(Event)) {
    let names: Vec<&str> = KNOWN_FILES
        .iter()
        .copied()
        .chain([UNINSTALLER])
        .chain(INNO_LEFTOVERS)
        .collect();
    let total = names.len() as f32;
    let mut explorer_restarted = false;
    for (index, name) in names.iter().enumerate() {
        let path = dir.join(name);
        if path.exists() && fs::remove_file(&path).is_err() {
            if *name == SHELL_DLL && !files_only && !explorer_restarted {
                system::restart_explorer();
                explorer_restarted = true;
            }
            if fs::remove_file(&path).is_err() {
                let _ = system::move_aside(&path);
            }
        }
        report(Event::Progress(overall(2, (index + 1) as f32 / total)));
    }
    let _ = fs::remove_dir_all(dir.join(PACKAGE_METADATA));
    system::sweep_leftovers(dir);
    let _ = fs::remove_dir(dir.join("Assets"));
    let _ = fs::remove_dir(dir);
}
