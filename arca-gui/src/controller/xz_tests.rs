use super::*;
use crate::test_support::Room;

#[test]
fn switching_to_xz_keeps_the_file_name_and_drops_encryption() {
    let room = Room::new();
    let file = room.path("notes.txt");
    std::fs::write(&file, b"notes").unwrap();
    let mut app = AppController::new(Settings::default());
    app.state.format = Format::Zip;
    app.prepare_compress(vec![file]);
    assert_eq!(app.state.output_name, "notes.zip");
    app.state.add_password = "secret".into();
    app.state.hide_names = true;
    app.set_create_format(Format::Xz);
    assert_eq!(app.state.output_name, "notes.txt.xz");
    assert!(!app.state.hide_names);
    let Some(Job::Compress {
        out,
        format,
        password,
        hide_names,
        ..
    }) = app.compression_job()
    else {
        panic!("compression job")
    };
    assert_eq!(out, room.path("notes.txt.xz"));
    assert_eq!(format, Format::Xz);
    assert!(password.is_none());
    assert!(!hide_names);
    app.set_create_format(Format::TarXz);
    assert_eq!(app.state.output_name, "notes.tar.xz");
    app.state.output_name = "custom.tar.xz".into();
    app.set_create_format(Format::Zip);
    assert_eq!(app.state.output_name, "custom.zip");
}
