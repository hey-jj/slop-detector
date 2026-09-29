//! Packaging pins. These run against `CARGO_MANIFEST_DIR`, so inside an
//! unpacked `.crate` they read the published tree and not the repository:
//! a path that never reached the tarball fails here.

use std::path::Path;

const MANIFEST: &str = include_str!("../Cargo.toml");
const CHANGELOG: &str = include_str!("../CHANGELOG.md");

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_include_list_names_every_shipped_tree() {
    for entry in [
        "\"src/**\"",
        "\"data/**\"",
        "\"skills/**\"",
        "\"tests/**\"",
        "\"README.md\"",
        "\"CHANGELOG.md\"",
        "\"LICENSE-MIT\"",
        "\"LICENSE-APACHE\"",
    ] {
        assert!(MANIFEST.contains(entry), "include list is missing {entry}");
    }
}

#[test]
fn the_published_tree_carries_the_skill_and_the_suite() {
    // The README advertises the paired skill and the suite is the
    // executable specification of the policy, so both ship.
    for path in [
        "skills/slop-detector/SKILL.md",
        "skills/slop-detector/references/rules.md",
        "tests/quality.rs",
        "tests/residue.rs",
        "tests/bundle.rs",
        "tests/cli.rs",
        "tests/duplication.rs",
        "tests/report_layer.rs",
        "data/inbound/inbound.toml",
        "CHANGELOG.md",
    ] {
        assert!(root().join(path).is_file(), "{path} is not in the tree");
    }
}

#[test]
fn the_changelog_opens_on_the_crate_version() {
    let version = env!("CARGO_PKG_VERSION");
    let first = CHANGELOG
        .lines()
        .find(|l| l.starts_with("## "))
        .expect("the changelog has a release heading");
    assert!(
        first.contains(&format!("[{version}]")),
        "newest changelog entry is {first}, crate version is {version}"
    );
    // Every release from the first one is accounted for.
    for released in [
        "0.1.0", "0.1.1", "0.1.2", "0.1.3", "0.1.4", "0.1.5", "0.1.6",
        "0.1.7", "0.1.8",
    ] {
        assert!(
            CHANGELOG.contains(&format!("## [{released}] - ")),
            "no entry for {released}"
        );
    }
}
