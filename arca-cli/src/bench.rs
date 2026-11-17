use arca_bench::{Event, Options, Phase};
use arca_core::Result;
use std::io::{self, IsTerminal, Write};

pub(crate) fn parse_seconds(s: &str) -> std::result::Result<f64, String> {
    match s.parse::<f64>() {
        Ok(v) if v.is_finite() && (0.0..=3600.0).contains(&v) => Ok(v),
        _ => Err(format!("'{s}' is not a number of seconds from 0 to 3600")),
    }
}

/// The codec benchmark: every codec on generated data, on one thread and on
/// `threads`. Ctrl-C stops it between runs and prints what was measured,
/// then exits with 130 as an interrupted program does.
pub(crate) fn codecs(threads: usize, seconds: f64, size: usize, json: bool) -> Result<()> {
    let stop = crate::rar::interruption()?;
    let mut options = Options::new(crate::resolve_threads(threads));
    options.seconds = seconds;
    options.size_mib = size;
    let live = io::stderr().is_terminal();
    let events = |event: Event<'_>| {
        let Event::Progress(p) = event else {
            return;
        };
        if !live {
            return;
        }
        let what = match (p.phase, p.codec) {
            (Phase::Generating, _) | (_, None) => "generating the data".to_string(),
            (phase, Some(codec)) => format!(
                "{} on {} thread{}, {}",
                codec.label(),
                p.threads,
                if p.threads == 1 { "" } else { "s" },
                if phase == Phase::Compressing {
                    "compressing"
                } else {
                    "decompressing"
                }
            ),
        };
        eprint!("\r[{:>2}/{}] {:<60}", p.step + 1, p.steps, what);
        let _ = io::stderr().flush();
    };
    let report = arca_bench::run(&options, &stop, &events);
    if live {
        eprint!("\r{:<80}\r", "");
    }
    let report = report?;
    let text = if json {
        arca_bench::json(&report) + "\n"
    } else {
        arca_bench::table(&report)
    };
    let mut out = io::stdout().lock();
    out.write_all(text.as_bytes())?;
    out.flush()?;
    if report.stopped {
        drop(out);
        std::process::exit(130);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_must_be_a_sensible_number() {
        assert_eq!(parse_seconds("0"), Ok(0.0));
        assert_eq!(parse_seconds("2.5"), Ok(2.5));
        for bad in ["-1", "nan", "inf", "", "1e9", "x"] {
            assert!(parse_seconds(bad).is_err(), "{bad}");
        }
    }
}
