//! Asset copies must remain below the selected output directory.

use std::path::Path;

use tempfile::TempDir;
use wcl_wdoc::{BuildError, build, markdown};

fn document(root: &Path, body: &str) -> std::path::PathBuf {
    let path = root.join("site.wcl");
    std::fs::write(&path, format!("import <wdoc.wcl>\n{body}\n")).unwrap();
    path
}

fn assert_invalid_destination(result: Result<usize, BuildError>) {
    assert!(matches!(result, Err(BuildError::Io(error, _))
        if error.kind() == std::io::ErrorKind::InvalidInput));
}

#[test]
fn markdown_file_destination_stays_under_output_root() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/payload.txt"), "payload").unwrap();
    let out = tmp.path().join("out");
    let src = document(
        tmp.path(),
        "page index { file \"src/payload.txt\" { dir = \"../escaped\" } }",
    );
    assert_invalid_destination(markdown(&src, &out, None));
    assert!(!tmp.path().join("escaped").exists());
    let src = document(
        tmp.path(),
        "page index { file \"src/payload.txt\" { dir = \"downloads/nested\" } }",
    );
    assert!(markdown(&src, &out, None).is_ok());
    assert_eq!(
        std::fs::read_to_string(out.join("downloads/nested/payload.txt")).unwrap(),
        "payload"
    );
}

#[test]
fn site_assets_copy_nested_files() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("assets/nested")).unwrap();
    std::fs::write(tmp.path().join("assets/nested/payload.txt"), "payload").unwrap();
    let out = tmp.path().join("out");
    let src = document(
        tmp.path(),
        "site { assets = [\"assets\"] }\npage index { h1 \"Hi\" }",
    );
    assert!(build(&src, &out, None).is_ok());
    assert_eq!(
        std::fs::read_to_string(out.join("assets/nested/payload.txt")).unwrap(),
        "payload"
    );
}

#[cfg(unix)]
#[test]
fn file_destination_rejects_shared_asset_symlink_before_bundled_writes() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("payload.txt"), "payload").unwrap();
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    let out = tmp.path().join("out");
    std::fs::create_dir(&out).unwrap();
    symlink(&outside, out.join("_wdoc")).unwrap();
    let src = document(
        tmp.path(),
        "page index { file \"payload.txt\" { dir = \"_wdoc\" } }",
    );
    assert_invalid_destination(build(&src, &out, None));
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn file_destination_stays_under_output_root() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/payload.txt"), "payload").unwrap();
    let out = tmp.path().join("out");
    let src = document(
        tmp.path(),
        "page index { file \"src/payload.txt\" { dir = \"../escaped\" } }",
    );
    assert_invalid_destination(build(&src, &out, None));
    assert!(!tmp.path().join("escaped").exists());

    let src = document(
        tmp.path(),
        "page index { file \"src/payload.txt\" { dir = \"downloads/nested\" } }",
    );
    assert!(build(&src, &out, None).is_ok());
    assert_eq!(
        std::fs::read_to_string(out.join("downloads/nested/payload.txt")).unwrap(),
        "payload"
    );
}

#[test]
fn site_assets_reject_parent_and_absolute_destinations() {
    let tmp = TempDir::new().unwrap();
    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    let outside = tmp.path().join("assets");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("payload.txt"), "original").unwrap();
    for entry in [
        "../assets".to_string(),
        outside.to_str().unwrap().to_string(),
    ] {
        let quoted = serde_json::to_string(&entry).unwrap();
        let src = document(
            &docs,
            &format!("site {{ assets = [{quoted}] }}\npage index {{ h1 \"Hi\" }}"),
        );
        assert_invalid_destination(build(&src, &tmp.path().join("out"), None));
        assert_eq!(
            std::fs::read_to_string(outside.join("payload.txt")).unwrap(),
            "original"
        );
    }
}

#[cfg(unix)]
#[test]
fn asset_destinations_reject_existing_directory_and_file_symlinks() {
    use std::os::unix::fs::symlink;

    for site_assets in [false, true] {
        for link_directory in [false, true] {
            let tmp = TempDir::new().unwrap();
            let source = tmp.path().join("assets");
            std::fs::create_dir_all(source.join("nested")).unwrap();
            std::fs::write(source.join("nested/payload.txt"), "replacement").unwrap();
            let outside = tmp.path().join("outside");
            std::fs::create_dir(&outside).unwrap();
            std::fs::write(outside.join("payload.txt"), "original").unwrap();
            let out = tmp.path().join("out");
            std::fs::create_dir_all(out.join("assets")).unwrap();
            if link_directory {
                symlink(&outside, out.join("assets/nested")).unwrap();
            } else {
                std::fs::create_dir(out.join("assets/nested")).unwrap();
                symlink(
                    outside.join("payload.txt"),
                    out.join("assets/nested/payload.txt"),
                )
                .unwrap();
            }
            let body = if site_assets {
                "site { assets = [\"assets\"] }\npage index { h1 \"Hi\" }"
            } else {
                "page index { file \"assets/nested/payload.txt\" { dir = \"assets/nested\" } }"
            };
            let src = document(tmp.path(), body);
            assert_invalid_destination(build(&src, &out, None));
            assert_eq!(
                std::fs::read_to_string(outside.join("payload.txt")).unwrap(),
                "original"
            );
        }
    }
}
