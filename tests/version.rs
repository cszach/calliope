//! The version lives in Cargo.toml; the metainfo's newest release, which
//! GNOME Software shows and the release notes come from, must match it.

#[test]
fn newest_metainfo_release_is_the_crate_version() {
    let metainfo = include_str!("../data/io.github.cszach.Calliope.metainfo.xml");
    let newest = metainfo
        .split("<release ")
        .nth(1)
        .and_then(|release| release.split("version=\"").nth(1))
        .and_then(|rest| rest.split('"').next());
    assert_eq!(newest, Some(env!("CARGO_PKG_VERSION")));
}
