use super::io::{collect_rar_sources, RarSourceLimits};
use crate::test_support::Room;
use arca_core::Error;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

fn tree(room: &Room) -> std::path::PathBuf {
    let source = room.path("source");
    fs::create_dir_all(source.join("dir")).unwrap();
    fs::write(source.join("a.txt"), b"aaaa").unwrap();
    fs::write(source.join("dir").join("b.txt"), b"bbbbbbbb").unwrap();
    source
}

#[test]
fn rar_source_walk_lists_directories_as_members_in_order() {
    let room = Room::new();
    let source = tree(&room);
    let sources =
        collect_rar_sources(&[source], &RarSourceLimits::WRITER, &|_, _, _| true).unwrap();
    let names: Vec<_> = sources.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["source/", "source/a.txt", "source/dir/", "source/dir/b.txt"]
    );
}

#[test]
fn rar_source_walk_stops_at_each_ceiling_before_growing_the_list() {
    let room = Room::new();
    let source = tree(&room);
    let writer = RarSourceLimits::WRITER;
    let cases = [
        RarSourceLimits {
            entries: 3,
            ..writer
        },
        RarSourceLimits {
            name_bytes: "source/dir/".len(),
            ..writer
        },
        RarSourceLimits {
            member_bytes: 7,
            ..writer
        },
        RarSourceLimits {
            total_bytes: 11,
            ..writer
        },
    ];
    for limits in cases {
        let result = collect_rar_sources(std::slice::from_ref(&source), &limits, &|_, _, _| true);
        assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    }
    assert!(collect_rar_sources(&[source], &writer, &|_, _, _| true).is_ok());
}

#[test]
fn rar_source_walk_is_cancelled_between_members() {
    let room = Room::new();
    let source = tree(&room);
    for stop_after in 0..4 {
        let seen = AtomicUsize::new(0);
        let result = collect_rar_sources(
            std::slice::from_ref(&source),
            &RarSourceLimits::WRITER,
            &|_, _, _| seen.fetch_add(1, Ordering::SeqCst) < stop_after,
        );
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(seen.load(Ordering::SeqCst), stop_after + 1);
    }
}

#[test]
fn rar_source_walk_stops_enumerating_a_wide_directory_at_the_ceiling() {
    let room = Room::new();
    let source = room.path("wide");
    fs::create_dir_all(&source).unwrap();
    for i in 0..64 {
        fs::write(source.join(format!("{i:03}.txt")), b"x").unwrap();
    }
    let limits = RarSourceLimits {
        entries: 5,
        ..RarSourceLimits::WRITER
    };
    let seen = AtomicUsize::new(0);
    let result = collect_rar_sources(&[source], &limits, &|_, queued, _| {
        seen.fetch_add(1, Ordering::SeqCst);
        assert!(queued <= limits.entries + 1, "queued {queued}");
        true
    });
    assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    assert!(seen.load(Ordering::SeqCst) <= limits.entries + 1);
}

#[test]
fn rar_source_walk_handles_a_deep_tree_without_recursion() {
    let room = Room::new();
    let source = room.path("deep");
    let mut dir = source.clone();
    for _ in 0..400 {
        dir.push("d");
    }
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("leaf.txt"), b"leaf").unwrap();
    let sources =
        collect_rar_sources(&[source], &RarSourceLimits::WRITER, &|_, _, _| true).unwrap();
    assert_eq!(sources.len(), 402);
    assert!(sources.last().unwrap().name.ends_with("/d/leaf.txt"));
}
