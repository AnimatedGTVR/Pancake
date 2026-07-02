/// Aero frosted-glass rendering pipeline.
///
/// Each frame, a procedural animated Aero gradient is rendered into an offscreen
/// FBO, then blurred with a dual-Kawase multi-pass filter and tinted. The blurred
/// result is composited as the full-screen desktop background before any windows
/// are drawn — giving the Aero glass desktop look.
///
/// ## Pipeline (per-frame)
///
///  ┌──────────────────────────────────────────────────────────────┐
///  │ 1. Render animated gradient → scene_fbo  (full res)          │
///  │ 2. Kawase down-pass         → blur_a_fbo (½ res)             │
///  │ 3. N blur passes (ping-pong blur_a ↔ blur_b at ½ res)        │
///  │ 4. draw_background() blits result to screen + Aero tint       │
///  └──────────────────────────────────────────────────────────────┘
use smithay::backend::renderer::gles::{ffi, GlesError, GlesFrame, GlesRenderer};
use std::time::Instant;
use tracing::error;

use crate::config::Config;

/// Default tint layered over the blurred gradient (RGBA, linear).
const DEFAULT_TINT: [f32; 4] = [0.55, 0.70, 1.00, 0.18];

/// Default number of additional blur ping-pong passes.
const DEFAULT_PASSES: usize = 4;

/// Default downscale factor for the blur FBOs.
const DEFAULT_DOWNSAMPLE: u32 = 2;

// ── Vertex shader — used by every pass ───────────────────────────────────────

const QUAD_VERT: &str = r#"
#version 100
attribute vec2 a_position;
varying   vec2 v_uv;
void main() {
    v_uv        = a_position * 0.5 + 0.5;
    gl_Position = vec4(a_position, 0.0, 1.0);
}
"#;

// ── Procedural Aero gradient (rendered to scene_fbo each frame) ───────────────

const BG_FRAG: &str = r#"
#version 100
precision mediump float;
uniform float u_time;
varying vec2  v_uv;
void main() {
    float wave = sin(v_uv.x * 3.14159 + u_time * 0.5) * 0.04;
    float grad = clamp(v_uv.y * 0.75 + 0.15 + wave, 0.0, 1.0);
    vec3 sky   = vec3(0.50, 0.72, 1.00);
    vec3 deep  = vec3(0.05, 0.12, 0.38);
    vec3 col   = mix(deep, sky, grad);
    float shim = sin(v_uv.x * 48.0 + u_time * 1.8) * 0.012 + 1.0;
    gl_FragColor = vec4(col * shim, 1.0);
}
"#;

// ── Dual-Kawase passes ────────────────────────────────────────────────────────

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

// ── Final blit: blurred texture + Aero tint ──────────────────────────────────

const BLIT_FRAG: &str = r#"
#version 100
precision mediump float;
uniform sampler2D u_tex;
uniform vec4      u_tint;
varying vec2      v_uv;
void main() {
    vec4 blur    = texture2D(u_tex, v_uv);
    gl_FragColor = blur + vec4(u_tint.rgb * u_tint.a, 0.0);
}
"#;

// ── GL handle type aliases ────────────────────────────────────────────────────

type Gl  = ffi::Gles2;
type Tex = ffi::types::GLuint;
type Fbo = ffi::types::GLuint;
type Prg = ffi::types::GLuint;
type Buf = ffi::types::GLuint;
type Loc = ffi::types::GLint;

// ── GPU resource bundle ───────────────────────────────────────────────────────

struct AeroGl {
    // Source FBO (full resolution): procedural gradient is rendered here
    scene_fbo: Fbo,
    scene_tex: Tex,
    // Ping-pong blur FBOs (half resolution)
    blur_a_fbo: Fbo,
    blur_a_tex: Tex,
    blur_b_fbo: Fbo,
    blur_b_tex: Tex,
    // Shader programs
    bg_prg:   Prg,
    down_prg: Prg,
    up_prg:   Prg,
    blit_prg: Prg,
    // Full-screen quad VBO (NDC triangle strip, 4 × vec2)
    quad_vbo: Buf,
    // Cached uniform locations
    bg_u_time:   Loc,
    down_u_tex:  Loc,
    down_u_hp:   Loc,
    up_u_tex:    Loc,
    up_u_hp:     Loc,
    blit_u_tex:  Loc,
    blit_u_tint: Loc,
    // Output and blur dimensions
    w: u32,
    h: u32,
    blur_w: u32,
    blur_h: u32,
    // Runtime blur settings (baked in at FBO creation time)
    blur_passes: usize,
    tint: [f32; 4],
}

// ── Public AeroRenderer ───────────────────────────────────────────────────────

pub struct AeroRenderer {
    gl: Option<AeroGl>,
    start: Instant,
    output_size: (u32, u32),
    blurred_tex: Option<Tex>,
    // Active config values — changing these clears gl to trigger re-init
    cfg_passes: usize,
    cfg_downsample: u32,
    cfg_tint: [f32; 4],
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
        }
    }
}

impl AeroRenderer {
    /// Apply config values. If any blur parameter changed, invalidates the GPU
    /// resources so they are re-created on the next `begin_frame` call.
    pub fn apply_config(&mut self, config: &Config) {
        let changed = self.cfg_passes != config.blur_passes
            || self.cfg_downsample != config.blur_downsample
            || self.cfg_tint != config.tint;

        self.cfg_passes = config.blur_passes;
        self.cfg_downsample = config.blur_downsample;
        self.cfg_tint = config.tint;

        if changed {
            self.gl = None;
            self.blurred_tex = None;
        }
    }

    /// Called once per frame, before the Smithay render frame starts.
    ///
    /// Lazily allocates GL resources and runs the blur pipeline into offscreen
    /// FBOs using a surfaceless GL context. The result is stored in
    /// `self.blurred_tex` and made available via [`blurred_background`].
    pub fn begin_frame(&mut self, renderer: &mut GlesRenderer, width: u32, height: u32) {
        if self.output_size != (width, height) {
            self.output_size = (width, height);
            self.gl = None;
            self.blurred_tex = None;
        }

        if width == 0 || height == 0 {
            return;
        }

        let elapsed = self.start.elapsed().as_secs_f32();

        // Lazy init: allocate FBOs, compile shaders, build quad VBO on first use.
        if self.gl.is_none() {
            let (w, h, passes, downsample, tint) = (
                width,
                height,
                self.cfg_passes,
                self.cfg_downsample,
                self.cfg_tint,
            );
            match renderer.with_context(|gl| unsafe {
                init_gl(gl, w, h, downsample, passes, tint)
            }) {
                Ok(Ok(res)) => self.gl = Some(res),
                Ok(Err(msg)) => {
                    error!("Aero: GL init failed: {msg}");
                    return;
                }
                Err(e) => {
                    error!("Aero: context error on init: {e}");
                    return;
                }
            }
        }

        let gl_res = self.gl.as_ref().unwrap();
        match renderer.with_context(|gl| unsafe { run_pipeline(gl, gl_res, elapsed) }) {
            Ok(tex) => self.blurred_tex = Some(tex),
            Err(e) => error!("Aero: pipeline error: {e}"),
        }
    }

    /// Returns the GL texture ID of the latest blur result, or `None` on the
    /// first frame before the pipeline has run.
    pub fn blurred_background(&self) -> Option<Tex> {
        self.blurred_tex
    }

    /// Draws the blurred background as a fullscreen quad.
    ///
    /// Must be called inside a live Smithay `GlesFrame`, before windows are
    /// drawn. Restores BLEND and SCISSOR_TEST state on exit so subsequent
    /// Smithay draw calls work normally.
    pub fn draw_background(&self, frame: &mut GlesFrame<'_, '_>) -> Result<(), GlesError> {
        let (gl_res, tex) = match (&self.gl, self.blurred_tex) {
            (Some(r), Some(t)) => (r, t),
            _ => return Ok(()),
        };

        frame.with_context(|gl| unsafe {
            blit_to_screen(gl, gl_res, tex);
        })
    }
}

// ── GL helpers — all unsafe, all called within a valid GL context ─────────────

unsafe fn compile_shader(gl: &Gl, ty: ffi::types::GLenum, src: &str) -> Result<ffi::types::GLuint, String> {
    let shader = gl.CreateShader(ty);
    if shader == 0 {
        return Err(format!("CreateShader(ty={ty:#x}) returned 0"));
    }
    gl.ShaderSource(
        shader,
        1,
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
        return Err(format!(
            "shader compile: {}",
            String::from_utf8_lossy(&buf).trim_end_matches('\0')
        ));
    }
    Ok(shader)
}

unsafe fn link_prog(gl: &Gl, vert: &str, frag: &str) -> Result<Prg, String> {
    let v = compile_shader(gl, ffi::VERTEX_SHADER, vert)?;
    let f = compile_shader(gl, ffi::FRAGMENT_SHADER, frag).map_err(|e| {
        gl.DeleteShader(v);
        e
    })?;
    let prog = gl.CreateProgram();
    gl.AttachShader(prog, v);
    gl.AttachShader(prog, f);
    // Lock a_position to attribute slot 0 before linking so every program
    // shares the same slot and we can use a single draw_quad helper.
    gl.BindAttribLocation(prog, 0, b"a_position\0".as_ptr() as *const ffi::types::GLchar);
    gl.LinkProgram(prog);
    gl.DetachShader(prog, v);
    gl.DetachShader(prog, f);
    gl.DeleteShader(v);
    gl.DeleteShader(f);
    let mut ok = 0i32;
    gl.GetProgramiv(prog, ffi::LINK_STATUS, &mut ok);
    if ok == 0 {
        let mut len = 0i32;
        gl.GetProgramiv(prog, ffi::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; len.max(0) as usize];
        gl.GetProgramInfoLog(prog, len, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _);
        gl.DeleteProgram(prog);
        return Err(format!(
            "link: {}",
            String::from_utf8_lossy(&buf).trim_end_matches('\0')
        ));
    }
    Ok(prog)
}

/// Allocate a RGBA texture + FBO pair at `w × h`.
unsafe fn make_fbo(gl: &Gl, w: u32, h: u32) -> Result<(Fbo, Tex), String> {
    let mut tex: Tex = 0;
    gl.GenTextures(1, &mut tex);
    gl.BindTexture(ffi::TEXTURE_2D, tex);
    gl.TexImage2D(
        ffi::TEXTURE_2D,
        0,
        ffi::RGBA as ffi::types::GLint,
        w as ffi::types::GLsizei,
        h as ffi::types::GLsizei,
        0,
        ffi::RGBA,
        ffi::UNSIGNED_BYTE,
        std::ptr::null(),
    );
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as ffi::types::GLint);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as ffi::types::GLint);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE as ffi::types::GLint);
    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE as ffi::types::GLint);
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

/// Get a uniform location by null-terminated byte-string name.
#[inline]
unsafe fn uloc(gl: &Gl, prog: Prg, name: &[u8]) -> Loc {
    gl.GetUniformLocation(prog, name.as_ptr() as *const ffi::types::GLchar)
}

/// Allocate all GPU resources: FBOs, shaders, quad VBO, and cache uniforms.
unsafe fn init_gl(
    gl: &Gl,
    w: u32,
    h: u32,
    downsample: u32,
    blur_passes: usize,
    tint: [f32; 4],
) -> Result<AeroGl, String> {
    let blur_w = (w / downsample.max(1)).max(1);
    let blur_h = (h / downsample.max(1)).max(1);

    let (scene_fbo, scene_tex)   = make_fbo(gl, w, h)?;
    let (blur_a_fbo, blur_a_tex) = make_fbo(gl, blur_w, blur_h)?;
    let (blur_b_fbo, blur_b_tex) = make_fbo(gl, blur_w, blur_h)?;

    let bg_prg   = link_prog(gl, QUAD_VERT, BG_FRAG)?;
    let down_prg = link_prog(gl, QUAD_VERT, DOWN_FRAG)?;
    let up_prg   = link_prog(gl, QUAD_VERT, UP_FRAG)?;
    let blit_prg = link_prog(gl, QUAD_VERT, BLIT_FRAG)?;

    // Full-screen NDC triangle strip: BL, BR, TL, TR
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

    Ok(AeroGl {
        scene_fbo, scene_tex,
        blur_a_fbo, blur_a_tex,
        blur_b_fbo, blur_b_tex,
        bg_prg, down_prg, up_prg, blit_prg,
        quad_vbo,
        bg_u_time:   uloc(gl, bg_prg,   b"u_time\0"),
        down_u_tex:  uloc(gl, down_prg, b"u_tex\0"),
        down_u_hp:   uloc(gl, down_prg, b"u_hp\0"),
        up_u_tex:    uloc(gl, up_prg,   b"u_tex\0"),
        up_u_hp:     uloc(gl, up_prg,   b"u_hp\0"),
        blit_u_tex:  uloc(gl, blit_prg, b"u_tex\0"),
        blit_u_tint: uloc(gl, blit_prg, b"u_tint\0"),
        w, h, blur_w, blur_h,
        blur_passes,
        tint,
    })
}

/// Draw the full-screen quad with attribute slot 0 bound to `a_position`.
#[inline]
unsafe fn draw_quad(gl: &Gl, vbo: Buf) {
    gl.BindBuffer(ffi::ARRAY_BUFFER, vbo);
    gl.EnableVertexAttribArray(0);
    gl.VertexAttribPointer(0, 2, ffi::FLOAT, ffi::FALSE, 0, std::ptr::null());
    gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);
    gl.DisableVertexAttribArray(0);
    gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
}

/// Run the blur pipeline and return the GL texture ID of the blurred result.
unsafe fn run_pipeline(gl: &Gl, r: &AeroGl, time: f32) -> Tex {
    gl.Disable(ffi::BLEND);
    gl.Disable(ffi::SCISSOR_TEST);
    gl.ActiveTexture(ffi::TEXTURE0);

    // 1. Render animated gradient → scene_fbo (full res)
    gl.BindFramebuffer(ffi::FRAMEBUFFER, r.scene_fbo);
    gl.Viewport(0, 0, r.w as ffi::types::GLsizei, r.h as ffi::types::GLsizei);
    gl.UseProgram(r.bg_prg);
    gl.Uniform1f(r.bg_u_time, time);
    draw_quad(gl, r.quad_vbo);

    // 2. Downsample scene → blur_a (half res) — first Kawase down-pass
    gl.BindFramebuffer(ffi::FRAMEBUFFER, r.blur_a_fbo);
    gl.Viewport(0, 0, r.blur_w as ffi::types::GLsizei, r.blur_h as ffi::types::GLsizei);
    gl.UseProgram(r.down_prg);
    gl.BindTexture(ffi::TEXTURE_2D, r.scene_tex);
    gl.Uniform1i(r.down_u_tex, 0);
    gl.Uniform2f(r.down_u_hp, 0.5 / r.w as f32, 0.5 / r.h as f32);
    draw_quad(gl, r.quad_vbo);

    // 3. Additional passes ping-pong between blur_a and blur_b
    for i in 0..r.blur_passes {
        // Even passes: blur_a → blur_b (down); odd passes: blur_b → blur_a (up)
        let (src_tex, dst_fbo, prg, u_tex, u_hp) = if i % 2 == 0 {
            (r.blur_a_tex, r.blur_b_fbo, r.down_prg, r.down_u_tex, r.down_u_hp)
        } else {
            (r.blur_b_tex, r.blur_a_fbo, r.up_prg, r.up_u_tex, r.up_u_hp)
        };
        gl.BindFramebuffer(ffi::FRAMEBUFFER, dst_fbo);
        gl.UseProgram(prg);
        gl.BindTexture(ffi::TEXTURE_2D, src_tex);
        gl.Uniform1i(u_tex, 0);
        gl.Uniform2f(u_hp, 0.5 / r.blur_w as f32, 0.5 / r.blur_h as f32);
        draw_quad(gl, r.quad_vbo);
    }

    gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
    gl.BindTexture(ffi::TEXTURE_2D, 0);
    gl.UseProgram(0);

    // Pass i=0 writes blur_b, i=1 writes blur_a, i=2 writes blur_b, ...
    // After N passes: N even → last write was blur_a; N odd → blur_b.
    if r.blur_passes % 2 == 0 {
        r.blur_a_tex
    } else {
        r.blur_b_tex
    }
}

/// Blit `tex` to the currently-bound default framebuffer (the screen) as a
/// fullscreen quad, adding the Aero tint from `r.tint`. Restores BLEND and SCISSOR_TEST.
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

    // Restore the state Smithay's frame machinery expects.
    gl.Enable(ffi::BLEND);
    gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
    gl.Enable(ffi::SCISSOR_TEST);
}
