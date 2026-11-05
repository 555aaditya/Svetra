/// The npm wrapper downloads the release whose tag is its own version, so the two must match.
#[test]
fn npm_package_version_matches_the_crate() {
    let pkg: serde_json::Value =
        serde_json::from_str(include_str!("../npm/package.json")).expect("npm/package.json");
    assert_eq!(pkg["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(pkg["name"], "svetra");
    assert_eq!(pkg["bin"]["svetra"], "bin/svetra.js");
}
