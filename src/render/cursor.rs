/// Default cursor image loader.
///
/// Tries the system xcursor theme first; falls back to a built-in 16×16
/// white arrow so Pancake always shows something.
use smithay::backend::renderer::gles::ffi;

pub const DEFAULT_CURSOR_SIZE: u32 = 24;

// ── Public API ────────────────────────────────────────────────────────────────

/// RGBA pixels + dimensions + hotspot for the pointer cursor.
pub struct CursorImage {
    pub pixels: Vec<u8>,   // RGBA8
    pub width:  u32,
    pub height: u32,
    pub hot_x:  u32,
    pub hot_y:  u32,
}

/// Load the system cursor, falling back to the built-in arrow.
pub fn load_default() -> CursorImage {
    if let Some(img) = try_xcursor() {
        return img;
    }
    builtin_arrow()
}

// ── xcursor attempt ───────────────────────────────────────────────────────────

fn try_xcursor() -> Option<CursorImage> {
    let theme = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
    let want_size: u32 = std::env::var("XCURSOR_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CURSOR_SIZE);

    let theme = xcursor::CursorTheme::load(&theme);
    let path  = theme.load_icon("default")?;
    let data  = std::fs::read(path).ok()?;
    let images = xcursor::parser::parse_xcursor(&data)?;

    let img = images
        .iter()
        .min_by_key(|i| (i.size as i32 - want_size as i32).unsigned_abs())
        .or_else(|| images.first())?;

    // xcursor pixels are packed ARGB u32 — convert to RGBA bytes
    let pixels: Vec<u8> = img.pixels_argb.iter().flat_map(|&argb| {
        let argb = argb as u32;
        let a = ((argb >> 24) & 0xFF) as u8;
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >> 8)  & 0xFF) as u8;
        let b = (argb          & 0xFF) as u8;
        [r, g, b, a]
    }).collect();

    Some(CursorImage {
        pixels,
        width:  img.size,
        height: img.size,
        hot_x:  img.xhot,
        hot_y:  img.yhot,
    })
}

// ── Built-in 16×16 arrow (fallback) ──────────────────────────────────────────

fn builtin_arrow() -> CursorImage {
    const S: usize = 16;
    #[rustfmt::skip]
    const MAP: &[&str] = &[
        "B...............",
        "BB..............",
        "BWB.............",
        "BWWB............",
        "BWWWB...........",
        "BWWWWB..........",
        "BWWWWWB.........",
        "BWWWWWWB........",
        "BWWWWWWWB.......",
        "BWWWWWB.........",
        "BWWBWWB.........",
        "BWB.BWWB........",
        "BB...BWWB.......",
        "B.....BWB.......",
        "......BB........",
        "................",
    ];

    let mut pixels = vec![0u8; S * S * 4];
    for (y, row) in MAP.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let i = (y * S + x) * 4;
            match ch {
                'B' => { pixels[i] = 0;   pixels[i+1] = 0;   pixels[i+2] = 0;   pixels[i+3] = 255; }
                'W' => { pixels[i] = 255; pixels[i+1] = 255; pixels[i+2] = 255; pixels[i+3] = 255; }
                _   => {}
            }
        }
    }

    CursorImage { pixels, width: S as u32, height: S as u32, hot_x: 0, hot_y: 0 }
}

// ── GL cursor renderer ────────────────────────────────────────────────────────

type Gl  = ffi::Gles2;
type Tex = ffi::types::GLuint;
type Prg = ffi::types::GLuint;
type Buf = ffi::types::GLuint;
type Loc = ffi::types::GLint;

/// Compiled GPU state for drawing one cursor image.
pub struct CursorGl {
    pub tex:     Tex,
    pub width:   u32,
    pub height:  u32,
    pub hot_x:   u32,
    pub hot_y:   u32,
    pub prg:     Prg,
    pub vbo:     Buf,
    pub u_ss:    Loc, // uniform: screen size (vec2)
    pub u_pos:   Loc, // uniform: top-left pixel position (vec2)
    pub u_size:  Loc, // uniform: cursor pixel size (vec2)
    pub u_tex:   Loc, // uniform: sampler2D
}

const CURSOR_VERT: &str = r#"
#version 100
attribute vec2  a_pos;          // 0..1 local quad coords
uniform   vec2  u_screen_size;
uniform   vec2  u_cursor_pos;   // top-left pixel of the cursor on screen
uniform   vec2  u_cursor_size;
varying   vec2  v_uv;
void main() {
    v_uv = a_pos;
    vec2 px = u_cursor_pos + a_pos * u_cursor_size;
    vec2 ndc = (px / u_screen_size) * 2.0 - 1.0;
    gl_Position = vec4(ndc.x, -ndc.y, 0.0, 1.0);
}
"#;

const CURSOR_FRAG: &str = r#"
#version 100
precision mediump float;
uniform sampler2D u_tex;
varying vec2 v_uv;
void main() {
    vec4 c = texture2D(u_tex, v_uv);
    if (c.a < 0.01) discard;
    gl_FragColor = c;
}
"#;

/// Compile shaders and upload texture; call inside a valid GL context.
pub unsafe fn init_cursor_gl(gl: &Gl, img: &CursorImage) -> Result<CursorGl, String> {
    // ── Texture ──────────────────────────────────────────────────────────────
    let mut tex: Tex = 0;
    gl.GenTextures(1, &mut tex);
    gl.BindTexture(ffi::TEXTURE_2D, tex);
    gl.TexImage2D(
        ffi::TEXTURE_2D, 0, ffi::RGBA as _,
        img.width as _, img.height as _, 0,
        ffi::RGBA, ffi::UNSIGNED_BYTE,
        img.pixels.as_ptr() as *const _,
    );
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as _);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as _);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_S,     ffi::CLAMP_TO_EDGE as _);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_T,     ffi::CLAMP_TO_EDGE as _);
    gl.BindTexture(ffi::TEXTURE_2D, 0);

    // ── Shaders ──────────────────────────────────────────────────────────────
    let prg = compile_cursor_prog(gl)?;

    // ── Unit quad VBO (0..1 × 0..1 triangle strip) ───────────────────────────
    let quad: [f32; 8] = [0.0, 0.0,  1.0, 0.0,  0.0, 1.0,  1.0, 1.0];
    let mut vbo: Buf = 0;
    gl.GenBuffers(1, &mut vbo);
    gl.BindBuffer(ffi::ARRAY_BUFFER, vbo);
    gl.BufferData(
        ffi::ARRAY_BUFFER,
        std::mem::size_of_val(&quad) as _,
        quad.as_ptr() as *const _,
        ffi::STATIC_DRAW,
    );
    gl.BindBuffer(ffi::ARRAY_BUFFER, 0);

    let u_ss   = gl.GetUniformLocation(prg, b"u_screen_size\0".as_ptr() as *const _);
    let u_pos  = gl.GetUniformLocation(prg, b"u_cursor_pos\0".as_ptr() as *const _);
    let u_size = gl.GetUniformLocation(prg, b"u_cursor_size\0".as_ptr() as *const _);
    let u_tex  = gl.GetUniformLocation(prg, b"u_tex\0".as_ptr() as *const _);

    Ok(CursorGl {
        tex,
        width: img.width, height: img.height,
        hot_x: img.hot_x, hot_y: img.hot_y,
        prg, vbo, u_ss, u_pos, u_size, u_tex,
    })
}

/// Draw the cursor at `(screen_x, screen_y)` in screen pixels (top-left corner
/// before hotspot correction). Call this inside a valid GL context.
pub unsafe fn draw_cursor_gl(
    gl:   &Gl,
    cur:  &CursorGl,
    sw: u32, sh: u32,
    pointer_x: f64, pointer_y: f64,
) {
    let x = pointer_x - cur.hot_x as f64;
    let y = pointer_y - cur.hot_y as f64;

    gl.Enable(ffi::BLEND);
    gl.BlendFunc(ffi::SRC_ALPHA, ffi::ONE_MINUS_SRC_ALPHA);

    gl.UseProgram(cur.prg);
    gl.Uniform2f(cur.u_ss,   sw as f32, sh as f32);
    gl.Uniform2f(cur.u_pos,  x as f32,  y as f32);
    gl.Uniform2f(cur.u_size, cur.width as f32, cur.height as f32);
    gl.Uniform1i(cur.u_tex,  0);

    gl.ActiveTexture(ffi::TEXTURE0);
    gl.BindTexture(ffi::TEXTURE_2D, cur.tex);

    gl.BindBuffer(ffi::ARRAY_BUFFER, cur.vbo);
    let a_pos_loc = gl.GetAttribLocation(cur.prg, b"a_pos\0".as_ptr() as *const _);
    if a_pos_loc >= 0 {
        gl.EnableVertexAttribArray(a_pos_loc as u32);
        gl.VertexAttribPointer(a_pos_loc as u32, 2, ffi::FLOAT, ffi::FALSE, 0, std::ptr::null());
        gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);
        gl.DisableVertexAttribArray(a_pos_loc as u32);
    }

    gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
    gl.BindTexture(ffi::TEXTURE_2D, 0);
    gl.UseProgram(0);

    // Restore Smithay's expected blend state
    gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
}

// ── Shader helpers ────────────────────────────────────────────────────────────

unsafe fn compile_shader(gl: &Gl, ty: ffi::types::GLenum, src: &str) -> Result<ffi::types::GLuint, String> {
    let s = gl.CreateShader(ty);
    gl.ShaderSource(s, 1,
        &src.as_ptr() as *const *const u8 as *const *const ffi::types::GLchar,
        &(src.len() as ffi::types::GLint) as *const _,
    );
    gl.CompileShader(s);
    let mut ok = 0i32;
    gl.GetShaderiv(s, ffi::COMPILE_STATUS, &mut ok);
    if ok == 0 {
        let mut len = 0i32;
        gl.GetShaderiv(s, ffi::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; len.max(0) as usize];
        gl.GetShaderInfoLog(s, len, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _);
        gl.DeleteShader(s);
        return Err(format!("cursor shader: {}", String::from_utf8_lossy(&buf).trim_end_matches('\0')));
    }
    Ok(s)
}

unsafe fn compile_cursor_prog(gl: &Gl) -> Result<Prg, String> {
    let v = compile_shader(gl, ffi::VERTEX_SHADER,   CURSOR_VERT)?;
    let f = compile_shader(gl, ffi::FRAGMENT_SHADER, CURSOR_FRAG).map_err(|e| { gl.DeleteShader(v); e })?;
    let p = gl.CreateProgram();
    gl.AttachShader(p, v);
    gl.AttachShader(p, f);
    gl.LinkProgram(p);
    gl.DetachShader(p, v); gl.DeleteShader(v);
    gl.DetachShader(p, f); gl.DeleteShader(f);
    let mut ok = 0i32;
    gl.GetProgramiv(p, ffi::LINK_STATUS, &mut ok);
    if ok == 0 {
        gl.DeleteProgram(p);
        return Err("cursor program link failed".into());
    }
    Ok(p)
}
