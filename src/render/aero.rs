/// Aero frosted-glass rendering pipeline.
///
/// Each frame:
///  1. Render animated aurora orb gradient → scene_fbo  (full res)
///  2. Kawase down-pass                    → blur_a_fbo (½ res)
///  3. N ping-pong passes (offset scales per pass for quality)
///  4. draw_background()  — blit blurred result to screen + screen-blend tint
///  5. draw_glass_rect()  — per-window frosted glass panel (optional, call before windows)
use smithay::backend::renderer::gles::{ffi, GlesError, GlesFrame, GlesRenderer};
use smithay::utils::{Logical, Rectangle};
use std::time::Instant;
use tracing::error;

use crate::config::Config;
use std::path::PathBuf;

const DEFAULT_TINT: [f32; 4] = [0.52, 0.68, 1.00, 0.16];
const DEFAULT_PASSES: usize = 4;
const DEFAULT_DOWNSAMPLE: u32 = 2;

// ── Shared vertex shader ──────────────────────────────────────────────────────

const QUAD_VERT: &str = r#"
#version 100
attribute vec2 a_position;
varying   vec2 v_uv;
void main() {
    v_uv        = a_position * 0.5 + 0.5;
    gl_Position = vec4(a_position, 0.0, 1.0);
}
"#;

// ── Background: three drifting aurora orbs ────────────────────────────────────
//
// Three soft-edged light blobs drift slowly on a deep midnight base.
// Each orb has a different colour (Aero blue, ice teal, soft violet) and an
// independent, irrational angular velocity so they never repeat.
// A subtle horizontal shimmer line adds the glass-refraction texture.

const BG_FRAG: &str = r#"
#version 100
precision mediump float;
uniform float u_time;
varying vec2  v_uv;

void main() {
    float t  = u_time * 0.10;

    // Three orb centres that drift independently
    vec2 p1 = vec2(0.28 + sin(t * 0.71) * 0.22,  0.62 + cos(t * 0.53) * 0.18);
    vec2 p2 = vec2(0.72 + cos(t * 0.43) * 0.18,  0.38 + sin(t * 0.67) * 0.22);
    vec2 p3 = vec2(0.50 + sin(t * 0.89) * 0.14,  0.20 + cos(t * 0.78) * 0.14);

    // Gaussian-shaped falloff for each orb
    float d1 = dot(v_uv - p1, v_uv - p1);
    float d2 = dot(v_uv - p2, v_uv - p2);
    float d3 = dot(v_uv - p3, v_uv - p3);
    float o1 = exp(-d1 * 5.0);
    float o2 = exp(-d2 * 6.5);
    float o3 = exp(-d3 * 8.0);

    // Palette: deep midnight base + Aero blue + ice teal + soft violet
    vec3 base   = vec3(0.03, 0.06, 0.18);
    vec3 azure  = vec3(0.22, 0.52, 1.00);
    vec3 teal   = vec3(0.08, 0.68, 0.82);
    vec3 violet = vec3(0.42, 0.28, 0.88);

    vec3 col = base
             + azure  * o1 * 0.75
             + teal   * o2 * 0.55
             + violet * o3 * 0.42;

    // Sky gradient — subtly brightens toward the top
    col += azure * v_uv.y * 0.06;

    // Subtle horizontal shimmer — glass-refraction texture
    float shimmer = sin(v_uv.x * 55.0 + t * 4.0) * 0.009 + 1.0;

    gl_FragColor = vec4(clamp(col * shimmer, 0.0, 1.0), 1.0);
}
"#;

// ── Dual-Kawase down / up passes ──────────────────────────────────────────────

const DOWN_FRAG: &str = r#"
#version 100
precision mediump float;
uniform sampler2D u_tex;
uniform vec2      u_hp;
varying vec2      v_uv;
void main() {
    vec4 s = texture2D(u_tex, v_uv) * 4.0;
    s += texture2D(u_tex, v_uv - u_hp);
    s += texture2D(u_tex, v_uv + u_hp);
    s += texture2D(u_tex, v_uv + vec2( u_hp.x, -u_hp.y));
    s += texture2D(u_tex, v_uv + vec2(-u_hp.x,  u_hp.y));
    gl_FragColor = s / 8.0;
}
"#;

const UP_FRAG: &str = r#"
#version 100
precision mediump float;
uniform sampler2D u_tex;
uniform vec2      u_hp;
varying vec2      v_uv;
void main() {
    vec4 s;
    s  = texture2D(u_tex, v_uv + vec2(-u_hp.x * 2.0,  0.0));
    s += texture2D(u_tex, v_uv + vec2(-u_hp.x,         u_hp.y)) * 2.0;
    s += texture2D(u_tex, v_uv + vec2( 0.0,             u_hp.y * 2.0));
    s += texture2D(u_tex, v_uv + vec2( u_hp.x,          u_hp.y)) * 2.0;
    s += texture2D(u_tex, v_uv + vec2( u_hp.x * 2.0,   0.0));
    s += texture2D(u_tex, v_uv + vec2( u_hp.x,         -u_hp.y)) * 2.0;
    s += texture2D(u_tex, v_uv + vec2( 0.0,            -u_hp.y * 2.0));
    s += texture2D(u_tex, v_uv + vec2(-u_hp.x,         -u_hp.y)) * 2.0;
    gl_FragColor = s / 12.0;
}
"#;

// ── Wallpaper blit ────────────────────────────────────────────────────────────

const WALLPAPER_FRAG: &str = r#"
#version 100
precision mediump float;
uniform sampler2D u_tex;
varying vec2 v_uv;
void main() {
    gl_FragColor = texture2D(u_tex, vec2(v_uv.x, 1.0 - v_uv.y));
}
"#;

// ── Final blit: blurred texture + screen-blend tint + vignette ───────────────
//
// Screen blend formula: 1 - (1-a)(1-b) — more natural than additive, avoids
// the washed-out look. A subtle radial vignette darkens the edges for depth.

const BLIT_FRAG: &str = r#"
#version 100
precision mediump float;
uniform sampler2D u_tex;
uniform vec4      u_tint;
varying vec2      v_uv;
void main() {
    vec4 blur = texture2D(u_tex, v_uv);

    // Screen-blend the Aero tint over the blurred background
    vec3 screen = 1.0 - (1.0 - blur.rgb) * (1.0 - u_tint.rgb * u_tint.a);
    vec3 result = mix(blur.rgb, screen, 0.72);

    // Subtle vignette — darkens edges 8% to give the desktop depth
    vec2 vig = v_uv * 2.0 - 1.0;
    float vignette = 1.0 - dot(vig, vig) * 0.08;

    gl_FragColor = vec4(result * vignette, 1.0);
}
"#;

// ── Per-window frosted glass overlay ─────────────────────────────────────────
//
// Drawn before each window surface. Screen-blends the blurred background at
// the window's position with a stronger glass tint.  Alpha < 1 lets any
// behind-glass content bleed through on compositors that support it.

const GLASS_FRAG: &str = r#"
#version 100
precision mediump float;
uniform sampler2D u_tex;
uniform vec4      u_tint;
varying vec2      v_uv;
void main() {
    vec4 blur = texture2D(u_tex, v_uv);

    // Stronger screen-blend for denser frosted-glass look
    vec3 screen = 1.0 - (1.0 - blur.rgb) * (1.0 - u_tint.rgb * u_tint.a);
    vec3 result = mix(blur.rgb, screen, 0.85);

    gl_FragColor = vec4(result, 0.82);
}
"#;

// ── Type aliases ──────────────────────────────────────────────────────────────

type Gl  = ffi::Gles2;
type Tex = ffi::types::GLuint;
type Fbo = ffi::types::GLuint;
type Prg = ffi::types::GLuint;
type Buf = ffi::types::GLuint;
type Loc = ffi::types::GLint;

// ── GPU resource bundle ───────────────────────────────────────────────────────

struct AeroGl {
    scene_fbo:  Fbo,
    scene_tex:  Tex,
    blur_a_fbo: Fbo,
    blur_a_tex: Tex,
    blur_b_fbo: Fbo,
    blur_b_tex: Tex,

    bg_prg:        Prg,
    wallpaper_prg: Prg,
    down_prg:      Prg,
    up_prg:        Prg,
    blit_prg:      Prg,
    glass_prg:     Prg,

    quad_vbo: Buf,

    bg_u_time:    Loc,
    wp_u_tex:     Loc,
    down_u_tex:   Loc,
    down_u_hp:    Loc,
    up_u_tex:     Loc,
    up_u_hp:      Loc,
    blit_u_tex:   Loc,
    blit_u_tint:  Loc,
    glass_u_tex:  Loc,
    glass_u_tint: Loc,

    w: u32, h: u32,
    blur_w: u32, blur_h: u32,
    blur_passes: usize,
    tint: [f32; 4],
    wallpaper_tex: Tex,
}

// ── Public API ────────────────────────────────────────────────────────────────

pub struct AeroRenderer {
    gl: Option<AeroGl>,
    start: Instant,
    output_size: (u32, u32),
    blurred_tex: Option<Tex>,
    cfg_passes: usize,
    cfg_downsample: u32,
    cfg_tint: [f32; 4],
    cfg_wallpaper: Option<PathBuf>,
    wallpaper_rgba: Option<Vec<u8>>,
    wallpaper_wh: (u32, u32),
}

impl Default for AeroRenderer {
    fn default() -> Self {
        Self {
            gl: None,
            start: Instant::now(),
            output_size: (0, 0),
            blurred_tex: None,
            cfg_passes: DEFAULT_PASSES,
            cfg_downsample: DEFAULT_DOWNSAMPLE,
            cfg_tint: DEFAULT_TINT,
            cfg_wallpaper: None,
            wallpaper_rgba: None,
            wallpaper_wh: (0, 0),
        }
    }
}

impl AeroRenderer {
    pub fn apply_config(&mut self, config: &Config) {
        let mut changed = self.cfg_passes != config.blur_passes
            || self.cfg_downsample != config.blur_downsample
            || self.cfg_tint != config.tint;

        self.cfg_passes = config.blur_passes;
        self.cfg_downsample = config.blur_downsample;
        self.cfg_tint = config.tint;

        if self.cfg_wallpaper != config.wallpaper {
            self.cfg_wallpaper = config.wallpaper.clone();
            self.wallpaper_rgba = config.wallpaper.as_ref().and_then(|p| {
                load_wallpaper_rgba(p)
                    .map(|(pixels, w, h)| { self.wallpaper_wh = (w, h); pixels })
                    .map_err(|e| { error!("Wallpaper load error: {e}"); e })
                    .ok()
            });
            changed = true;
        }

        if changed {
            self.gl = None;
            self.blurred_tex = None;
        }
    }

    /// Run the blur pipeline. Call once per frame before rendering any surfaces.
    pub fn begin_frame(&mut self, renderer: &mut GlesRenderer, width: u32, height: u32) {
        if self.output_size != (width, height) {
            self.output_size = (width, height);
            self.gl = None;
            self.blurred_tex = None;
        }

        if width == 0 || height == 0 { return; }

        let elapsed = self.start.elapsed().as_secs_f32();

        if self.gl.is_none() {
            let (w, h, passes, ds, tint) = (width, height, self.cfg_passes, self.cfg_downsample, self.cfg_tint);
            let wp_rgba = self.wallpaper_rgba.clone();
            let wp_wh   = self.wallpaper_wh;
            match renderer.with_context(|gl| unsafe {
                init_gl(gl, w, h, ds, passes, tint, wp_rgba.as_deref(), wp_wh)
            }) {
                Ok(Ok(res)) => self.gl = Some(res),
                Ok(Err(msg)) => { error!("Aero: GL init failed: {msg}"); return; }
                Err(e)       => { error!("Aero: context error on init: {e}"); return; }
            }
        }

        let gl_res = self.gl.as_ref().unwrap();
        match renderer.with_context(|gl| unsafe { run_pipeline(gl, gl_res, elapsed) }) {
            Ok(tex) => self.blurred_tex = Some(tex),
            Err(e)  => error!("Aero: pipeline error: {e}"),
        }
    }

    pub fn blurred_background(&self) -> Option<Tex> { self.blurred_tex }

    /// Blit the full-screen blurred background with screen-blend tint + vignette.
    pub fn draw_background(&self, frame: &mut GlesFrame<'_, '_>) -> Result<(), GlesError> {
        let (gl_res, tex) = match (&self.gl, self.blurred_tex) {
            (Some(r), Some(t)) => (r, t),
            _ => return Ok(()),
        };
        frame.with_context(|gl| unsafe { blit_to_screen(gl, gl_res, tex); })
    }

    /// Draw a frosted glass panel at `rect` (logical coords, Y-down) before the
    /// window's surfaces are composited. Must be called inside a live GlesFrame,
    /// after `draw_background`, before `draw_render_elements`.
    pub fn draw_glass_rect(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        rect: Rectangle<i32, Logical>,
        output_h: i32,
    ) -> Result<(), GlesError> {
        let (gl_res, tex) = match (&self.gl, self.blurred_tex) {
            (Some(r), Some(t)) => (r, t),
            _ => return Ok(()),
        };

        // Convert logical Y-down → GL Y-up for scissor rect.
        let sx = rect.loc.x;
        let sy = output_h - rect.loc.y - rect.size.h;
        let sw = rect.size.w;
        let sh = rect.size.h;

        frame.with_context(|gl| unsafe {
            blit_glass_rect(gl, gl_res, tex, sx, sy, sw, sh);
        })
    }
}

// ── GL helpers ────────────────────────────────────────────────────────────────

unsafe fn compile_shader(gl: &Gl, ty: ffi::types::GLenum, src: &str) -> Result<ffi::types::GLuint, String> {
    let shader = gl.CreateShader(ty);
    if shader == 0 { return Err(format!("CreateShader(ty={ty:#x}) returned 0")); }
    gl.ShaderSource(
        shader, 1,
        &src.as_ptr() as *const *const u8 as *const *const ffi::types::GLchar,
        &(src.len() as ffi::types::GLint) as *const _,
    );
    gl.CompileShader(shader);
    let mut ok = 0i32;
    gl.GetShaderiv(shader, ffi::COMPILE_STATUS, &mut ok);
    if ok == 0 {
        let mut len = 0i32;
        gl.GetShaderiv(shader, ffi::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; len.max(0) as usize];
        gl.GetShaderInfoLog(shader, len, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _);
        gl.DeleteShader(shader);
        return Err(format!("shader: {}", String::from_utf8_lossy(&buf).trim_end_matches('\0')));
    }
    Ok(shader)
}

unsafe fn link_prog(gl: &Gl, vert: &str, frag: &str) -> Result<Prg, String> {
    let v = compile_shader(gl, ffi::VERTEX_SHADER, vert)?;
    let f = compile_shader(gl, ffi::FRAGMENT_SHADER, frag).map_err(|e| { gl.DeleteShader(v); e })?;
    let prog = gl.CreateProgram();
    gl.AttachShader(prog, v);
    gl.AttachShader(prog, f);
    gl.BindAttribLocation(prog, 0, b"a_position\0".as_ptr() as *const ffi::types::GLchar);
    gl.LinkProgram(prog);
    gl.DetachShader(prog, v); gl.DetachShader(prog, f);
    gl.DeleteShader(v); gl.DeleteShader(f);
    let mut ok = 0i32;
    gl.GetProgramiv(prog, ffi::LINK_STATUS, &mut ok);
    if ok == 0 {
        let mut len = 0i32;
        gl.GetProgramiv(prog, ffi::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; len.max(0) as usize];
        gl.GetProgramInfoLog(prog, len, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _);
        gl.DeleteProgram(prog);
        return Err(format!("link: {}", String::from_utf8_lossy(&buf).trim_end_matches('\0')));
    }
    Ok(prog)
}

unsafe fn make_fbo(gl: &Gl, w: u32, h: u32) -> Result<(Fbo, Tex), String> {
    let mut tex: Tex = 0;
    gl.GenTextures(1, &mut tex);
    gl.BindTexture(ffi::TEXTURE_2D, tex);
    gl.TexImage2D(
        ffi::TEXTURE_2D, 0, ffi::RGBA as ffi::types::GLint,
        w as _, h as _, 0, ffi::RGBA, ffi::UNSIGNED_BYTE, std::ptr::null(),
    );
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as _);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as _);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE as _);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE as _);
    gl.BindTexture(ffi::TEXTURE_2D, 0);

    let mut fbo: Fbo = 0;
    gl.GenFramebuffers(1, &mut fbo);
    gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo);
    gl.FramebufferTexture2D(ffi::FRAMEBUFFER, ffi::COLOR_ATTACHMENT0, ffi::TEXTURE_2D, tex, 0);
    let status = gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
    gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);

    if status != ffi::FRAMEBUFFER_COMPLETE {
        gl.DeleteTextures(1, &tex);
        gl.DeleteFramebuffers(1, &fbo);
        return Err(format!("FBO incomplete: {status:#x}"));
    }
    Ok((fbo, tex))
}

#[inline]
unsafe fn uloc(gl: &Gl, prog: Prg, name: &[u8]) -> Loc {
    gl.GetUniformLocation(prog, name.as_ptr() as *const ffi::types::GLchar)
}

unsafe fn init_gl(
    gl: &Gl,
    w: u32, h: u32,
    downsample: u32,
    blur_passes: usize,
    tint: [f32; 4],
    wallpaper_rgba: Option<&[u8]>,
    wallpaper_wh:   (u32, u32),
) -> Result<AeroGl, String> {
    let blur_w = (w / downsample.max(1)).max(1);
    let blur_h = (h / downsample.max(1)).max(1);

    let (scene_fbo,  scene_tex)  = make_fbo(gl, w, h)?;
    let (blur_a_fbo, blur_a_tex) = make_fbo(gl, blur_w, blur_h)?;
    let (blur_b_fbo, blur_b_tex) = make_fbo(gl, blur_w, blur_h)?;

    let bg_prg        = link_prog(gl, QUAD_VERT, BG_FRAG)?;
    let wallpaper_prg = link_prog(gl, QUAD_VERT, WALLPAPER_FRAG)?;
    let down_prg      = link_prog(gl, QUAD_VERT, DOWN_FRAG)?;
    let up_prg        = link_prog(gl, QUAD_VERT, UP_FRAG)?;
    let blit_prg      = link_prog(gl, QUAD_VERT, BLIT_FRAG)?;
    let glass_prg     = link_prog(gl, QUAD_VERT, GLASS_FRAG)?;

    let quad: [f32; 8] = [-1.0, -1.0,  1.0, -1.0,  -1.0, 1.0,  1.0, 1.0];
    let mut quad_vbo: Buf = 0;
    gl.GenBuffers(1, &mut quad_vbo);
    gl.BindBuffer(ffi::ARRAY_BUFFER, quad_vbo);
    gl.BufferData(
        ffi::ARRAY_BUFFER,
        std::mem::size_of_val(&quad) as ffi::types::GLsizeiptr,
        quad.as_ptr() as *const _,
        ffi::STATIC_DRAW,
    );
    gl.BindBuffer(ffi::ARRAY_BUFFER, 0);

    let wallpaper_tex: Tex = if let Some(rgba) = wallpaper_rgba {
        let (ww, wh) = wallpaper_wh;
        if ww > 0 && wh > 0 {
            let mut tex: Tex = 0;
            gl.GenTextures(1, &mut tex);
            gl.BindTexture(ffi::TEXTURE_2D, tex);
            gl.TexImage2D(
                ffi::TEXTURE_2D, 0, ffi::RGBA as _,
                ww as _, wh as _, 0, ffi::RGBA, ffi::UNSIGNED_BYTE,
                rgba.as_ptr() as *const _,
            );
            gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as _);
            gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as _);
            gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE as _);
            gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE as _);
            gl.BindTexture(ffi::TEXTURE_2D, 0);
            tex
        } else { 0 }
    } else { 0 };

    Ok(AeroGl {
        scene_fbo, scene_tex,
        blur_a_fbo, blur_a_tex,
        blur_b_fbo, blur_b_tex,
        bg_prg, wallpaper_prg, down_prg, up_prg, blit_prg, glass_prg,
        quad_vbo,
        bg_u_time:    uloc(gl, bg_prg,        b"u_time\0"),
        wp_u_tex:     uloc(gl, wallpaper_prg,  b"u_tex\0"),
        down_u_tex:   uloc(gl, down_prg,       b"u_tex\0"),
        down_u_hp:    uloc(gl, down_prg,       b"u_hp\0"),
        up_u_tex:     uloc(gl, up_prg,         b"u_tex\0"),
        up_u_hp:      uloc(gl, up_prg,         b"u_hp\0"),
        blit_u_tex:   uloc(gl, blit_prg,       b"u_tex\0"),
        blit_u_tint:  uloc(gl, blit_prg,       b"u_tint\0"),
        glass_u_tex:  uloc(gl, glass_prg,      b"u_tex\0"),
        glass_u_tint: uloc(gl, glass_prg,      b"u_tint\0"),
        w, h, blur_w, blur_h,
        blur_passes,
        tint,
        wallpaper_tex,
    })
}

#[inline]
unsafe fn draw_quad(gl: &Gl, vbo: Buf) {
    gl.BindBuffer(ffi::ARRAY_BUFFER, vbo);
    gl.EnableVertexAttribArray(0);
    gl.VertexAttribPointer(0, 2, ffi::FLOAT, ffi::FALSE, 0, std::ptr::null());
    gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);
    gl.DisableVertexAttribArray(0);
    gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
}

/// Run the full blur pipeline. Returns the GL texture ID of the blurred result.
///
/// Kawase quality fix: each pair of down/up passes uses a progressively larger
/// half-pixel offset so the kernel radius grows with each iteration — this gives
/// much better quality at low pass counts than a fixed offset.
unsafe fn run_pipeline(gl: &Gl, r: &AeroGl, time: f32) -> Tex {
    gl.Disable(ffi::BLEND);
    gl.Disable(ffi::SCISSOR_TEST);
    gl.ActiveTexture(ffi::TEXTURE0);

    // 1. Render scene source → scene_fbo
    gl.BindFramebuffer(ffi::FRAMEBUFFER, r.scene_fbo);
    gl.Viewport(0, 0, r.w as _, r.h as _);
    if r.wallpaper_tex != 0 {
        gl.UseProgram(r.wallpaper_prg);
        gl.BindTexture(ffi::TEXTURE_2D, r.wallpaper_tex);
        gl.Uniform1i(r.wp_u_tex, 0);
    } else {
        gl.UseProgram(r.bg_prg);
        gl.Uniform1f(r.bg_u_time, time);
    }
    draw_quad(gl, r.quad_vbo);

    // 2. First Kawase down-pass: scene → blur_a (half res)
    gl.BindFramebuffer(ffi::FRAMEBUFFER, r.blur_a_fbo);
    gl.Viewport(0, 0, r.blur_w as _, r.blur_h as _);
    gl.UseProgram(r.down_prg);
    gl.BindTexture(ffi::TEXTURE_2D, r.scene_tex);
    gl.Uniform1i(r.down_u_tex, 0);
    gl.Uniform2f(r.down_u_hp, 0.5 / r.w as f32, 0.5 / r.h as f32);
    draw_quad(gl, r.quad_vbo);

    // 3. Ping-pong passes — offset grows each pair for wider kernel
    for i in 0..r.blur_passes {
        let scale = (i / 2 + 1) as f32;
        let hp_x  = scale * 0.5 / r.blur_w as f32;
        let hp_y  = scale * 0.5 / r.blur_h as f32;

        let (src_tex, dst_fbo, prg, u_tex, u_hp) = if i % 2 == 0 {
            (r.blur_a_tex, r.blur_b_fbo, r.down_prg, r.down_u_tex, r.down_u_hp)
        } else {
            (r.blur_b_tex, r.blur_a_fbo, r.up_prg,   r.up_u_tex,   r.up_u_hp)
        };

        gl.BindFramebuffer(ffi::FRAMEBUFFER, dst_fbo);
        gl.UseProgram(prg);
        gl.BindTexture(ffi::TEXTURE_2D, src_tex);
        gl.Uniform1i(u_tex, 0);
        gl.Uniform2f(u_hp, hp_x, hp_y);
        draw_quad(gl, r.quad_vbo);
    }

    gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
    gl.BindTexture(ffi::TEXTURE_2D, 0);
    gl.UseProgram(0);

    // After N passes: even → last write was blur_a; odd → blur_b
    if r.blur_passes % 2 == 0 { r.blur_a_tex } else { r.blur_b_tex }
}

unsafe fn blit_to_screen(gl: &Gl, r: &AeroGl, tex: Tex) {
    gl.Disable(ffi::BLEND);
    gl.Disable(ffi::SCISSOR_TEST);

    gl.UseProgram(r.blit_prg);
    gl.ActiveTexture(ffi::TEXTURE0);
    gl.BindTexture(ffi::TEXTURE_2D, tex);
    gl.Uniform1i(r.blit_u_tex, 0);
    gl.Uniform4f(r.blit_u_tint, r.tint[0], r.tint[1], r.tint[2], r.tint[3]);
    draw_quad(gl, r.quad_vbo);

    gl.BindTexture(ffi::TEXTURE_2D, 0);
    gl.UseProgram(0);

    gl.Enable(ffi::BLEND);
    gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
    gl.Enable(ffi::SCISSOR_TEST);
}

/// Blit a frosted glass panel clipped to `(sx, sy, sw, sh)` in GL framebuffer
/// coordinates (Y=0 at bottom). Alpha = 0.82 lets a little background bleed
/// through at window edges.
unsafe fn blit_glass_rect(gl: &Gl, r: &AeroGl, tex: Tex, sx: i32, sy: i32, sw: i32, sh: i32) {
    gl.Enable(ffi::SCISSOR_TEST);
    gl.Scissor(sx, sy, sw, sh);

    gl.Enable(ffi::BLEND);
    gl.BlendFunc(ffi::SRC_ALPHA, ffi::ONE_MINUS_SRC_ALPHA);

    gl.UseProgram(r.glass_prg);
    gl.ActiveTexture(ffi::TEXTURE0);
    gl.BindTexture(ffi::TEXTURE_2D, tex);
    gl.Uniform1i(r.glass_u_tex, 0);
    // Stronger tint for the frosted panel
    gl.Uniform4f(r.glass_u_tint, r.tint[0], r.tint[1], r.tint[2], r.tint[3] * 2.2);
    draw_quad(gl, r.quad_vbo);

    gl.BindTexture(ffi::TEXTURE_2D, 0);
    gl.UseProgram(0);

    // Restore Smithay's expected blend state
    gl.Enable(ffi::BLEND);
    gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
    gl.Enable(ffi::SCISSOR_TEST);
}

// ── Wallpaper loader ──────────────────────────────────────────────────────────

pub fn load_wallpaper_rgba(
    path: &std::path::Path,
) -> Result<(Vec<u8>, u32, u32), Box<dyn std::error::Error>> {
    let img = image::open(path)?.into_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}
