//! Embedded libmpv video player (real audio, seek, volume, hardware decode).
//! mpv renders into an offscreen GL framebuffer which we blit into the egui
//! viewport rect from inside an `egui_glow` paint callback.
use eframe::glow::{self, HasContext};
use libmpv2::{
    render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType},
    Mpv,
};
use std::ffi::CString;

fn get_proc(_ctx: &(), name: &str) -> *mut std::ffi::c_void {
    match CString::new(name) {
        Ok(c) => unsafe { libc::dlsym(libc::RTLD_DEFAULT, c.as_ptr()) },
        Err(_) => std::ptr::null_mut(),
    }
}

pub struct MpvPlayer {
    mpv: Mpv,
    render: Option<RenderContext>,
    fbo: Option<glow::Framebuffer>,
    tex: Option<glow::Texture>,
    fb_w: i32,
    fb_h: i32,
}

// mpv handles/render context are used only on the render (main) thread; the
// raw pointers are not auto-Send. We never touch them from other threads.
unsafe impl Send for MpvPlayer {}
unsafe impl Sync for MpvPlayer {}

impl MpvPlayer {
    pub fn new() -> Result<Self, String> {
        let mpv = Mpv::with_initializer(|init| {
            // Render API requires the libmpv video output.
            let _ = init.set_property("vo", "libmpv");
            let _ = init.set_property("hwdec", "auto-safe");
            let _ = init.set_property("keep-open", "yes");
            let _ = init.set_property("osc", false);
            let _ = init.set_property("volume", 0.0);
            let _ = init.set_property("mute", true);
            // Enable mpv logging to help diagnose file loading issues
            let _ = init.set_property("log-file", "/Users/rishiwork/Desktop/old-projects/smart-file-manager/mpv_log.txt");
            Ok(())
        })
        .map_err(|e| format!("mpv init failed: {e:?}"))?;
        Ok(Self {
            mpv,
            render: None,
            fbo: None,
            tex: None,
            fb_w: 0,
            fb_h: 0,
        })
    }

    pub fn load(&self, path: &str) -> Result<(), String> {
        // Escape the path with double quotes in case it contains spaces.
        // mpv_command_string requires paths with spaces to be wrapped in double quotes.
        let escaped_path = format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""));
        self.mpv.command("loadfile", &[&escaped_path])
            .map_err(|e| format!("loadfile failed: {:?}", e))?;
        let _ = self.mpv.set_property("volume", 0.0);
        let _ = self.mpv.set_property("mute", true);
        Ok(())
    }

    /// Unload the current file and silence audio without destroying the render
    /// context (freeing the context off the GL thread crashes the app).
    pub fn stop(&self) {
        let _ = self.mpv.command("stop", &[]);
    }

    pub fn toggle_pause(&self) {
        let p = self.paused();
        let _ = self.mpv.set_property("pause", !p);
    }

    pub fn paused(&self) -> bool {
        self.mpv.get_property("pause").unwrap_or(false)
    }

    pub fn seek_absolute(&self, secs: f64) {
        let _ = self.mpv.command("seek", &[&format!("{secs:.3}"), "absolute"]);
    }

    pub fn set_volume(&self, v: f64) {
        let _ = self.mpv.set_property("volume", v.clamp(0.0, 130.0));
    }

    pub fn set_muted(&self, m: bool) {
        let _ = self.mpv.set_property("mute", m);
    }

    pub fn muted(&self) -> bool {
        self.mpv.get_property("mute").unwrap_or(false)
    }

    pub fn set_rotation(&self, degrees: i64) {
        let _ = self.mpv.set_property("video-rotate", degrees);
    }

    pub fn volume(&self) -> f64 {
        self.mpv.get_property("volume").unwrap_or(100.0)
    }

    pub fn position(&self) -> f64 {
        self.mpv.get_property("time-pos").unwrap_or(0.0)
    }

    pub fn duration(&self) -> f64 {
        self.mpv.get_property("duration").unwrap_or(0.0)
    }

    /// Create the render context once (needs a current GL context → first paint).
    pub fn ensure_render(&mut self, egui_ctx: &eframe::egui::Context) -> bool {
        if self.render.is_some() {
            return true;
        }
        let repaint = egui_ctx.clone();
        let params = vec![
            RenderParam::ApiType(RenderParamApiType::OpenGl),
            RenderParam::InitParams(OpenGLInitParams {
                get_proc_address: get_proc,
                ctx: (),
            }),
        ];
        match RenderContext::new(unsafe { self.mpv.ctx.as_mut() }, params) {
            Ok(mut r) => {
                r.set_update_callback(move || repaint.request_repaint());
                self.render = Some(r);
                true
            }
            Err(_) => false,
        }
    }

    /// Render the current frame into our FBO and blit it to `dst` (framebuffer
    /// pixels, origin bottom-left). `vw`/`vh` is the offscreen render size.
    pub fn paint(&mut self, gl: &glow::Context, vw: i32, vh: i32, dst: [i32; 4]) {
        if self.render.is_none() || vw <= 0 || vh <= 0 {
            return;
        }
        unsafe {
            if self.fbo.is_none() || self.fb_w != vw || self.fb_h != vh {
                if let Some(t) = self.tex.take() {
                    gl.delete_texture(t);
                }
                if let Some(f) = self.fbo.take() {
                    gl.delete_framebuffer(f);
                }
                let tex = gl.create_texture().ok();
                let fbo = gl.create_framebuffer().ok();
                if let (Some(tex), Some(fbo)) = (tex, fbo) {
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D, 0, glow::RGBA8 as i32, vw, vh, 0,
                        glow::RGBA, glow::UNSIGNED_BYTE, None,
                    );
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0,
                    );
                    self.tex = Some(tex);
                    self.fbo = Some(fbo);
                    self.fb_w = vw;
                    self.fb_h = vh;
                }
            }
            let Some(fbo) = self.fbo else { return };
            let render = self.render.as_ref().unwrap();
            let fbo_id = fbo.0.get() as i32;
            let _ = render.render::<()>(fbo_id, vw, vh, true);

            // Query active draw framebuffer to preserve egui's post-processing/MSAA FBO state
            let draw_fbo = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);

            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw_fbo);
            // Flip Y by swapping the destination top/bottom.
            gl.blit_framebuffer(
                0, 0, vw, vh,
                dst[0], dst[3], dst[2], dst[1],
                glow::COLOR_BUFFER_BIT, glow::LINEAR,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, draw_fbo);
        }
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        // C-API constraint: the render context MUST be freed/dropped BEFORE
        // the Mpv instance itself is terminated/dropped, otherwise libmpv aborts/crashes.
        self.render.take();
    }
}
