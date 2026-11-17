//! What the self-extractor says, in the language the system is set to, the
//! same three the window speaks.

pub(crate) struct Words {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) title: &'static str,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) pick: &'static str,
    pub(crate) prompt: &'static str,
    pub(crate) done: &'static str,
    pub(crate) failed: &'static str,
    pub(crate) encrypted: &'static str,
    pub(crate) usage: &'static str,
}

const EN: Words = Words {
    title: "Arca",
    pick: "Extract {name} to",
    prompt: "Extract to [{dir}]: ",
    done: "Extracted {n} files to {dir}",
    failed: "Could not extract: {error}",
    encrypted: "this archive is encrypted, which a self-extractor cannot open yet",
    usage: "Usage: [-o FOLDER] [--console]\n  -o, --output FOLDER  extract there without asking\n  --console            ask and report in the console",
};

const ES: Words = Words {
    title: "Arca",
    pick: "Extraer {name} en",
    prompt: "Extraer en [{dir}]: ",
    done: "Se extrajeron {n} ficheros en {dir}",
    failed: "No se pudo extraer: {error}",
    encrypted: "este archivo está cifrado, y un autoextraíble aún no sabe abrirlo",
    usage: "Uso: [-o CARPETA] [--console]\n  -o, --output CARPETA  extrae ahí sin preguntar\n  --console             pregunta e informa en la consola",
};

const UK: Words = Words {
    title: "Arca",
    pick: "Розпакувати {name} до",
    prompt: "Розпакувати до [{dir}]: ",
    done: "Розпаковано файлів: {n} до {dir}",
    failed: "Не вдалося розпакувати: {error}",
    encrypted: "цей архів зашифровано, а самозапакований архів поки не вміє його відкривати",
    usage: "Використання: [-o ПАПКА] [--console]\n  -o, --output ПАПКА  розпакувати туди без запитань\n  --console           питати й повідомляти в консолі",
};

fn of(locale: &str) -> &'static Words {
    let lower = locale.to_ascii_lowercase();
    if lower.starts_with("es") {
        &ES
    } else if lower.starts_with("uk") {
        &UK
    } else {
        &EN
    }
}

pub(crate) fn pick() -> &'static Words {
    of(&sys_locale::get_locale().unwrap_or_default())
}

pub(crate) fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut text = template.to_string();
    for (key, value) in pairs {
        text = text.replace(&format!("{{{key}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_follows_the_locale() {
        assert_eq!(of("es-ES").done, ES.done);
        assert_eq!(of("uk-UA").done, UK.done);
        assert_eq!(of("en-GB").done, EN.done);
        assert_eq!(of("").done, EN.done);
        for words in [&EN, &ES, &UK] {
            for text in [words.pick, words.prompt, words.done, words.failed] {
                assert!(text.contains('{'), "{text}");
            }
        }
        assert_eq!(fill("{n} to {dir}", &[("n", "3"), ("dir", "x")]), "3 to x");
    }
}
