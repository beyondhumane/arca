use crate::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub sidebar: bool,
    pub rail: bool,
    pub preview: bool,
}

pub(crate) fn decide(width: f32, settings: &Settings) -> Layout {
    let sidebar = settings.folders;
    // The rail is nobody's choice: the sidebar shrinks to icons when the
    // folders would otherwise be left with too little room beside it.
    let preview = if settings.preview_visible {
        settings.preview_width
    } else {
        0.
    };
    let rail = sidebar && width - settings.sidebar - preview < 480.;
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

pub(crate) fn toggle_sidebar(settings: &mut Settings) {
    settings.folders = !settings.folders;
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
            decide(700., &settings),
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
    }
    #[test]
    fn sidebar_toggle_shows_and_hides_without_losing_width() {
        let mut settings = Settings::default();
        let width = settings.sidebar;
        toggle_sidebar(&mut settings);
        assert!(!settings.folders);
        toggle_sidebar(&mut settings);
        assert!(settings.folders);
        assert_eq!(settings.sidebar, width);
        // The rail is a matter of room, not of preference: it appears when
        // the folders would be squeezed, and a hidden preview gives room back.
        assert!(decide(640., &settings).rail);
        assert!(!decide(1280., &settings).rail);
        let squeezed = Settings {
            preview_width: 500.,
            ..Settings::default()
        };
        assert!(decide(1024., &squeezed).rail);
        assert!(decide(1024., &squeezed).preview);
        let no_preview = Settings {
            preview_visible: false,
            ..squeezed
        };
        assert!(!decide(1024., &no_preview).rail);
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
