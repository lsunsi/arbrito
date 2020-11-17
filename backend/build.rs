use std::path::{Path, PathBuf};

fn main() {
    std::fs::create_dir_all(Path::new("./src/gen")).unwrap();

    for entry in std::fs::read_dir("./abis")
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    {
        let contract_name = entry
            .path()
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();

        ethcontract_generate::Builder::new(entry.path().to_str().unwrap())
            .with_contract_name_override(Some(&contract_name))
            .with_visibility_modifier(Some("pub"))
            .generate()
            .unwrap()
            .write_to_file(PathBuf::from(format!(
                "./src/gen/{}.rs",
                &contract_name.to_lowercase()
            )))
            .unwrap();
    }
}
