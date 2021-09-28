fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let mut builder = luajit_src::Build::new();
    if cfg!(feature = "lua52compat") {
        builder.lua52compat(true);
    }
    let artifacts = builder.build();
    artifacts.print_cargo_metadata();
}
