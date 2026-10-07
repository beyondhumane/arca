use crate::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub sidebar: bool,
    pub rail: bool,
    pub preview: bool,
}

pub(crate) fn decide(width: f32, settings: &Settings) -> Layout {
    let rail = settings.sidebar_collapsed || width < 1100.;
    let sidebar = settings.folders;
    let side = if !sidebar {
        0.
    } else if rail {
        44.
    } else {
        settings.sidebar
    };
    Layout {
        sidebar,
        rail,
        preview: settings.preview_visible && width - side - settings.preview_width >= 440.,
    }
}

pub(crate) fn cycle_sidebar(settings: &mut Settings) {
    if !settings.folders {
        settings.folders = true;
        settings.sidebar_collapsed = false;
    } else if !settings.sidebar_collapsed {
        settings.sidebar_collapsed = true;
    } else {
        settings.folders = false;
    }
}

pub(crate) fn breadcrumb_indices(count: usize, width: f32) -> (Vec<usize>, Vec<usize>) {
    let capacity = if width < 900. {
        3
    } else if width < 1300. {
        5
    } else {
        7
    };
    if count <= capacity {
        return ((0..count).collect(), Vec::new());
    }
    let tail = count - capacity + 1;
    (
        std::iter::once(0).chain(tail..count).collect(),
        (1..tail).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrow_layout_preserves_preferences_and_restores_panels() {
        let settings = Settings::default();
        assert_eq!(
            decide(800., &settings),
            Layout {
                sidebar: true,
                rail: true,
                preview: false
            }
        );
        assert_eq!(
            decide(1280., &settings),
            Layout {
                sidebar: true,
                rail: false,
                preview: true
            }
        );
        assert_eq!(decide(1920., &settings), decide(1280., &settings));
        assert!(settings.preview_visible);
        assert!(!settings.sidebar_collapsed);
    }
    #[test]
    fn sidebar_toggle_cycles_expanded_rail_hidden_without_losing_width() {
        let mut settings = Settings::default();
        let width = settings.sidebar;
        cycle_sidebar(&mut settings);
        assert!(settings.folders && settings.sidebar_collapsed);
        cycle_sidebar(&mut settings);
        assert!(!settings.folders);
        cycle_sidebar(&mut settings);
        assert!(settings.folders && !settings.sidebar_collapsed);
        assert_eq!(settings.sidebar, width);
    }
    #[test]
    fn preview_space_accounts_for_hidden_sidebar_and_saved_width() {
        let settings = Settings {
            preview_width: 800.,
            ..Settings::default()
        };
        assert!(!decide(1280., &settings).preview);
        assert!(decide(1920., &settings).preview);
        let hidden = Settings {
            folders: false,
            ..settings
        };
        assert!(decide(1280., &hidden).preview);
    }
    #[test]
    fn hidden_panels_stay_hidden_on_widening() {
        let settings = Settings {
            folders: false,
            preview_visible: false,
            ..Settings::default()
        };
        assert!(!decide(1920., &settings).sidebar);
        assert!(!decide(1920., &settings).preview);
    }
    #[test]
    fn breadcrumbs_keep_root_and_destination_with_accessible_middle() {
        assert_eq!(breadcrumb_indices(6, 800.), (vec![0, 4, 5], vec![1, 2, 3]));
        assert_eq!(breadcrumb_indices(2, 800.), (vec![0, 1], vec![]));
        let (shown, hidden) = breadcrumb_indices(10, 1920.);
        assert_eq!(shown, vec![0, 4, 5, 6, 7, 8, 9]);
        assert_eq!(hidden, vec![1, 2, 3]);
    }
}
