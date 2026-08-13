using Silk.NET.OpenGLES;

namespace Pancake.Render;

// Port of src/render/aero.rs. The GLES draw calls that were Rust's
// `unsafe` surface here become Silk.NET.OpenGLES calls -- ordinary,
// non-unsafe C# (Silk.NET wraps the raw GL entry points), confirming the
// earlier readmenow.md call that this file's `unsafe` was pure GL usage,
// not something that needed Cn. Cn's job ends at handing over a live GL
// context (see Pancake.Cn.GpuDevice.GetProcAddress); everything below is
// plain C#.
internal sealed class AeroGl
{
    public uint SceneFbo, SceneTex;
    public uint BlurAFbo, BlurATex;
    public uint BlurBFbo, BlurBTex;

    public uint BgPrg, WallpaperPrg, DownPrg, UpPrg, BlitPrg, GlassPrg;

    public uint QuadVbo;

    public int BgUTime;
    public int WpUTex;
    public int DownUTex, DownUHp;
    public int UpUTex, UpUHp;
    public int BlitUTex, BlitUTint;
    public int GlassUTex, GlassUTint;

    public uint W, H;
    public uint BlurW, BlurH;
    public int BlurPasses;
    public float[] Tint = new float[4];
    public uint WallpaperTex;
}

public sealed class AeroRenderer
{
    private static readonly float[] DefaultTint = { 0.52f, 0.68f, 1.00f, 0.16f };
    private const int DefaultPasses = 4;
    private const uint DefaultDownsample = 2;

    private AeroGl? _gl;
    private readonly DateTime _start = DateTime.UtcNow;
    private (uint W, uint H) _outputSize;
    private uint? _blurredTex;

    private int _cfgPasses = DefaultPasses;
    private uint _cfgDownsample = DefaultDownsample;
    private float[] _cfgTint = (float[])DefaultTint.Clone();
    private string? _cfgWallpaper;
    private byte[]? _wallpaperRgba;
    private (uint W, uint H) _wallpaperWh;

    public void ApplyConfig(int blurPasses, uint blurDownsample, float[] tint, string? wallpaper)
    {
        var changed = _cfgPasses != blurPasses || _cfgDownsample != blurDownsample || !_cfgTint.AsSpan().SequenceEqual(tint);

        _cfgPasses = blurPasses;
        _cfgDownsample = blurDownsample;
        _cfgTint = (float[])tint.Clone();

        if (_cfgWallpaper != wallpaper)
        {
            _cfgWallpaper = wallpaper;
            _wallpaperRgba = null;
            if (wallpaper is not null)
            {
                try
                {
                    var (pixels, w, h) = Wallpaper.LoadRgba(wallpaper);
                    _wallpaperRgba = pixels;
                    _wallpaperWh = (w, h);
                }
                catch (Exception)
                {
                    // Matches the Rust side's "log + fall back to no wallpaper".
                    _wallpaperRgba = null;
                }
            }
            changed = true;
        }

        if (changed)
        {
            _gl = null;
            _blurredTex = null;
        }
    }

    public void BeginFrame(GL gl, uint width, uint height)
    {
        if (_outputSize != (width, height))
        {
            _outputSize = (width, height);
            _gl = null;
            _blurredTex = null;
        }

        if (width == 0 || height == 0) return;

        var elapsed = (float)(DateTime.UtcNow - _start).TotalSeconds;

        _gl ??= InitGl(gl, width, height, _cfgDownsample, _cfgPasses, _cfgTint, _wallpaperRgba, _wallpaperWh);

        _blurredTex = RunPipeline(gl, _gl, elapsed);
    }

    public uint? BlurredBackground() => _blurredTex;

    public void DrawBackground(GL gl)
    {
        if (_gl is null || _blurredTex is not { } tex) return;
        BlitToScreen(gl, _gl, tex);
    }

    public void DrawGlassRect(GL gl, int sx, int sy, int sw, int sh)
    {
        if (_gl is null || _blurredTex is not { } tex) return;
        BlitGlassRect(gl, _gl, tex, sx, sy, sw, sh);
    }

    // ── GL helpers ───────────────────────────────────────────────────

    private static uint CompileShader(GL gl, GLEnum type, string src)
    {
        var shader = gl.CreateShader(type);
        gl.ShaderSource(shader, src);
        gl.CompileShader(shader);

        var ok = gl.GetShader(shader, GLEnum.CompileStatus);
        if (ok == 0)
        {
            var log = gl.GetShaderInfoLog(shader);
            gl.DeleteShader(shader);
            throw new InvalidOperationException($"shader compile failed: {log}");
        }
        return shader;
    }

    private static uint LinkProg(GL gl, string vert, string frag)
    {
        var v = CompileShader(gl, GLEnum.VertexShader, vert);
        uint f;
        try
        {
            f = CompileShader(gl, GLEnum.FragmentShader, frag);
        }
        catch
        {
            gl.DeleteShader(v);
            throw;
        }

        var prog = gl.CreateProgram();
        gl.AttachShader(prog, v);
        gl.AttachShader(prog, f);
        gl.BindAttribLocation(prog, 0, "a_position");
        gl.LinkProgram(prog);
        gl.DeleteShader(v);
        gl.DeleteShader(f);

        var ok = gl.GetProgram(prog, GLEnum.LinkStatus);
        if (ok == 0)
        {
            var log = gl.GetProgramInfoLog(prog);
            throw new InvalidOperationException($"program link failed: {log}");
        }
        return prog;
    }

    private static unsafe (uint Fbo, uint Tex) MakeFbo(GL gl, uint w, uint h)
    {
        var tex = gl.GenTextures(1);
        gl.BindTexture(GLEnum.Texture2D, tex);
        gl.TexImage2D(GLEnum.Texture2D, 0, (int)GLEnum.Rgba, w, h, 0, GLEnum.Rgba, GLEnum.UnsignedByte, null);
        gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureMinFilter, (int)GLEnum.Linear);
        gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureMagFilter, (int)GLEnum.Linear);
        gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureWrapS, (int)GLEnum.ClampToEdge);
        gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureWrapT, (int)GLEnum.ClampToEdge);
        gl.BindTexture(GLEnum.Texture2D, 0);

        var fbo = gl.GenFramebuffers(1);
        gl.BindFramebuffer(GLEnum.Framebuffer, fbo);
        gl.FramebufferTexture2D(GLEnum.Framebuffer, GLEnum.ColorAttachment0, GLEnum.Texture2D, tex, 0);
        var status = gl.CheckFramebufferStatus(GLEnum.Framebuffer);
        gl.BindFramebuffer(GLEnum.Framebuffer, 0);

        if (status != GLEnum.FramebufferComplete)
            throw new InvalidOperationException($"framebuffer incomplete: 0x{(int)status:X}");

        return (fbo, tex);
    }

    private static unsafe AeroGl InitGl(GL gl, uint w, uint h, uint downsample, int passes, float[] tint,
        byte[]? wpRgba, (uint W, uint H) wpWh)
    {
        var r = new AeroGl
        {
            W = w,
            H = h,
            BlurW = Math.Max(1u, w / Math.Max(1u, downsample)),
            BlurH = Math.Max(1u, h / Math.Max(1u, downsample)),
            BlurPasses = passes,
            Tint = (float[])tint.Clone(),
        };

        (r.SceneFbo, r.SceneTex) = MakeFbo(gl, r.W, r.H);
        (r.BlurAFbo, r.BlurATex) = MakeFbo(gl, r.BlurW, r.BlurH);
        (r.BlurBFbo, r.BlurBTex) = MakeFbo(gl, r.BlurW, r.BlurH);

        r.BgPrg = LinkProg(gl, Shaders.QuadVert, Shaders.BgFrag);
        r.WallpaperPrg = LinkProg(gl, Shaders.QuadVert, Shaders.WallpaperFrag);
        r.DownPrg = LinkProg(gl, Shaders.QuadVert, Shaders.DownFrag);
        r.UpPrg = LinkProg(gl, Shaders.QuadVert, Shaders.UpFrag);
        r.BlitPrg = LinkProg(gl, Shaders.QuadVert, Shaders.BlitFrag);
        r.GlassPrg = LinkProg(gl, Shaders.QuadVert, Shaders.GlassFrag);

        r.BgUTime = gl.GetUniformLocation(r.BgPrg, "u_time");
        r.WpUTex = gl.GetUniformLocation(r.WallpaperPrg, "u_tex");
        r.DownUTex = gl.GetUniformLocation(r.DownPrg, "u_tex");
        r.DownUHp = gl.GetUniformLocation(r.DownPrg, "u_hp");
        r.UpUTex = gl.GetUniformLocation(r.UpPrg, "u_tex");
        r.UpUHp = gl.GetUniformLocation(r.UpPrg, "u_hp");
        r.BlitUTex = gl.GetUniformLocation(r.BlitPrg, "u_tex");
        r.BlitUTint = gl.GetUniformLocation(r.BlitPrg, "u_tint");
        r.GlassUTex = gl.GetUniformLocation(r.GlassPrg, "u_tex");
        r.GlassUTint = gl.GetUniformLocation(r.GlassPrg, "u_tint");

        float[] quad = { -1, -1, 1, -1, -1, 1, 1, 1 };
        r.QuadVbo = gl.GenBuffers(1);
        gl.BindBuffer(GLEnum.ArrayBuffer, r.QuadVbo);
        gl.BufferData<float>(GLEnum.ArrayBuffer, quad, GLEnum.StaticDraw);
        gl.BindBuffer(GLEnum.ArrayBuffer, 0);

        if (wpRgba is not null && wpWh.W > 0 && wpWh.H > 0)
        {
            var tex = gl.GenTextures(1);
            gl.BindTexture(GLEnum.Texture2D, tex);
            gl.TexImage2D<byte>(GLEnum.Texture2D, 0, (int)GLEnum.Rgba, wpWh.W, wpWh.H, 0, GLEnum.Rgba, GLEnum.UnsignedByte, wpRgba);
            gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureMinFilter, (int)GLEnum.Linear);
            gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureMagFilter, (int)GLEnum.Linear);
            gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureWrapS, (int)GLEnum.ClampToEdge);
            gl.TexParameter(GLEnum.Texture2D, GLEnum.TextureWrapT, (int)GLEnum.ClampToEdge);
            gl.BindTexture(GLEnum.Texture2D, 0);
            r.WallpaperTex = tex;
        }

        return r;
    }

    private static void DrawQuad(GL gl, uint vbo)
    {
        gl.BindBuffer(GLEnum.ArrayBuffer, vbo);
        gl.EnableVertexAttribArray(0);
        gl.VertexAttribPointer(0, 2, GLEnum.Float, false, 0, 0);
        gl.DrawArrays(GLEnum.TriangleStrip, 0, 4);
        gl.BindBuffer(GLEnum.ArrayBuffer, 0);
    }

    private static uint RunPipeline(GL gl, AeroGl r, float time)
    {
        gl.Disable(GLEnum.Blend);
        gl.Disable(GLEnum.ScissorTest);
        gl.ActiveTexture(GLEnum.Texture0);

        // Scene pass: background (wallpaper if set, else aurora gradient).
        gl.BindFramebuffer(GLEnum.Framebuffer, r.SceneFbo);
        gl.Viewport(0, 0, r.W, r.H);
        if (r.WallpaperTex != 0)
        {
            gl.UseProgram(r.WallpaperPrg);
            gl.BindTexture(GLEnum.Texture2D, r.WallpaperTex);
            gl.Uniform1(r.WpUTex, 0);
        }
        else
        {
            gl.UseProgram(r.BgPrg);
            gl.Uniform1(r.BgUTime, time);
        }
        DrawQuad(gl, r.QuadVbo);

        // First Kawase down-pass: scene -> blur_a (half-res).
        gl.BindFramebuffer(GLEnum.Framebuffer, r.BlurAFbo);
        gl.Viewport(0, 0, r.BlurW, r.BlurH);
        gl.UseProgram(r.DownPrg);
        gl.BindTexture(GLEnum.Texture2D, r.SceneTex);
        gl.Uniform1(r.DownUTex, 0);
        gl.Uniform2(r.DownUHp, 1.0f / r.BlurW, 1.0f / r.BlurH);
        DrawQuad(gl, r.QuadVbo);

        var srcTex = r.BlurATex;
        var dstFbo = r.BlurBFbo;
        var dstTex = r.BlurBTex;

        for (var i = 0; i < r.BlurPasses; i++)
        {
            gl.BindFramebuffer(GLEnum.Framebuffer, dstFbo);
            gl.Viewport(0, 0, r.BlurW, r.BlurH);
            gl.UseProgram(r.UpPrg);
            gl.BindTexture(GLEnum.Texture2D, srcTex);
            gl.Uniform1(r.UpUTex, 0);
            gl.Uniform2(r.UpUHp, 1.0f / r.BlurW, 1.0f / r.BlurH);
            DrawQuad(gl, r.QuadVbo);

            (srcTex, dstTex) = (dstTex, srcTex);
            dstFbo = dstFbo == r.BlurAFbo ? r.BlurBFbo : r.BlurAFbo;
        }

        gl.BindFramebuffer(GLEnum.Framebuffer, 0);
        gl.BindTexture(GLEnum.Texture2D, 0);
        return srcTex;
    }

    private static void BlitToScreen(GL gl, AeroGl r, uint tex)
    {
        gl.Disable(GLEnum.Blend);
        gl.Disable(GLEnum.ScissorTest);

        gl.UseProgram(r.BlitPrg);
        gl.ActiveTexture(GLEnum.Texture0);
        gl.BindTexture(GLEnum.Texture2D, tex);
        gl.Uniform1(r.BlitUTex, 0);
        gl.Uniform4(r.BlitUTint, r.Tint[0], r.Tint[1], r.Tint[2], r.Tint[3]);
        DrawQuad(gl, r.QuadVbo);
        gl.BindTexture(GLEnum.Texture2D, 0);

        gl.Enable(GLEnum.Blend);
        gl.BlendFunc(GLEnum.One, GLEnum.OneMinusSrcAlpha);
        gl.Enable(GLEnum.ScissorTest);
    }

    private static void BlitGlassRect(GL gl, AeroGl r, uint tex, int sx, int sy, int sw, int sh)
    {
        gl.Enable(GLEnum.ScissorTest);
        gl.Scissor(sx, sy, (uint)Math.Max(0, sw), (uint)Math.Max(0, sh));

        gl.Enable(GLEnum.Blend);
        gl.BlendFunc(GLEnum.SrcAlpha, GLEnum.OneMinusSrcAlpha);

        gl.UseProgram(r.GlassPrg);
        gl.ActiveTexture(GLEnum.Texture0);
        gl.BindTexture(GLEnum.Texture2D, tex);
        gl.Uniform1(r.GlassUTex, 0);
        gl.Uniform4(r.GlassUTint, r.Tint[0], r.Tint[1], r.Tint[2], r.Tint[3]);
        DrawQuad(gl, r.QuadVbo);
        gl.BindTexture(GLEnum.Texture2D, 0);

        gl.Enable(GLEnum.Blend);
        gl.BlendFunc(GLEnum.One, GLEnum.OneMinusSrcAlpha);
        gl.Enable(GLEnum.ScissorTest);
    }
}
