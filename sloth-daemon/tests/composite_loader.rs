//! End-to-end check that the daemon's registered loader (a `CompositeLoader`
//! over the DvorakJ and sloth adapters) can load *both* a real DvorakJ
//! `.txt` layout and a sloth TOML layout, exactly as `main()` registers it.

use sloth_core::loader::{CompositeLoader, LayoutLoader};

fn loader() -> CompositeLoader {
    CompositeLoader::new(vec![
        Box::new(sloth_dvorakj_adapter::RmapDvorakJLayoutLoader::new()),
        Box::new(sloth_core::sloth_parser::SlothLayoutLoader::new()),
    ])
}

#[test]
fn loads_real_dvorakj_txt_layout() {
    let bytes = include_bytes!("../../data/layouts/圧縮版_新下駄配列.txt");
    let layout = loader()
        .load(bytes, "圧縮版_新下駄配列.jp.txt")
        .expect("dvorakj layout should load through the composite loader");
    assert!(
        !layout.single_map.is_empty() || !layout.combos.is_empty(),
        "loaded layout should have some mapping"
    );
}

#[test]
fn loads_sloth_toml_layout() {
    let text = include_str!("../../config-idea/config.toml");
    let layout = loader()
        .load(text.as_bytes(), "config.toml")
        .expect("sloth toml layout should load through the composite loader");
    assert_eq!(layout.name, "my-layout");
}

#[test]
fn loads_sloth_toml_layout_from_data_layouts() {
    // Same file sloth-config's file picker (filters .txt/.toml/.json) would
    // let a user select and AppConfig::layout_path_for_app would then read,
    // placed exactly where a real profile's `layout` field would point.
    let bytes = include_bytes!("../../data/layouts/shingeta.toml");
    let layout = loader()
        .load(bytes, "data/layouts/shingeta.toml")
        .expect("sloth toml layout under data/layouts should load");
    assert_eq!(layout.name, "shingeta");
    assert!(!layout.combos.is_empty());
}
