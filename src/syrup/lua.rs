/// Lua 5.4 frontend for Syrup.
///
/// The `.confi` file is executed as a Lua 5.4 script. It must return a table
/// whose keys are section names and whose values are tables of key=value pairs.
///
/// ```lua
/// !lang lua
///
/// return {
///     compositor = {
///         terminal    = "foot",
///         blur_passes = 4,
///         wallpaper   = "/usr/share/pancake/walls/aero-default.jpg",
///         tint        = {0.55, 0.70, 1.00, 0.18},
///     },
///     keybinds = {
///         terminal = "Super+T",
///         close    = "Super+Q",
///         quit     = "Super+Escape",
///         cycle    = "Super+Tab",
///     },
/// }
/// ```
///
/// Globals are sandboxed: `os.execute`, `io`, `require`, and `load` are
/// removed so config scripts cannot perform arbitrary system calls.
use super::{SyrupDoc, SyrupSection, SyrupValue};
use mlua::prelude::*;
use std::collections::HashMap;
use tracing::warn;

pub fn parse(src: &str) -> SyrupDoc {
    // Strip `!lang lua` directive before running
    let script: String = src
        .lines()
        .filter(|l| !l.trim().starts_with("!lang"))
        .collect::<Vec<_>>()
        .join("\n");

    match parse_lua(&script) {
        Ok(doc) => doc,
        Err(e) => {
            warn!("Syrup Lua parse error: {e}");
            SyrupDoc::default()
        }
    }
}

fn parse_lua(script: &str) -> LuaResult<SyrupDoc> {
    let lua = Lua::new();

    // Sandbox: remove dangerous globals
    {
        let globals = lua.globals();
        for key in &["os", "io", "require", "load", "loadfile", "dofile", "package"] {
            let _ = globals.set(*key, LuaValue::Nil);
        }
        // Restrict os table further if somehow still accessible
    }

    let result: LuaTable = lua.load(script).eval()?;

    let mut doc: HashMap<String, SyrupSection> = HashMap::new();

    for pair in result.pairs::<LuaValue, LuaValue>() {
        let (k, v) = pair?;
        let section_name = match k {
            LuaValue::String(s) => s.to_str()?.to_string(),
            _ => continue,
        };

        if let LuaValue::Table(tbl) = v {
            let mut section: SyrupSection = HashMap::new();
            for pair2 in tbl.pairs::<LuaValue, LuaValue>() {
                let (k2, v2) = pair2?;
                let key = match k2 {
                    LuaValue::String(s) => s.to_str()?.to_string(),
                    _ => continue,
                };
                if let Some(val) = lua_value_to_syrup(v2) {
                    section.insert(key, val);
                }
            }
            doc.insert(section_name, section);
        }
    }

    Ok(SyrupDoc(doc))
}

fn lua_value_to_syrup(v: LuaValue) -> Option<SyrupValue> {
    match v {
        LuaValue::String(s) => Some(SyrupValue::String(s.to_str().ok()?.to_string())),
        LuaValue::Integer(i) => Some(SyrupValue::Int(i)),
        LuaValue::Number(f) => Some(SyrupValue::Float(f)),
        LuaValue::Boolean(b) => Some(SyrupValue::Bool(b)),
        LuaValue::Table(t) => {
            // Treat numeric-keyed tables as arrays
            let mut arr: Vec<SyrupValue> = Vec::new();
            for pair in t.pairs::<LuaInteger, LuaValue>() {
                match pair {
                    Ok((_, val)) => {
                        if let Some(sv) = lua_value_to_syrup(val) {
                            arr.push(sv);
                        }
                    }
                    Err(_) => break,
                }
            }
            if arr.is_empty() { None } else { Some(SyrupValue::Array(arr)) }
        }
        _ => None,
    }
}
