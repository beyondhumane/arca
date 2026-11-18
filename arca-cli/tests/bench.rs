use std::process::{Command, Output};

fn arca(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_codec_benchmark_prints_a_table_with_the_command_that_made_it() {
    let out = arca(&[
        "bench",
        "--codecs",
        "-j",
        "2",
        "--seconds",
        "0",
        "--size",
        "2",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    for codec in ["Store", "Deflate", "LZMA2", "bzip2", "Rating", "2 threads"] {
        assert!(text.contains(codec), "{codec} missing from\n{text}");
    }
    assert!(text.ends_with("Reproduce: arca bench --codecs --threads 2 --seconds 0 --size 2\n"));
}

#[test]
fn the_codec_benchmark_prints_one_json_object() {
    let out = arca(&[
        "bench",
        "--codecs",
        "-j",
        "1",
        "--seconds",
        "0",
        "--size",
        "1",
        "--json",
    ]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with("{\"arca\":") && text.ends_with("}\n"));
    assert!(text.contains("\"codec\":\"deflate\""));
    assert!(text.contains("\"multi\":null"));
    assert_eq!(text.lines().count(), 1);
}

#[test]
fn an_archive_and_the_codec_benchmark_do_not_mix() {
    assert!(!arca(&["bench", "some.zip", "--codecs"]).status.success());
    assert!(!arca(&["bench"]).status.success());
    assert!(!arca(&["bench", "some.zip", "--json"]).status.success());
    assert!(!arca(&["bench", "--codecs", "--seconds", "-1"])
        .status
        .success());
    assert!(!arca(&["bench", "--codecs", "--size", "0"]).status.success());
    for threads in ["257", "100000"] {
        let out = arca(&["bench", "--codecs", "-j", threads]);
        assert_eq!(out.status.code(), Some(2), "{threads}");
        assert!(out.stdout.is_empty(), "{threads}");
    }
}

#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_codec_benchmark_and_prints_what_it_measured() {
    use std::time::Duration;
    let child = Command::new(env!("CARGO_BIN_EXE_arca"))
        .args([
            "bench",
            "--codecs",
            "-j",
            "1",
            "--seconds",
            "30",
            "--size",
            "1",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(130));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Stopped before the end"), "{text}");
    assert!(text.contains("Reproduce: arca bench --codecs"));
}
