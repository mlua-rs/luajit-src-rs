# luajit-src

[![Build Status]][github-actions]

[Build Status]: https://github.com/mlua-rs/luajit-src-rs/workflows/CI/badge.svg
[github-actions]: https://github.com/mlua-rs/luajit-src-rs/actions

This crate contains the sources of [LuaJIT] 2.1 (from Mike Pall) and logic to build it.
Intended to be consumed by the [mlua-sys] crate.

By default, this crate builds vanilla LuaJIT from the official repository. An optional `openresty` feature is available that builds [OpenResty's LuaJIT fork](https://github.com/openresty/luajit2) instead, which includes additional patches and support for architectures not available in vanilla LuaJIT, such as s390x (interpreter-only mode, no JIT compilation).

[LuaJIT]: https://github.com/LuaJIT/LuaJIT
[mlua-sys]: https://crates.io/crates/mlua-sys

# License

This project is licensed under [MIT license](http://opensource.org/licenses/MIT)
