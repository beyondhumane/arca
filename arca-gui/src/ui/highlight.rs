//! Colour for the viewer's text, one line at a time.
//!
//! A small tokenizer rather than a grammar engine: comments, strings,
//! numbers, the keywords of a handful of language families, and the
//! names of calls. It knows nothing that spans lines, which is the price
//! of drawing only the lines on screen. A block comment is coloured on the
//! line it opens and closes on, and plain in between.

use super::theme::{hex, Palette};
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Family {
    Shell,
    Rust,
    CLike,
    Python,
    Script,
    Data,
    Markup,
    Css,
    Markdown,
    Plain,
}

pub(crate) fn family_of(name: &str) -> Family {
    let lower = name.to_ascii_lowercase();
    let leaf = lower.rsplit('/').next().unwrap_or(&lower);
    if matches!(
        leaf,
        "makefile" | "dockerfile" | ".bashrc" | ".zshrc" | ".profile" | "cmakelists.txt"
    ) {
        return Family::Shell;
    }
    let ext = leaf.rsplit('.').next().unwrap_or("");
    match ext {
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" | "nix" => Family::Shell,
        "rs" => Family::Rust,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "java" | "cs" | "go" | "swift" | "kt"
        | "kts" | "m" | "mm" | "zig" | "d" | "scala" => Family::CLike,
        "py" | "pyi" | "pyw" | "rb" => Family::Python,
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" | "cts" | "php" | "dart" | "vue"
        | "svelte" => Family::Script,
        "json" | "jsonc" | "json5" | "toml" | "yml" | "yaml" | "ini" | "cfg" | "conf"
        | "properties" | "env" | "lock" => Family::Data,
        "html" | "htm" | "xhtml" | "xml" | "svg" | "plist" | "xaml" | "axaml" | "xsl" | "xslt"
        | "csproj" | "desktop" => Family::Markup,
        "css" | "scss" | "sass" | "less" => Family::Css,
        "md" | "markdown" | "mdx" | "rst" | "adoc" => Family::Markdown,
        _ => Family::Plain,
    }
}

/// The colours the tokens take, from the theme so that both looks read.
pub(crate) struct Ink {
    pub text: Color32,
    pub comment: Color32,
    pub string: Color32,
    pub number: Color32,
    pub keyword: Color32,
    pub call: Color32,
    pub symbol: Color32,
}

impl Ink {
    pub(crate) fn from_palette(p: &Palette) -> Self {
        Self {
            text: hex(p.text),
            comment: hex(p.muted),
            string: hex(p.success),
            number: hex(p.spark),
            keyword: hex(p.focus),
            call: hex(p.warning),
            symbol: hex(p.muted).lerp_to_gamma(hex(p.text), 0.5),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tone {
    Text,
    Comment,
    String,
    Number,
    Keyword,
    Call,
    Symbol,
}

impl Family {
    fn line_comment(self) -> &'static [&'static str] {
        match self {
            Family::Shell | Family::Python => &["#"],
            Family::Rust | Family::CLike | Family::Script => &["//"],
            Family::Data => &["#", ";", "//"],
            Family::Css | Family::Markup | Family::Markdown | Family::Plain => &[],
        }
    }

    fn block_comments(self) -> bool {
        matches!(
            self,
            Family::Rust | Family::CLike | Family::Script | Family::Css | Family::Data
        )
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Family::Shell => &[
                "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case",
                "esac", "in", "function", "local", "return", "exit", "export", "set", "unset",
                "readonly", "shift", "source", "alias", "echo", "printf", "read", "test", "true",
                "false", "break", "continue", "trap", "eval", "exec", "declare", "typeset", "let",
                "select", "time", "cd", "pwd", "with", "let", "inherit", "rec", "import",
                "setlocal", "goto", "call", "param",
            ],
            Family::Rust => &[
                "as",
                "async",
                "await",
                "break",
                "const",
                "continue",
                "crate",
                "dyn",
                "else",
                "enum",
                "extern",
                "false",
                "fn",
                "for",
                "if",
                "impl",
                "in",
                "let",
                "loop",
                "match",
                "mod",
                "move",
                "mut",
                "pub",
                "ref",
                "return",
                "self",
                "Self",
                "static",
                "struct",
                "super",
                "trait",
                "true",
                "type",
                "unsafe",
                "use",
                "where",
                "while",
                "yield",
                "macro_rules",
            ],
            Family::CLike => &[
                "auto",
                "break",
                "case",
                "char",
                "const",
                "continue",
                "default",
                "do",
                "double",
                "else",
                "enum",
                "extern",
                "float",
                "for",
                "goto",
                "if",
                "inline",
                "int",
                "long",
                "register",
                "return",
                "short",
                "signed",
                "sizeof",
                "static",
                "struct",
                "switch",
                "typedef",
                "union",
                "unsigned",
                "void",
                "volatile",
                "while",
                "class",
                "namespace",
                "new",
                "delete",
                "template",
                "typename",
                "this",
                "public",
                "private",
                "protected",
                "virtual",
                "override",
                "using",
                "try",
                "catch",
                "throw",
                "throws",
                "bool",
                "true",
                "false",
                "nullptr",
                "null",
                "var",
                "val",
                "fun",
                "func",
                "package",
                "import",
                "interface",
                "final",
                "finally",
                "defer",
                "go",
                "chan",
                "map",
                "range",
                "select",
                "string",
                "byte",
                "let",
                "guard",
                "where",
                "extends",
                "implements",
                "abstract",
                "readonly",
                "async",
                "await",
                "object",
                "when",
                "is",
                "in",
                "out",
                "ref",
                "foreach",
                "yield",
                "lock",
                "operator",
                "explicit",
                "implicit",
                "constexpr",
                "noexcept",
                "mutable",
                "friend",
                "decltype",
                "nullable",
                "instanceof",
                "super",
                "self",
                "mut",
                "pub",
                "comptime",
            ],
            Family::Python => &[
                "and",
                "as",
                "assert",
                "async",
                "await",
                "break",
                "class",
                "continue",
                "def",
                "del",
                "elif",
                "else",
                "except",
                "finally",
                "for",
                "from",
                "global",
                "if",
                "import",
                "in",
                "is",
                "lambda",
                "nonlocal",
                "not",
                "or",
                "pass",
                "raise",
                "return",
                "try",
                "while",
                "with",
                "yield",
                "True",
                "False",
                "None",
                "self",
                "print",
                "match",
                "case",
                "end",
                "do",
                "then",
                "unless",
                "module",
                "require",
                "nil",
                "begin",
                "rescue",
                "ensure",
                "elsif",
                "puts",
                "attr_accessor",
            ],
            Family::Script => &[
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "finally",
                "for",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "return",
                "super",
                "switch",
                "this",
                "throw",
                "try",
                "typeof",
                "var",
                "void",
                "while",
                "with",
                "yield",
                "async",
                "await",
                "of",
                "static",
                "get",
                "set",
                "null",
                "undefined",
                "true",
                "false",
                "interface",
                "type",
                "enum",
                "implements",
                "readonly",
                "declare",
                "namespace",
                "abstract",
                "public",
                "private",
                "protected",
                "as",
                "is",
                "keyof",
                "from",
                "echo",
                "fn",
                "use",
                "require",
                "final",
                "late",
                "mixin",
                "override",
            ],
            Family::Data => &["true", "false", "null", "yes", "no", "on", "off", "nil"],
            Family::Css => &[
                "important",
                "import",
                "media",
                "keyframes",
                "font-face",
                "supports",
                "charset",
                "namespace",
                "page",
                "layer",
                "container",
                "mixin",
                "include",
                "extend",
                "if",
                "else",
                "each",
                "for",
                "while",
                "function",
                "return",
                "use",
                "forward",
            ],
            Family::Markup | Family::Markdown | Family::Plain => &[],
        }
    }
}

fn is_word_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Splits one line into (text, tone) runs.
fn tokens(line: &str, family: Family) -> Vec<(usize, usize, Tone)> {
    let bytes = line.as_bytes();
    let n = bytes.len();
    let mut out: Vec<(usize, usize, Tone)> = Vec::new();
    let mut i = 0;
    let push = |out: &mut Vec<(usize, usize, Tone)>, a: usize, b: usize, tone: Tone| {
        if a >= b {
            return;
        }
        if let Some(last) = out.last_mut() {
            if last.2 == tone && last.1 == a {
                last.1 = b;
                return;
            }
        }
        out.push((a, b, tone));
    };
    if family == Family::Markdown {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            push(&mut out, 0, n, Tone::Keyword);
            return out;
        }
        if trimmed.starts_with('>') {
            push(&mut out, 0, n, Tone::Comment);
            return out;
        }
    }
    let rest = line;
    let comment_at = |i: usize| -> bool {
        family
            .line_comment()
            .iter()
            .any(|marker| rest[i..].starts_with(marker))
    };
    while i < n {
        let c = rest[i..].chars().next().unwrap_or(' ');
        let len = c.len_utf8();
        // Comments to the end of the line.
        if comment_at(i) && (family != Family::Shell || !rest[..i].ends_with('$')) {
            push(&mut out, i, n, Tone::Comment);
            break;
        }
        if family.block_comments() && rest[i..].starts_with("/*") {
            let end = rest[i + 2..]
                .find("*/")
                .map(|at| i + 2 + at + 2)
                .unwrap_or(n);
            push(&mut out, i, end, Tone::Comment);
            i = end;
            continue;
        }
        if family == Family::Markup && rest[i..].starts_with("<!--") {
            let end = rest[i + 4..]
                .find("-->")
                .map(|at| i + 4 + at + 3)
                .unwrap_or(n);
            push(&mut out, i, end, Tone::Comment);
            i = end;
            continue;
        }
        // Strings, with backslash escapes.
        if c == '"'
            || c == '\''
            || (c == '`' && matches!(family, Family::Script | Family::Shell | Family::Markdown))
        {
            let mut j = i + 1;
            while j < n {
                let d = bytes[j];
                if d == b'\\' {
                    j += 2;
                    continue;
                }
                if d == c as u8 {
                    j += 1;
                    break;
                }
                j += 1;
            }
            let end = j.min(n);
            push(&mut out, i, end, Tone::String);
            i = end;
            continue;
        }
        // Numbers: 0x1F, 1_000, 3.14, 10px, 1e9.
        if c.is_ascii_digit() {
            let mut j = i + 1;
            while j < n {
                let d = bytes[j] as char;
                if d.is_ascii_alphanumeric() || d == '_' || d == '.' {
                    j += 1;
                } else {
                    break;
                }
            }
            push(&mut out, i, j, Tone::Number);
            i = j;
            continue;
        }
        // Shell and script variables.
        if c == '$' && matches!(family, Family::Shell | Family::Script | Family::Css) {
            let mut j = i + 1;
            if rest[j..].starts_with('{') {
                j = rest[j..].find('}').map(|at| j + at + 1).unwrap_or(n);
            } else {
                while j < n && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if j == i + 1 && j < n {
                    j += 1;
                }
            }
            push(&mut out, i, j, Tone::Keyword);
            i = j;
            continue;
        }
        // Markup tags and CSS properties and data keys.
        if family == Family::Markup && c == '<' {
            let mut j = i + 1;
            if rest[j..].starts_with('/') {
                j += 1;
            }
            while j < n
                && (bytes[j].is_ascii_alphanumeric()
                    || matches!(bytes[j], b'-' | b':' | b'_' | b'!' | b'?'))
            {
                j += 1;
            }
            push(&mut out, i, j, Tone::Keyword);
            i = j;
            continue;
        }
        if is_word_start(c) {
            let mut j = i + len;
            while j < n {
                let d = rest[j..].chars().next().unwrap_or(' ');
                if is_word(d) || (family == Family::Css && d == '-') {
                    j += d.len_utf8();
                } else {
                    break;
                }
            }
            let word = &rest[i..j];
            let after = rest[j..].trim_start();
            let key_here = matches!(family, Family::Data | Family::Css)
                && (after.starts_with(':') || after.starts_with('='))
                && !rest[..i].contains(|d: char| !d.is_whitespace() && d != '-' && d != '"');
            let call_here =
                (after.starts_with('(') && family != Family::Data && family != Family::Plain)
                    || (family == Family::Markup && after.starts_with('='));
            let tone = if family.keywords().contains(&word) || key_here {
                Tone::Keyword
            } else if call_here {
                Tone::Call
            } else {
                Tone::Text
            };
            push(&mut out, i, j, tone);
            i = j;
            continue;
        }
        if c.is_ascii_punctuation() && family != Family::Plain {
            push(&mut out, i, i + len, Tone::Symbol);
            i += len;
            continue;
        }
        push(&mut out, i, i + len, Tone::Text);
        i += len;
    }
    out
}

/// One line laid out in colour.
pub(crate) fn line_job(
    line: &str,
    family: Family,
    ink: &Ink,
    font: FontId,
    wrap: Option<f32>,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap.unwrap_or(f32::INFINITY);
    let format = |tone: Tone| {
        let color = match tone {
            Tone::Text => ink.text,
            Tone::Comment => ink.comment,
            Tone::String => ink.string,
            Tone::Number => ink.number,
            Tone::Keyword => ink.keyword,
            Tone::Call => ink.call,
            Tone::Symbol => ink.symbol,
        };
        TextFormat {
            font_id: font.clone(),
            color,
            italics: tone == Tone::Comment,
            ..Default::default()
        }
    };
    if line.is_empty() {
        job.append(" ", 0.0, format(Tone::Text));
        return job;
    }
    if family == Family::Plain {
        job.append(line, 0.0, format(Tone::Text));
        return job;
    }
    for (a, b, tone) in tokens(line, family) {
        job.append(&line[a..b], 0.0, format(tone));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tones(line: &str, family: Family) -> Vec<(&str, Tone)> {
        tokens(line, family)
            .into_iter()
            .map(|(a, b, tone)| (&line[a..b], tone))
            .collect()
    }

    #[test]
    fn families_come_from_the_extension() {
        assert_eq!(family_of("build.sh"), Family::Shell);
        assert_eq!(family_of("dir/Makefile"), Family::Shell);
        assert_eq!(family_of("main.rs"), Family::Rust);
        assert_eq!(family_of("a.hpp"), Family::CLike);
        assert_eq!(family_of("x.py"), Family::Python);
        assert_eq!(family_of("x.tsx"), Family::Script);
        assert_eq!(family_of("Cargo.toml"), Family::Data);
        assert_eq!(family_of("page.html"), Family::Markup);
        assert_eq!(family_of("site.css"), Family::Css);
        assert_eq!(family_of("README.md"), Family::Markdown);
        assert_eq!(family_of("notes.txt"), Family::Plain);
        assert_eq!(family_of("out.log"), Family::Plain);
    }

    #[test]
    fn shell_line_splits_into_keyword_variable_string_and_comment() {
        let got = tones(r#"if [[ "$MODE" != 1 ]]; then # note"#, Family::Shell);
        assert!(got.contains(&("if", Tone::Keyword)));
        assert!(got.contains(&("\"$MODE\"", Tone::String)));
        assert!(got.contains(&("1", Tone::Number)));
        assert!(got.contains(&("then", Tone::Keyword)));
        assert!(got.contains(&("# note", Tone::Comment)));
        let got = tones("MODE=${1:-manifest}", Family::Shell);
        assert!(got.contains(&("${1:-manifest}", Tone::Keyword)));
    }

    #[test]
    fn rust_calls_and_strings() {
        let got = tones(r#"let x = foo("a\"b"); // hi"#, Family::Rust);
        assert!(got.contains(&("let", Tone::Keyword)));
        assert!(got.contains(&("foo", Tone::Call)));
        assert!(got.contains(&("\"a\\\"b\"", Tone::String)));
        assert!(got.contains(&("// hi", Tone::Comment)));
    }

    #[test]
    fn data_keys_and_markup_tags() {
        let got = tones("name = \"arca\"", Family::Data);
        assert_eq!(got[0], ("name", Tone::Keyword));
        let got = tones("<div class=\"x\">", Family::Markup);
        assert_eq!(got[0], ("<div", Tone::Keyword));
        assert!(got.contains(&("class", Tone::Call)));
        assert!(got.contains(&("\"x\"", Tone::String)));
    }

    #[test]
    fn plain_text_is_one_run_and_empty_lines_still_take_a_row() {
        let ink = Ink {
            text: Color32::WHITE,
            comment: Color32::GRAY,
            string: Color32::GREEN,
            number: Color32::YELLOW,
            keyword: Color32::BLUE,
            call: Color32::GOLD,
            symbol: Color32::LIGHT_GRAY,
        };
        let job = line_job("a b c", Family::Plain, &ink, FontId::monospace(12.0), None);
        assert_eq!(job.sections.len(), 1);
        let job = line_job("", Family::Rust, &ink, FontId::monospace(12.0), None);
        assert_eq!(job.text, " ");
        let unterminated = tones("\"open", Family::Rust);
        assert_eq!(unterminated, vec![("\"open", Tone::String)]);
    }
}
