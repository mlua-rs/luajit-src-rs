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

#[test]
fn test_lua() {
    use std::{ptr, slice};
    unsafe {
        let state = luaL_newstate();
        assert!(state != ptr::null_mut());

        luaL_openlibs(state);

        let version = {
            lua_getglobal(state, "_VERSION\0".as_ptr().cast());
            let mut len: c_long = 0;
            let version_ptr = lua_tolstring(state, -1, &mut len);
            slice::from_raw_parts(version_ptr as *const u8, len as usize)
        };

        assert_eq!(version, b"Lua 5.1");
    }
}

#[test]
fn test_lua52compat() {
    use std::{ptr, slice};
    unsafe {
        let state = luaL_newstate();
        assert!(state != ptr::null_mut());

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
            let mut len: c_long = 0;
            let lua52compat_ptr = lua_tolstring(state, -1, &mut len);
            slice::from_raw_parts(lua52compat_ptr as *const u8, len as usize)
        };

        #[cfg(feature = "lua52compat")]
        assert_eq!(lua52compat, b"yes");
        #[cfg(not(feature = "lua52compat"))]
        assert_eq!(lua52compat, b"no");
    }
}
