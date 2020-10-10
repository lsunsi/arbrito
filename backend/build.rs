use std::path::Path;

fn main() {
    std::fs::create_dir_all(Path::new("./src/gen")).unwrap();

    ethcontract_generate::Builder::new("./abis/Uniswap.json")
        .with_contract_name_override(Some("Uniswap"))
        .with_visibility_modifier(Some("pub"))
        .generate()
        .unwrap()
        .write_to_file(Path::new("./src/gen/uniswap.rs"))
        .unwrap();

    ethcontract_generate::Builder::new("./abis/Balancer.json")
        .with_contract_name_override(Some("Balancer"))
        .with_visibility_modifier(Some("pub"))
        .generate()
        .unwrap()
        .write_to_file(Path::new("./src/gen/balancer.rs"))
        .unwrap();
}
