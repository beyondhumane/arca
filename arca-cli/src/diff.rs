use arca_compare::{compare, Options, Side};
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub(crate) fn run(
    a: &Path,
    b: &Path,
    options: Options,
    password_a: Option<&str>,
    password_b: Option<&str>,
) -> i32 {
    match differences(a, b, options, password_a, password_b) {
        Ok(false) => 0,
        Ok(true) => 1,
        Err(e) => {
            eprintln!("arca: {e}");
            2
        }
    }
}

fn differences(
    a: &Path,
    b: &Path,
    options: Options,
    password_a: Option<&str>,
    password_b: Option<&str>,
) -> arca_core::Result<bool> {
    let go = |_: usize, _: usize, _: &str| true;
    let first = Side::open(a, password_a, &go).map_err(|e| named(a, e))?;
    let second = Side::open(b, password_b, &go).map_err(|e| named(b, e))?;
    let found = compare(&first, &second, options, &go)?;
    let mut out = BufWriter::new(io::stdout().lock());
    for difference in &found {
        writeln!(
            out,
            "{} {}",
            difference.change.marker(),
            crate::printable(&difference.path).replace(['\n', '\t'], " ")
        )?;
    }
    out.flush()?;
    Ok(!found.is_empty())
}

fn named(path: &Path, error: arca_core::Error) -> arca_core::Error {
    match error {
        arca_core::Error::Io(e) => {
            arca_core::Error::Io(io::Error::new(e.kind(), format!("{}: {e}", path.display())))
        }
        other => other,
    }
}
