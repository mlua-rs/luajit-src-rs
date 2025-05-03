#![allow(clippy::missing_safety_doc)]

use std::os::raw::{c_char, c_int, c_long, c_void};

extern "C" {
    pub fn luaL_newstate() -> *mut c_void;
    pub fn luaL_openlibs(state: *mut c_void);
    pub fn lua_getfield(state: *mut c_void, index: c_int, k: *const c_char);
    pub fn lua_tolstring(state: *mut c_void, index: c_int, len: *mut c_long) -> *const c_char;
    pub fn luaL_loadstring(state: *mut c_void, s: *const c_char) -> c_int;
    pub fn lua_pcall(state: *mut c_void, nargs: c_int, nresults: c_int, errfunc: c_int) -> c_int;
}

pub unsafe fn lua_getglobal(state: *mut c_void, k: *const c_char) {
    lua_getfield(state, -10002 /* LUA_GLOBALSINDEX */, k);
}

pub unsafe fn to_string<'a>(state: *mut c_void, index: c_int) -> &'a str {
    let mut len: c_long = 0;
    let str_ptr = lua_tolstring(state, index, &mut len);
    let bytes = std::slice::from_raw_parts(str_ptr as *const u8, len as usize);
    std::str::from_utf8(bytes).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua() {
        unsafe {
            let state = luaL_newstate();
            assert!(!state.is_null());

            luaL_openlibs(state);

            let version = {
                lua_getglobal(state, "_VERSION\0".as_ptr().cast());
                to_string(state, -1)
            };
            assert_eq!(version, "Lua 5.1");

            let jit_version = {
                luaL_loadstring(state, c"return jit.version".as_ptr().cast());
                let ret = lua_pcall(state, 0, 1, 0);
                assert_eq!(0, ret);
                to_string(state, -1)
            };
            let mut version_it = jit_version.split('.');
            assert_eq!(version_it.next().unwrap(), "LuaJIT 2");
            assert_eq!(version_it.next().unwrap(), "1");
            assert!(version_it.next().unwrap().parse::<u32>().is_ok());
        }
    }

    #[test]
    fn test_lua52compat() {
        unsafe {
            let state = luaL_newstate();
            assert!(!state.is_null());

            luaL_openlibs(state);

            let code = "
                lua52compat = \"no\"
                t = setmetatable({}, {
                    __pairs = function(t)
                        lua52compat = \"yes\"
                        return next, t, nil
                    end
                })
                for k,v in pairs(t) do end
            \0";
            let ret1 = luaL_loadstring(state, code.as_ptr().cast());
            assert_eq!(0, ret1);
            let ret2 = lua_pcall(state, 0, 0, 0);
            assert_eq!(0, ret2);

            let lua52compat = {
                lua_getglobal(state, "lua52compat\0".as_ptr().cast());
                to_string(state, -1) == "yes"
            };
            assert_eq!(lua52compat, cfg!(feature = "lua52compat"));
        }
    }
}
