namespace Pancake.Render;

// Shader sources ported verbatim from src/render/aero.rs -- GLSL ES 100
// is portable text, nothing Rust-specific about it.
internal static class Shaders
{
    internal const string QuadVert = """
        #version 100
        attribute vec2 a_position;
        varying   vec2 v_uv;
        void main() {
            v_uv        = a_position * 0.5 + 0.5;
            gl_Position = vec4(a_position, 0.0, 1.0);
        }
        """;

    internal const string BgFrag = """
        #version 100
        precision mediump float;
        uniform float u_time;
        varying vec2  v_uv;

        void main() {
            float t  = u_time * 0.10;

            vec2 p1 = vec2(0.28 + sin(t * 0.71) * 0.22,  0.62 + cos(t * 0.53) * 0.18);
            vec2 p2 = vec2(0.72 + cos(t * 0.43) * 0.18,  0.38 + sin(t * 0.67) * 0.22);
            vec2 p3 = vec2(0.50 + sin(t * 0.89) * 0.14,  0.20 + cos(t * 0.78) * 0.14);

            float d1 = dot(v_uv - p1, v_uv - p1);
            float d2 = dot(v_uv - p2, v_uv - p2);
            float d3 = dot(v_uv - p3, v_uv - p3);
            float o1 = exp(-d1 * 5.0);
            float o2 = exp(-d2 * 6.5);
            float o3 = exp(-d3 * 8.0);

            vec3 base   = vec3(0.03, 0.06, 0.18);
            vec3 azure  = vec3(0.22, 0.52, 1.00);
            vec3 teal   = vec3(0.08, 0.68, 0.82);
            vec3 violet = vec3(0.42, 0.28, 0.88);

            vec3 col = base
                     + azure  * o1 * 0.75
                     + teal   * o2 * 0.55
                     + violet * o3 * 0.42;

            col += azure * v_uv.y * 0.06;

            float shimmer = sin(v_uv.x * 55.0 + t * 4.0) * 0.009 + 1.0;

            gl_FragColor = vec4(clamp(col * shimmer, 0.0, 1.0), 1.0);
        }
        """;

    internal const string DownFrag = """
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
        """;

    internal const string UpFrag = """
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
        """;

    internal const string WallpaperFrag = """
        #version 100
        precision mediump float;
        uniform sampler2D u_tex;
        varying vec2 v_uv;
        void main() {
            gl_FragColor = texture2D(u_tex, vec2(v_uv.x, 1.0 - v_uv.y));
        }
        """;

    internal const string BlitFrag = """
        #version 100
        precision mediump float;
        uniform sampler2D u_tex;
        uniform vec4      u_tint;
        varying vec2      v_uv;
        void main() {
            vec4 blur = texture2D(u_tex, v_uv);

            vec3 screen = 1.0 - (1.0 - blur.rgb) * (1.0 - u_tint.rgb * u_tint.a);
            vec3 result = mix(blur.rgb, screen, 0.72);

            vec2 vig = v_uv * 2.0 - 1.0;
            float vignette = 1.0 - dot(vig, vig) * 0.08;

            gl_FragColor = vec4(result * vignette, 1.0);
        }
        """;

    internal const string GlassFrag = """
        #version 100
        precision mediump float;
        uniform sampler2D u_tex;
        uniform vec4      u_tint;
        varying vec2      v_uv;
        void main() {
            vec4 blur = texture2D(u_tex, v_uv);

            vec3 screen = 1.0 - (1.0 - blur.rgb) * (1.0 - u_tint.rgb * u_tint.a);
            vec3 result = mix(blur.rgb, screen, 0.85);

            gl_FragColor = vec4(result, 0.82);
        }
        """;
}
