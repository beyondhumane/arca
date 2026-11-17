use crate::corpus::{Kind, CHUNK, SEED};
use crate::{Outcome, Report, Row, Speeds, LEVEL, RATED};
use std::fmt::Write;

fn mb_s(value: Option<f64>) -> String {
    match value {
        Some(v) if v >= 1000.0 => format!("{v:.0}"),
        Some(v) => format!("{v:.1}"),
        None => "-".into(),
    }
}

fn threads_word(n: usize) -> String {
    if n == 1 {
        "1 thread".into()
    } else {
        format!("{n} threads")
    }
}

fn kind_text(kind: Kind) -> &'static str {
    match kind {
        Kind::Mixed => "3/4 text-like and 1/4 random",
        Kind::Text => "text-like",
        Kind::Random => "random",
    }
}

fn build_text(report: &Report) -> &'static str {
    if report.native {
        "native codecs"
    } else {
        "pure Rust codecs"
    }
}

/// The report as a plain-text table, ending with the command that measures
/// the same thing again.
pub fn table(report: &Report) -> String {
    let o = &report.options;
    let multi = o.threads > 1;
    let mut t = String::new();
    let _ = writeln!(
        t,
        "Arca {} codec benchmark, level {}, {}",
        env!("CARGO_PKG_VERSION"),
        LEVEL,
        build_text(report)
    );
    let _ = writeln!(
        t,
        "Corpus: {} MiB made in memory from a fixed seed, {}, in {} MiB chunks",
        o.corpus_mib(),
        kind_text(o.kind),
        CHUNK >> 20
    );
    let _ = writeln!(t);
    let mut head = format!("{:<20}{:^22}", "", threads_word(1));
    if multi {
        let _ = write!(head, "  {:^22}", threads_word(o.threads));
    }
    let _ = writeln!(t, "{}", head.trim_end());
    let mut head = format!(
        "{:<10}{:>8}  {:>10}{:>12}",
        "Codec", "Ratio", "Compress", "Decompress"
    );
    if multi {
        let _ = write!(head, "  {:>10}{:>12}", "Compress", "Decompress");
    }
    let _ = writeln!(t, "{head}");
    for row in &report.rows {
        let _ = writeln!(t, "{}", line(row, multi, o.threads));
    }
    let _ = writeln!(t);
    let mut rating = format!(
        "Rating    {}: {}",
        threads_word(1),
        mb_s(report.rating(false))
    );
    if multi {
        let _ = write!(
            rating,
            "   {}: {}",
            threads_word(o.threads),
            mb_s(report.rating(true))
        );
    }
    let _ = writeln!(t, "{rating}");
    if report.stopped {
        let _ = writeln!(
            t,
            "Stopped before the end: what is missing was not measured."
        );
    }
    let _ = writeln!(
        t,
        "Speeds in MB/s of uncompressed data (1 MB = 10^6 bytes), the median of the runs."
    );
    let rated: Vec<&str> = RATED.iter().map(|c| c.label()).collect();
    let _ = writeln!(
        t,
        "The rating is the geometric mean of the {} speeds; compare it between builds with the same codecs.",
        rated.join(", ")
    );
    let _ = writeln!(
        t,
        "Memory: about {} MiB of data, plus up to {} MiB for the LZMA2 encoders.",
        o.data_mib(),
        o.codec_state_mib()
    );
    let _ = writeln!(t, "Reproduce: {}", o.command());
    t
}

fn line(row: &Row, multi: bool, threads: usize) -> String {
    let label = row.codec.label();
    match &row.outcome {
        Outcome::Unavailable(why) => format!("{label:<10}not in this build: {why}"),
        Outcome::Measured {
            ratio,
            single,
            multi: many,
        } => {
            let cells = |s: &Option<Speeds>| {
                format!(
                    "{:>10}{:>12}",
                    mb_s(s.map(|s| s.compress)),
                    mb_s(s.map(|s| s.decompress))
                )
            };
            let mut l = format!("{label:<10}{:>7.1}%  {}", ratio * 100.0, cells(single));
            if multi {
                let _ = write!(l, "  {}", cells(many));
                if row.threads != threads {
                    let _ = write!(l, "  on {}", threads_word(row.threads));
                }
            }
            l
        }
    }
}

fn number(out: &mut String, value: Option<f64>) {
    match value {
        Some(v) if v.is_finite() => {
            let _ = write!(out, "{}", (v * 10_000.0).round() / 10_000.0);
        }
        _ => out.push_str("null"),
    }
}

fn string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn speeds(out: &mut String, s: &Option<Speeds>) {
    match s {
        None => out.push_str("null"),
        Some(s) => {
            out.push_str("{\"compress\":");
            number(out, Some(s.compress));
            out.push_str(",\"decompress\":");
            number(out, Some(s.decompress));
            let _ = write!(
                out,
                ",\"compress_runs\":{},\"decompress_runs\":{}}}",
                s.compress_runs, s.decompress_runs
            );
        }
    }
}

/// The report as one JSON object. Speeds are MB/s (10^6 bytes a second) and
/// ratios compressed over uncompressed; what was not measured is null.
pub fn json(report: &Report) -> String {
    let o = &report.options;
    let mut j = String::from("{\"arca\":");
    string(&mut j, env!("CARGO_PKG_VERSION"));
    let _ = write!(
        j,
        ",\"native_codecs\":{},\"level\":{},\"threads\":{},\"seconds\":",
        report.native, LEVEL, o.threads
    );
    number(&mut j, Some(o.seconds));
    let _ = write!(
        j,
        ",\"size_mib\":{},\"chunk_mib\":{},\"seed\":\"0x{:016x}\",\"corpus\":",
        o.corpus_mib(),
        CHUNK >> 20,
        SEED
    );
    string(&mut j, kind_text(o.kind));
    let _ = write!(
        j,
        ",\"stopped\":{},\"rating\":{{\"single\":",
        report.stopped
    );
    number(&mut j, report.rating(false));
    j.push_str(",\"multi\":");
    number(&mut j, report.rating(true));
    let _ = write!(
        j,
        "}},\"memory_mib\":{{\"data\":{},\"lzma2_encoders\":{}}},\"codecs\":[",
        o.data_mib(),
        o.codec_state_mib()
    );
    for (i, row) in report.rows.iter().enumerate() {
        if i > 0 {
            j.push(',');
        }
        j.push_str("{\"codec\":");
        string(&mut j, row.codec.id());
        let _ = write!(j, ",\"threads\":{}", row.threads);
        match &row.outcome {
            Outcome::Unavailable(why) => {
                j.push_str(",\"available\":false,\"reason\":");
                string(&mut j, why);
            }
            Outcome::Measured {
                ratio,
                single,
                multi,
            } => {
                j.push_str(",\"available\":true,\"ratio\":");
                number(&mut j, Some(*ratio));
                j.push_str(",\"single\":");
                speeds(&mut j, single);
                j.push_str(",\"multi\":");
                speeds(&mut j, multi);
            }
        }
        j.push('}');
    }
    j.push_str("],\"command\":");
    string(&mut j, &o.command());
    j.push('}');
    j
}
