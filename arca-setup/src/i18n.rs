#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Es,
}

impl Lang {
    pub fn from_code(code: &str) -> Option<Lang> {
        match code.get(..2)?.to_ascii_lowercase().as_str() {
            "en" => Some(Lang::En),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }

    pub fn from_system() -> Lang {
        sys_locale::get_locale()
            .and_then(|locale| Lang::from_code(&locale))
            .unwrap_or(Lang::En)
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::Es => &ES,
        }
    }
}

pub struct Strings {
    pub window_title: &'static str,
    pub uninstall_window_title: &'static str,
    pub welcome_title: &'static str,
    pub welcome_body: &'static str,
    pub install: &'static str,
    pub customize: &'static str,
    pub back: &'static str,
    pub options_title: &'static str,
    pub options_body: &'static str,
    pub option_menu: &'static str,
    pub option_menu_hint: &'static str,
    pub option_assoc: &'static str,
    pub option_assoc_hint: &'static str,
    pub option_path: &'static str,
    pub option_path_hint: &'static str,
    pub location: &'static str,
    pub installing_title: &'static str,
    pub installing_body: &'static str,
    pub steps: [&'static str; 4],
    pub done_title: &'static str,
    pub done_body: &'static str,
    pub start: &'static str,
    pub whats_new: &'static str,
    pub confirm_title: &'static str,
    pub confirm_body: &'static str,
    pub uninstall: &'static str,
    pub cancel: &'static str,
    pub uninstalling_title: &'static str,
    pub uninstalling_body: &'static str,
    pub uninstall_steps: [&'static str; 4],
    pub uninstalled_title: &'static str,
    pub uninstalled_body: &'static str,
    pub close: &'static str,
    pub retry: &'static str,
    pub failed_install: &'static str,
    pub failed_uninstall: &'static str,
    pub failed_hint: &'static str,
}

static ES: Strings = Strings {
    window_title: "Instalador de Arca",
    uninstall_window_title: "Desinstalar Arca",
    welcome_title: "Bienvenido a ARCA",
    welcome_body: "El archivador moderno para un mundo\nen movimiento. Comprime, protege\ny preserva tus datos más importantes.",
    install: "Instalar ARCA",
    customize: "Personalizar",
    back: "Volver",
    options_title: "Personalizar",
    options_body: "Elige cómo se integra Arca con tu equipo.",
    option_menu: "Menú contextual del Explorador",
    option_menu_hint: "Comprimir y extraer con el botón derecho",
    option_assoc: "Abrir .zip, .tar, .gz y .tgz con Arca",
    option_assoc_hint: "Arca aparece en Aplicaciones predeterminadas",
    option_path: "Añadir el comando arca al PATH",
    option_path_hint: "Usar Arca desde la terminal",
    location: "Carpeta de instalación",
    installing_title: "Instalando ARCA…",
    installing_body: "Optimizando componentes. Casi listo.",
    steps: [
        "Extrayendo archivos…",
        "Instalando componentes…",
        "Configurando el entorno…",
        "Finalizando…",
    ],
    done_title: "¡Listo!",
    done_body: "ARCA se ha instalado correctamente.",
    start: "Comenzar a usar ARCA",
    whats_new: "Explorar novedades",
    confirm_title: "¿Desinstalar ARCA?",
    confirm_body: "Se quitarán Arca, su menú contextual\ny sus asociaciones de archivo.\nTus archivos comprimidos no se tocan.",
    uninstall: "Desinstalar",
    cancel: "Cancelar",
    uninstalling_title: "Desinstalando ARCA…",
    uninstalling_body: "Quitando Arca de este equipo.",
    uninstall_steps: [
        "Quitando el menú contextual…",
        "Quitando asociaciones y PATH…",
        "Eliminando archivos…",
        "Finalizando…",
    ],
    uninstalled_title: "¡Hecho!",
    uninstalled_body: "ARCA se ha desinstalado de este equipo.",
    close: "Cerrar",
    retry: "Reintentar",
    failed_install: "No se pudo instalar",
    failed_uninstall: "No se pudo desinstalar",
    failed_hint: "Cierra Arca si está abierta y vuelve a intentarlo.",
};

static EN: Strings = Strings {
    window_title: "Arca Setup",
    uninstall_window_title: "Uninstall Arca",
    welcome_title: "Welcome to ARCA",
    welcome_body: "The modern archiver for a world\nin motion. Compress, protect and\npreserve the data that matters most.",
    install: "Install ARCA",
    customize: "Customize",
    back: "Back",
    options_title: "Customize",
    options_body: "Choose how Arca fits into your computer.",
    option_menu: "Explorer context menu",
    option_menu_hint: "Compress and extract with a right click",
    option_assoc: "Open .zip, .tar, .gz and .tgz with Arca",
    option_assoc_hint: "Arca shows up in Default apps",
    option_path: "Add the arca command to PATH",
    option_path_hint: "Use Arca from the terminal",
    location: "Install location",
    installing_title: "Installing ARCA…",
    installing_body: "Optimizing components. Almost there.",
    steps: [
        "Extracting files…",
        "Installing components…",
        "Setting up the environment…",
        "Finishing…",
    ],
    done_title: "All set!",
    done_body: "ARCA was installed successfully.",
    start: "Start using ARCA",
    whats_new: "See what's new",
    confirm_title: "Uninstall ARCA?",
    confirm_body: "Arca, its context menu and its file\nassociations will be removed.\nYour archives are left untouched.",
    uninstall: "Uninstall",
    cancel: "Cancel",
    uninstalling_title: "Uninstalling ARCA…",
    uninstalling_body: "Removing Arca from this computer.",
    uninstall_steps: [
        "Removing the context menu…",
        "Removing associations and PATH…",
        "Deleting files…",
        "Finishing…",
    ],
    uninstalled_title: "Done!",
    uninstalled_body: "ARCA was uninstalled from this computer.",
    close: "Close",
    retry: "Try again",
    failed_install: "Could not install",
    failed_uninstall: "Could not uninstall",
    failed_hint: "Close Arca if it is open and try again.",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_read_locales() {
        assert_eq!(Lang::from_code("es-ES"), Some(Lang::Es));
        assert_eq!(Lang::from_code("EN_us"), Some(Lang::En));
        assert_eq!(Lang::from_code("fr-FR"), None);
        assert_eq!(Lang::from_code(""), None);
        assert_eq!(Lang::from_code("e"), None);
    }

    #[test]
    fn both_languages_fill_every_string() {
        for lang in [Lang::En, Lang::Es] {
            let s = lang.strings();
            let all = [
                s.window_title,
                s.uninstall_window_title,
                s.welcome_title,
                s.welcome_body,
                s.install,
                s.customize,
                s.back,
                s.options_title,
                s.options_body,
                s.option_menu,
                s.option_menu_hint,
                s.option_assoc,
                s.option_assoc_hint,
                s.option_path,
                s.option_path_hint,
                s.location,
                s.installing_title,
                s.installing_body,
                s.done_title,
                s.done_body,
                s.start,
                s.whats_new,
                s.confirm_title,
                s.confirm_body,
                s.uninstall,
                s.cancel,
                s.uninstalling_title,
                s.uninstalling_body,
                s.uninstalled_title,
                s.uninstalled_body,
                s.close,
                s.retry,
                s.failed_install,
                s.failed_uninstall,
                s.failed_hint,
            ];
            assert!(all.iter().all(|text| !text.is_empty()));
            assert!(s.steps.iter().all(|text| !text.is_empty()));
            assert!(s.uninstall_steps.iter().all(|text| !text.is_empty()));
        }
    }
}
