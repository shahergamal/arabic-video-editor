//! The project file must never be lost or left half-written.

use engine::{Fps, Project, TextLayer};

fn sample() -> Project {
    let mut p = Project::new("تجربة", 1080, 1920, Fps::new(30, 1));
    p.texts.push(TextLayer {
        id: "t1".into(),
        text: "عرض خاص".into(),
        font_family: "Cairo".into(),
        size: 0.06,
        weight: 700,
        color: [255, 255, 255, 255],
        x: 0.5,
        y: 0.8,
        box_width: 0.9,
        shadow: true,
        outline: 0.0,
        outline_color: [0, 0, 0, 255],
        background: [0, 0, 0, 0],
        align: "center".into(),
        start: 0,
        duration: 90,
        anim_in: None,
        anim_out: None,
    });
    p
}

#[test]
fn save_and_open_gives_back_the_same_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.ave");
    let p = sample();
    p.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap(), p);
}

#[test]
fn previous_version_is_kept_as_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.ave");
    let mut p = sample();
    p.save(&path).unwrap();
    p.name = "v2".into();
    p.save(&path).unwrap();
    let backup = Project::load(&dir.path().join("p.ave.bak")).unwrap();
    assert_eq!(backup.name, "تجربة");
    assert!(!dir.path().join("p.ave.tmp").exists());
}

#[test]
fn damaged_file_is_recovered_from_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.ave");
    let p = sample();
    p.save(&path).unwrap();
    p.save(&path).unwrap();
    // Simulate a crash that cut the file in half.
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    let (recovered, from_backup) = Project::load_or_recover(&path).unwrap();
    assert!(from_backup);
    assert_eq!(recovered, p);
}
