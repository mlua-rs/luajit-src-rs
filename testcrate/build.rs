fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let mut builder = luajit_src::Build::new();
    builder.lua52compat(cfg!(feature = "lua52compat"));
    let artifacts = builder.build();
    artifacts.print_cargo_metadata();
}
