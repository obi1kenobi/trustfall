use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

use super::generate_rust_stub;

/// Write the given contents to a file, asserting that the file did not previously exist.
fn write_new_file(path: &Path, contents: &str) {
    match path.try_exists() {
        Ok(true) => panic!(
            "file at path unexpectedly already exists: {}",
            path.display()
        ),
        Ok(false) => std::fs::write(path, contents).expect("failed to write file"),
        Err(e) => panic!("{e}"),
    }
}

fn assert_generated_code_compiles(path: &Path) {
    let cargo_toml = r#"
[package]
name = "tests"
publish = false
version = "0.1.0"
edition = "2021"
rust-version = "1.70"

[dependencies]
trustfall = "0.5.0"

[workspace]
"#;
    let mut cargo_toml_path = PathBuf::from(path);
    cargo_toml_path.push("Cargo.toml");
    write_new_file(cargo_toml_path.as_path(), cargo_toml);

    let lib_rs = "\
mod adapter;
";
    let mut lib_rs_path = PathBuf::from(path);
    lib_rs_path.push("src");
    lib_rs_path.push("lib.rs");
    write_new_file(lib_rs_path.as_path(), lib_rs);

    let output = Command::new("cargo")
        .current_dir(path)
        .arg("check")
        .output()
        .expect("failed to execute process");

    assert!(
        output.status.success(),
        "stdout:\n{}\n\nstderr:\n{}",
        std::str::from_utf8(&output.stdout).expect("invalid utf-8"),
        std::str::from_utf8(&output.stderr).expect("invalid utf-8"),
    );

    // Clean up the new files we created, so as not to confuse the remaining tests.
    std::fs::remove_file(cargo_toml_path).expect("failed to remove Cargo.toml file");
    std::fs::remove_file(lib_rs_path).expect("failed to remove lib.rs file");

    // Clean up the "target" directory so we don't
    // have to worry about traversing those files in the rest of the tests.
    // This is just an optimization, we don't really care if it succeeds.
    let mut target = PathBuf::from(path);
    target.push("target");
    let _ = std::fs::remove_dir_all(&target);
}

fn get_relevant_files_from_dir(path: &Path) -> BTreeMap<PathBuf, PathBuf> {
    let mut base = path.to_str().expect("failed to make input path");
    base = base.strip_prefix("./").unwrap_or(base);
    let base = format!("{base}/");

    let mut dir_glob = path.to_path_buf();
    dir_glob.push("**");
    dir_glob.push("*");

    glob::glob(dir_glob.to_str().expect("failed to convert path to &str"))
        .expect("failed to list dir")
        .filter_map(|res| {
            let pathbuf = res.expect("failed to check file");
            if !pathbuf.is_file() {
                return None;
            }

            let extension = pathbuf
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or_default();
            if matches!(extension, "rs" | "graphql") {
                let mut matched_filepath = pathbuf.to_str().expect("failed to make str");
                matched_filepath = matched_filepath
                    .strip_prefix("./")
                    .unwrap_or(matched_filepath);

                let key = matched_filepath
                    .strip_prefix(&base)
                    .expect("base path was not present as prefix");
                Some((Path::new(key).to_path_buf(), pathbuf))
            } else {
                None
            }
        })
        .collect()
}

fn assert_generated_code_is_unchanged(test_dir: &Path, expected_dir: &Path) {
    let test_files = get_relevant_files_from_dir(test_dir);
    let expected_files = get_relevant_files_from_dir(expected_dir);

    assert!(!test_files.is_empty());
    assert!(!expected_files.is_empty());

    let mut unexpected_files = test_files.clone();
    unexpected_files.retain(|k, _| !expected_files.contains_key(k));

    let mut missing_expected_files = expected_files.clone();
    missing_expected_files.retain(|k, _| !test_files.contains_key(k));

    for key in test_files
        .keys()
        .filter(|k| expected_files.contains_key(*k))
    {
        let expected_filepath = &expected_files[key];
        let test_filepath = &test_files[key];
        println!("{expected_filepath:?} {test_filepath:?}");

        let expected = std::fs::read_to_string(expected_filepath).expect("failed to read file");
        let actual = std::fs::read_to_string(test_filepath).expect("failed to read file");

        similar_asserts::assert_eq!(expected, actual);
    }

    assert!(
        unexpected_files.is_empty(),
        "unexpected files found: {unexpected_files:?}"
    );
    assert!(
        missing_expected_files.is_empty(),
        "expected files were missing: {missing_expected_files:?}"
    );
}

fn test_schema(name: &str) {
    let mut test_dir = Path::new("/tmp/trustfall_stubgen/tests").to_path_buf();
    test_dir.push(name);
    let _ = std::fs::remove_dir_all(&test_dir); // it's fine if the dir didn't exist

    let mut schema_path = Path::new("./test_data").to_path_buf();
    schema_path.push(format!("{name}.graphql"));
    let schema = std::fs::read_to_string(&schema_path).expect("failed to read schema file");

    let mut test_src_dir = test_dir.clone();
    test_src_dir.push("src");
    generate_rust_stub(&schema, &test_src_dir).expect("failed to generate stub");

    let mut expected_dir = Path::new("./test_data/expected_outputs").to_path_buf();
    expected_dir.push(name);
    assert_generated_code_compiles(&test_dir);
    assert_generated_code_is_unchanged(&test_src_dir, &expected_dir);
}

#[test]
fn test_hackernews_schema() {
    test_schema("hackernews")
}

#[test]
fn test_use_reserved_rust_names_in_schema() {
    test_schema("use_reserved_rust_names_in_schema");
}
