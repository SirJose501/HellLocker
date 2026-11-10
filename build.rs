fn main() {
    slint_build::compile_with_config(
        "src/ui/panel.slint",
        slint_build::CompilerConfiguration::new()
            .with_style("fluent-dark".into())
            .with_default_translation_context(slint_build::DefaultTranslationContext::None)
            .with_bundled_translations("translations"),
    )
    .expect("failed to compile preset panel");
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    bundle_preset_icons();

    #[cfg(target_os = "windows")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/app-icon.ico")
            .compile()
            .expect("failed to embed Windows application icon");
    }
}

/// HellLocker 1.9.5: every picture under assets/icons/<group>/ is built into the app as
/// a preset icon. A group's names.tsv ("file-stem<TAB>Shown name") sets the order and
/// names; pictures it doesn't list follow, named after their file.
fn bundle_preset_icons() {
    use std::fmt::Write as _;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    println!("cargo:rerun-if-changed=assets/icons");
    let mut out = String::from("pub static BUNDLED: &[(&str, &str, &str, &[u8])] = &[\n");
    let mut groups: Vec<_> = std::fs::read_dir(&root)
        .map(|dir| {
            dir.flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    groups.sort();
    for group_dir in groups {
        println!("cargo:rerun-if-changed={}", group_dir.display());
        let group = group_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut listed: Vec<(String, String)> =
            std::fs::read_to_string(group_dir.join("names.tsv"))
                .unwrap_or_default()
                .lines()
                .filter_map(|line| {
                    let (stem, name) = line.split_once('\t')?;
                    Some((stem.trim().to_owned(), name.trim().to_owned()))
                })
                .collect();
        let mut files: Vec<_> = std::fs::read_dir(&group_dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            })
            .collect();
        files.sort();
        for file in &files {
            let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
            if !listed.iter().any(|(listed, _)| *listed == stem) {
                let name = stem.replace(['-', '_'], " ");
                listed.push((stem, name));
            }
        }
        for (stem, name) in listed {
            let file = group_dir.join(format!("{stem}.png"));
            if file.is_file() {
                writeln!(
                    out,
                    "    ({group:?}, {stem:?}, {name:?}, include_bytes!({:?})),",
                    file.display().to_string()
                )
                .unwrap();
            }
        }
    }
    out.push_str("];\n");
    let target = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("bundled_icons.rs");
    std::fs::write(target, out).expect("failed to write the bundled icon list");
}
