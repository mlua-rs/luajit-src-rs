use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Represents the configuration for building LuaJIT artifacts.
pub struct Build {
    out_dir: Option<PathBuf>,
    target: Option<String>,
    host: Option<String>,
    lua52compat: bool,
}

/// Represents the artifacts produced by the build process.
pub struct Artifacts {
    include_dir: PathBuf,
    lib_dir: PathBuf,
    libs: Vec<String>,
}

impl Default for Build {
    fn default() -> Self {
        Build {
            out_dir: env::var_os("OUT_DIR").map(PathBuf::from),
            target: env::var("TARGET").ok(),
            host: env::var("HOST").ok(),
            lua52compat: false,
        }
    }
}

impl Build {
    /// Creates a new `Build` instance with default settings.
    pub fn new() -> Build {
        Build::default()
    }

    /// Sets the output directory for the build artifacts.
    ///
    /// This is required if called outside of a build script.
    pub fn out_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Build {
        self.out_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Sets the target architecture for the build.
    ///
    /// This is required if called outside of a build script.
    pub fn target(&mut self, target: &str) -> &mut Build {
        self.target = Some(target.to_string());
        self
    }

    /// Sets the host architecture for the build.
    ///
    /// This is optional and will default to the environment variable `HOST` if not set.
    /// If called outside of a build script, it will default to the target architecture.
    pub fn host(&mut self, host: &str) -> &mut Build {
        self.host = Some(host.to_string());
        self
    }

    /// Enables or disables Lua 5.2 limited compatibility mode.
    pub fn lua52compat(&mut self, enabled: bool) -> &mut Build {
        self.lua52compat = enabled;
        self
    }

    fn cmd_make(&self) -> Command {
        match &self.host.as_ref().expect("HOST is not set")[..] {
            "x86_64-unknown-dragonfly" => Command::new("gmake"),
            "x86_64-unknown-freebsd" => Command::new("gmake"),
            _ => Command::new("make"),
        }
    }

    /// Builds the LuaJIT artifacts.
    pub fn build(&mut self) -> Artifacts {
        let target = &self.target.as_ref().expect("TARGET is not set")[..];

        if target.contains("msvc") {
            return self.build_msvc();
        }

        self.build_unix()
    }

    fn build_unix(&mut self) -> Artifacts {
        let target = &self.target.as_ref().expect("TARGET is not set")[..];
        let host = &self.host.as_ref().expect("HOST is not set")[..];
        let out_dir = self.out_dir.as_ref().expect("OUT_DIR is not set");
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source_dir = manifest_dir.join("luajit2");
        let build_dir = out_dir.join("luajit-build");
        let lib_dir = out_dir.join("lib");
        let include_dir = out_dir.join("include");

        // Cleanup
        for dir in [&build_dir, &lib_dir, &include_dir] {
            if dir.exists() {
                fs::remove_dir_all(dir)
                    .unwrap_or_else(|e| panic!("cannot remove {}: {e}", dir.display()));
            }
            fs::create_dir_all(dir)
                .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
        }
        cp_r(&source_dir, &build_dir);

        // Copy release version file
        let relver = build_dir.join(".relver");
        #[rustfmt::skip]
        fs::copy(manifest_dir.join("luajit_relver.txt"), &relver).unwrap();

        // Fix permissions for certain build situations
        let mut perms = fs::metadata(&relver).unwrap().permissions();
        perms.set_readonly(false);
        fs::set_permissions(relver, perms).unwrap();

        let mut cc = cc::Build::new();
        cc.warnings(false);
        let compiler = cc.get_compiler();
        let compiler_path = compiler.path().to_str().unwrap();

        let mut make = self.cmd_make();
        make.current_dir(build_dir.join("src"));
        make.arg("-e");

        match target {
            "x86_64-apple-darwin" if env::var_os("MACOSX_DEPLOYMENT_TARGET").is_none() => {
                make.env("MACOSX_DEPLOYMENT_TARGET", "10.14");
            }
            "aarch64-apple-darwin" if env::var_os("MACOSX_DEPLOYMENT_TARGET").is_none() => {
                make.env("MACOSX_DEPLOYMENT_TARGET", "11.0");
            }
            _ if target.contains("linux") => {
                make.env("TARGET_SYS", "Linux");
            }
            _ if target.contains("windows") => {
                make.env("TARGET_SYS", "Windows");
            }
            _ => {}
        }

        let target_pointer_width = env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap();
        if target_pointer_width == "32" && env::var_os("HOST_CC").is_none() {
            // 32-bit cross-compilation?
            let host_cc = cc::Build::new().target(host).get_compiler();
            make.env("HOST_CC", format!("{} -m32", host_cc.path().display()));
        }

        // Infer ar/ranlib tools from cross compilers if the it looks like
        // we're doing something like `foo-gcc` route that to `foo-ranlib`
        // as well.
        let prefix = if compiler_path.ends_with("-gcc") {
            &compiler_path[..compiler_path.len() - 3]
        } else if compiler_path.ends_with("-clang") {
            &compiler_path[..compiler_path.len() - 5]
        } else {
            ""
        };

        let compiler_path =
            which::which(compiler_path).unwrap_or_else(|_| panic!("cannot find {compiler_path}"));
        let bindir = compiler_path.parent().unwrap();
        let compiler_path = compiler_path.to_str().unwrap();
        let compiler_args = compiler.cflags_env();
        let compiler_args = compiler_args.to_str().unwrap();
        if env::var_os("STATIC_CC").is_none() {
            make.env("STATIC_CC", format!("{compiler_path} {compiler_args}"));
        }
        if env::var_os("TARGET_LD").is_none() {
            make.env("TARGET_LD", format!("{compiler_path} {compiler_args}"));
        }

        // Find ar
        if env::var_os("TARGET_AR").is_none() {
            let mut ar = if bindir.join(format!("{prefix}ar")).is_file() {
                bindir.join(format!("{prefix}ar")).into_os_string()
            } else if compiler.is_like_clang() && bindir.join("llvm-ar").is_file() {
                bindir.join("llvm-ar").into_os_string()
            } else if compiler.is_like_gnu() && bindir.join("ar").is_file() {
                bindir.join("ar").into_os_string()
            } else if let Ok(ar) = which::which(format!("{prefix}ar")) {
                ar.into_os_string()
            } else {
                panic!("cannot find {prefix}ar")
            };
            ar.push(" rcus");
            make.env("TARGET_AR", ar);
        }

        // Find strip
        if env::var_os("TARGET_STRIP").is_none() {
            let strip = if bindir.join(format!("{prefix}strip")).is_file() {
                bindir.join(format!("{prefix}strip"))
            } else if compiler.is_like_clang() && bindir.join("llvm-strip").is_file() {
                bindir.join("llvm-strip")
            } else if compiler.is_like_gnu() && bindir.join("strip").is_file() {
                bindir.join("strip")
            } else if let Ok(strip) = which::which(format!("{prefix}strip")) {
                strip
            } else {
                panic!("cannot find {prefix}strip")
            };
            make.env("TARGET_STRIP", strip);
        }

        let mut xcflags = vec!["-fPIC"];
        if self.lua52compat {
            xcflags.push("-DLUAJIT_ENABLE_LUA52COMPAT");
        }
        if cfg!(debug_assertions) {
            xcflags.push("-DLUA_USE_ASSERT");
            xcflags.push("-DLUA_USE_APICHECK");
        }

        make.env("BUILDMODE", "static");
        make.env("XCFLAGS", xcflags.join(" "));
        self.run_command(make, "building LuaJIT");

        Artifacts::make(&build_dir, &include_dir, &lib_dir, false)
    }

    fn build_msvc(&mut self) -> Artifacts {
        let target = &self.target.as_ref().expect("TARGET is not set")[..];
        let out_dir = self.out_dir.as_ref().expect("OUT_DIR is not set");
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source_dir = manifest_dir.join("luajit2");
        let build_dir = out_dir.join("luajit-build");
        let lib_dir = out_dir.join("lib");
        let include_dir = out_dir.join("include");

        // Cleanup
        for dir in [&build_dir, &lib_dir, &include_dir] {
            if dir.exists() {
                fs::remove_dir_all(dir)
                    .unwrap_or_else(|e| panic!("cannot remove {}: {e}", dir.display()));
            }
            fs::create_dir_all(dir)
                .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
        }
        cp_r(&source_dir, &build_dir);

        // Copy release version file
        #[rustfmt::skip]
        fs::copy(manifest_dir.join("luajit_relver.txt"), build_dir.join(".relver")).unwrap();

        let mut msvcbuild = Command::new(build_dir.join("src").join("msvcbuild.bat"));
        msvcbuild.current_dir(build_dir.join("src"));
        if self.lua52compat {
            msvcbuild.arg("lua52compat");
        }
        msvcbuild.arg("static");

        let cl = cc::windows_registry::find_tool(target, "cl.exe").expect("failed to find cl");
        for (k, v) in cl.env() {
            msvcbuild.env(k, v);
        }

        self.run_command(msvcbuild, "building LuaJIT");

        Artifacts::make(&build_dir, &include_dir, &lib_dir, true)
    }

    fn run_command(&self, mut command: Command, desc: &str) {
        let status = command.status().unwrap();
        if !status.success() {
            panic!(
                "
Error {desc}:
    Command: {command:?}
    Exit status: {status}
    "
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
    /// Returns the directory containing the LuaJIT headers.
    pub fn include_dir(&self) -> &Path {
        &self.include_dir
    }

    /// Returns the directory containing the LuaJIT libraries.
    pub fn lib_dir(&self) -> &Path {
        &self.lib_dir
    }

    /// Returns the names of the LuaJIT libraries built.
    pub fn libs(&self) -> &[String] {
        &self.libs
    }

    /// Prints the necessary Cargo metadata for linking the LuaJIT libraries.
    ///
    /// This method is typically called in a build script to inform Cargo
    /// about the location of the LuaJIT libraries and how to link them.
    pub fn print_cargo_metadata(&self) {
        println!("cargo:rerun-if-env-changed=HOST_CC");
        println!("cargo:rerun-if-env-changed=STATIC_CC");
        println!("cargo:rerun-if-env-changed=TARGET_LD");
        println!("cargo:rerun-if-env-changed=TARGET_AR");
        println!("cargo:rerun-if-env-changed=TARGET_STRIP");
        println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");

        println!("cargo:rustc-link-search=native={}", self.lib_dir.display());
        for lib in self.libs.iter() {
            println!("cargo:rustc-link-lib=static={lib}");
        }
    }

    fn make(build_dir: &Path, include_dir: &Path, lib_dir: &Path, is_msvc: bool) -> Self {
        for f in &["lauxlib.h", "lua.h", "luaconf.h", "luajit.h", "lualib.h"] {
            fs::copy(build_dir.join("src").join(f), include_dir.join(f)).unwrap();
        }

        let lib_name = if !is_msvc { "luajit" } else { "lua51" };
        let lib_file = if !is_msvc { "libluajit.a" } else { "lua51.lib" };
        if build_dir.join("src").join(lib_file).exists() {
            fs::copy(build_dir.join("src").join(lib_file), lib_dir.join(lib_file)).unwrap();
        }

        Artifacts {
            lib_dir: lib_dir.to_path_buf(),
            include_dir: include_dir.to_path_buf(),
            libs: vec![lib_name.to_string()],
        }
    }
}
