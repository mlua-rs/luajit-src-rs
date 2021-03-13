use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Build {
    out_dir: Option<PathBuf>,
    target: Option<String>,
    host: Option<String>,
}

pub struct Artifacts {
    include_dir: PathBuf,
    lib_dir: PathBuf,
    libs: Vec<String>,
}

impl Build {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Build {
        Build {
            out_dir: env::var_os("OUT_DIR").map(|s| PathBuf::from(s).join("luajit-build")),
            target: env::var("TARGET").ok(),
            host: env::var("HOST").ok(),
        }
    }

    pub fn out_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Build {
        self.out_dir = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn target(&mut self, target: &str) -> &mut Build {
        self.target = Some(target.to_string());
        self
    }

    pub fn host(&mut self, host: &str) -> &mut Build {
        self.host = Some(host.to_string());
        self
    }

    fn cmd_make(&self) -> Command {
        match &self.host.as_ref().expect("HOST dir not set")[..] {
            "x86_64-unknown-dragonfly" => Command::new("gmake"),
            "x86_64-unknown-freebsd" => Command::new("gmake"),
            _ => Command::new("make"),
        }
    }

    pub fn build(&mut self) -> Artifacts {
        let target = &self.target.as_ref().expect("TARGET not set")[..];

        if target.contains("msvc") {
            return self.build_msvc();
        }

        self.build_unix()
    }

    pub fn build_unix(&mut self) -> Artifacts {
        let target = &self.target.as_ref().expect("TARGET not set")[..];
        let host = &self.host.as_ref().expect("HOST not set")[..];
        let out_dir = self.out_dir.as_ref().expect("OUT_DIR not set");
        let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("luajit2");
        let build_dir = out_dir.join("build");
        let lib_dir = out_dir.join("lib");
        let include_dir = out_dir.join("include");

        for dir in &[&build_dir, &lib_dir, &include_dir] {
            if dir.exists() {
                fs::remove_dir_all(dir)
                    .unwrap_or_else(|e| panic!("cannot remove {}: {}", dir.display(), e));
            }
            fs::create_dir_all(dir)
                .unwrap_or_else(|e| panic!("cannot create {}: {}", dir.display(), e));
        }
        cp_r(&source_dir, &build_dir);

        let mut cc = cc::Build::new();
        cc.target(target).host(host).warnings(false).opt_level(2);
        let compiler = cc.get_compiler();
        let compiler_path = compiler.path().to_str().unwrap();

        let mut make = self.cmd_make();
        make.current_dir(build_dir.join("src"));
        make.arg("-e");

        match target {
            "x86_64-apple-darwin" => {
                // 10.15 causes segmentation fault on github ci
                make.env("MACOSX_DEPLOYMENT_TARGET", "10.11");
            }
            "aarch64-apple-darwin" => {
                make.env("MACOSX_DEPLOYMENT_TARGET", "11.0");
            }
            _ => {}
        }

        // Infer ar/ranlib tools from cross compilers if the it looks like
        // we're doing something like `foo-gcc` route that to `foo-ranlib`
        // as well.
        if compiler_path.ends_with("-gcc") && !target.contains("unknown-linux-musl") {
            let prefix = &compiler_path[..compiler_path.len() - 3];
            make.env("CROSS", prefix);
        }

        make.env("BUILDMODE", "static");
        make.env("XCFLAGS", "-fPIC");
        self.run_command(make, "building LuaJIT");

        for f in &["lauxlib.h", "lua.h", "luaconf.h", "luajit.h", "lualib.h"] {
            fs::copy(build_dir.join("src").join(f), include_dir.join(f)).unwrap();
        }
        fs::copy(
            build_dir.join("src").join("libluajit.a"),
            lib_dir.join("libluajit-5.1.a"),
        )
        .unwrap();

        Artifacts {
            lib_dir,
            include_dir,
            libs: vec!["luajit-5.1".to_string()],
        }
    }

    pub fn build_msvc(&mut self) -> Artifacts {
        let target = &self.target.as_ref().expect("TARGET not set")[..];
        let out_dir = self.out_dir.as_ref().expect("OUT_DIR not set");
        let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("luajit2");
        let build_dir = out_dir.join("build");
        let lib_dir = out_dir.join("lib");
        let include_dir = out_dir.join("include");

        for dir in &[&build_dir, &lib_dir, &include_dir] {
            if dir.exists() {
                fs::remove_dir_all(dir)
                    .unwrap_or_else(|e| panic!("cannot remove {}: {}", dir.display(), e));
            }
            fs::create_dir_all(dir)
                .unwrap_or_else(|e| panic!("cannot create {}: {}", dir.display(), e));
        }
        cp_r(&source_dir, &build_dir);

        let mut msvcbuild = Command::new(build_dir.join("src").join("msvcbuild.bat"));
        msvcbuild.current_dir(build_dir.join("src"));
        msvcbuild.arg("static");

        let cl = cc::windows_registry::find_tool(&target, "cl.exe").expect("failed to find cl");
        for (k, v) in cl.env() {
            msvcbuild.env(k, v);
        }

        self.run_command(msvcbuild, "building LuaJIT");

        for f in &["lauxlib.h", "lua.h", "luaconf.h", "luajit.h", "lualib.h"] {
            fs::copy(build_dir.join("src").join(f), include_dir.join(f)).unwrap();
        }
        fs::copy(
            build_dir.join("src").join("lua51.lib"),
            lib_dir.join("luajit.lib"),
        )
        .unwrap();

        Artifacts {
            lib_dir,
            include_dir,
            libs: vec!["luajit".to_string()],
        }
    }

    fn run_command(&self, mut command: Command, desc: &str) {
        println!("running {:?}", command);
        let status = command.status().unwrap();
        if !status.success() {
            panic!(
                "
Error {}:
    Command: {:?}
    Exit status: {}
    ",
                desc, command, status
            );
        }
    }
}

fn cp_r(src: &Path, dst: &Path) {
    for f in fs::read_dir(src).unwrap() {
        let f = f.unwrap();
        let path = f.path();
        let name = path.file_name().unwrap();

        // Skip git metadata
        if name.to_str() == Some(".git") {
            continue;
        }

        let dst = dst.join(name);
        if f.file_type().unwrap().is_dir() {
            fs::create_dir_all(&dst).unwrap();
            cp_r(&path, &dst);
        } else {
            let _ = fs::remove_file(&dst);
            fs::copy(&path, &dst).unwrap();
        }
    }
}

impl Artifacts {
    pub fn include_dir(&self) -> &Path {
        &self.include_dir
    }

    pub fn lib_dir(&self) -> &Path {
        &self.lib_dir
    }

    pub fn libs(&self) -> &[String] {
        &self.libs
    }

    pub fn print_cargo_metadata(&self) {
        println!("cargo:rustc-link-search=native={}", self.lib_dir.display());
        for lib in self.libs.iter() {
            println!("cargo:rustc-link-lib=static={}", lib);
        }
        println!("cargo:include={}", self.include_dir.display());
        println!("cargo:lib={}", self.lib_dir.display());
    }
}
