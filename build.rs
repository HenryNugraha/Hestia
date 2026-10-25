use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src/asset/icon.ico");
    println!("cargo:rerun-if-changed=src/integrations/gamebanana_matcher");
    println!("cargo:rustc-check-cfg=cfg(gamebanana_matcher_fixtures)");
    let fixtures = Path::new("src/integrations/gamebanana_matcher/tests/fixtures");
    if fixtures.join("model-parity-fixtures.json").is_file()
        && fixtures.join("parity-score-fixtures.json").is_file()
    {
        println!("cargo:rustc-cfg=gamebanana_matcher_fixtures");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/asset/icon.ico");
        res.compile().unwrap();
    }
}
