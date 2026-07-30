use crate::config::{RenderConfig, ScaleMode};
use crate::video::shader::{ShaderFrameBlendMode, ShaderPrecision};
use crate::video::{calc_win_height, calc_win_width, new_scaled_rect, shader};
use gl::types::{GLenum, GLint};
use sdl2::rect::Rect;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{Sdl, VideoSubsystem};
use std::ffi::CStr;
use std::ptr;
#[cfg(feature = "frontend-modern")]
use std::sync::Arc;

pub struct GlBackend {
    gl: GLSetup,
    #[cfg(feature = "frontend-modern")]
    egui: egui_sdl2::EguiGlow,
    shader_program: u32,
    frame_texture_id: u32,
    prev_frame_texture_id: u32,
    vao: u32,
    vbo: u32,
    uniform_locations: UniformLocations,
    game_rect: Rect,
    shader_frame_blend_mode: ShaderFrameBlendMode,
    prev_buffer: Box<[u8]>,
}

impl GlBackend {
    pub fn new(sdl: &Sdl, game_rect: Rect, config: &RenderConfig) -> Result<Self, String> {
        let gl = create_gl_with_fallback(sdl, game_rect.width(), game_rect.height())?;
        #[cfg(feature = "frontend-modern")]
        let egui = egui_sdl2::EguiGlow::new(&gl.window, gl.glow.clone(), None, true);

        unsafe {
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        let mut obj = Self {
            #[cfg(feature = "frontend-modern")]
            egui,
            shader_program: 0,
            frame_texture_id: 0,
            vao: 0,
            vbo: 0,
            uniform_locations: Default::default(),
            prev_frame_texture_id: 0,
            shader_frame_blend_mode: config.gl.shader_frame_blend_mode,
            prev_buffer: Box::new([]),
            game_rect,
            gl,
        };
        obj.load_shader(
            &config.gl.shader_name,
            config.gl.shader_frame_blend_mode,
            config.gl.shader_precision,
        )?;

        Ok(obj)
    }

    /// Closes the window and returns true when main window is closed.
    pub fn close_window(&mut self, _id: u32) -> bool {
        true
    }

    pub fn update_config(&mut self, config: &RenderConfig) {
        self.load_shader(
            &config.gl.shader_name,
            config.gl.shader_frame_blend_mode,
            config.gl.shader_precision,
        )
        .unwrap();
    }

    fn draw_quad(&self) {
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }
    }

    pub fn draw_menu(&mut self, buffer: &[u8]) {
        // Uniforms apply to the bound program, and egui's is still bound after it
        // painted the previous frame.
        unsafe {
            gl::UseProgram(self.shader_program);
        }

        self.uniform_locations
            .send_frame_blend_mode(ShaderFrameBlendMode::None);

        self.draw_buffer(buffer);

        self.uniform_locations
            .send_frame_blend_mode(self.shader_frame_blend_mode);
    }

    pub fn set_scale(&mut self, scale: u32, mode: ScaleMode) -> Result<(), String> {
        self.gl
            .window
            .set_size(calc_win_width(scale), calc_win_height(scale))
            .map_err(|e| e.to_string())?;
        self.gl.window.set_position(
            sdl2::video::WindowPos::Centered,
            sdl2::video::WindowPos::Centered,
        );
        self.update_game_rect(mode);

        Ok(())
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, scale_mode: ScaleMode) {
        if fullscreen {
            self.gl
                .window
                .set_fullscreen(sdl2::video::FullscreenType::Desktop)
                .unwrap();
        } else {
            self.gl
                .window
                .set_fullscreen(sdl2::video::FullscreenType::Off)
                .unwrap();
        };
        self.update_game_rect(scale_mode);
    }

    /// Recomputes the game rect for the window's current size (e.g. after an
    /// orientation change), without changing the window size.
    pub fn handle_resize(&mut self, mode: ScaleMode) {
        self.update_game_rect(mode);
    }

    pub fn draw_buffer(&mut self, buffer: &[u8]) {
        let width = RenderConfig::WIDTH;
        let height = RenderConfig::HEIGHT;

        unsafe {
            // egui_glow leaves its premultiplied-alpha blend func behind.
            #[cfg(feature = "frontend-modern")]
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.shader_program);

            self.uniform_locations.send_image();
            self.uniform_locations
                .send_in_resolution(RenderConfig::WIDTH as f32, RenderConfig::HEIGHT as f32);
            self.uniform_locations.send_out_resolution(
                self.game_rect.width() as f32,
                self.game_rect.height() as f32,
            );
            self.uniform_locations
                .send_origin(self.game_rect.x as f32, self.game_rect.y as f32);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.frame_texture_id);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1); // needed for UNSIGNED_SHORT_5_6_5

            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                width as i32,
                height as i32,
                gl::RGB,
                gl::UNSIGNED_SHORT_5_6_5,
                buffer.as_ptr() as *const _,
            );

            if self.shader_frame_blend_mode != ShaderFrameBlendMode::None {
                self.uniform_locations.send_prev_image();

                gl::ActiveTexture(gl::TEXTURE1);
                gl::BindTexture(gl::TEXTURE_2D, self.prev_frame_texture_id);
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1); // needed for UNSIGNED_SHORT_5_6_5

                gl::TexSubImage2D(
                    gl::TEXTURE_2D,
                    0,
                    0,
                    0,
                    width as i32,
                    height as i32,
                    gl::RGB,
                    gl::UNSIGNED_SHORT_5_6_5,
                    self.prev_buffer.as_ptr() as *const _,
                );

                if buffer.len() == self.prev_buffer.len() {
                    ptr::copy_nonoverlapping(
                        buffer.as_ptr(),
                        self.prev_buffer.as_mut_ptr(),
                        buffer.len(),
                    );
                }
            }

            gl::Viewport(
                self.game_rect.x,
                self.game_rect.y,
                self.game_rect.width() as i32,
                self.game_rect.height() as i32,
            );

            self.draw_quad();
        }
    }

    pub fn show(&self) {
        self.gl.window.gl_swap_window();
    }

    /// Returns whether egui consumed the event.
    #[cfg(feature = "frontend-modern")]
    pub fn egui_on_event(&mut self, event: &sdl2::event::Event) -> bool {
        self.egui.on_event(&self.gl.window, event).consumed
    }

    /// Runs and paints egui over the frame already drawn; [`Self::show`] presents.
    #[cfg(feature = "frontend-modern")]
    pub fn render_egui(&mut self, run_ui: &mut dyn FnMut(&egui_sdl2::egui::Context)) {
        self.egui.run(run_ui);
        self.egui.paint();
    }

    #[cfg(feature = "frontend-modern")]
    pub fn egui_repaint_delay(&self) -> std::time::Duration {
        self.egui.repaint_delay()
    }

    #[cfg(feature = "frontend-modern")]
    pub fn destroy_egui(&mut self) {
        self.egui.destroy();
    }

    /// Loads and initializes shaders + GPU resources
    pub fn load_shader(
        &mut self,
        name: &str,
        frame_blend_mode: ShaderFrameBlendMode,
        precision: ShaderPrecision,
    ) -> Result<(), String> {
        let program = shader::load_shader_program(name, &self.gl, precision)?;
        self.shader_frame_blend_mode = frame_blend_mode;

        unsafe {
            let mut vao = 0;
            let mut vbo = 0;
            let vertices: [f32; 16] = [
                -1.0, -1.0, 0.0, 0.0, 1.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            ];

            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                size_of_val(&vertices) as isize,
                vertices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            let stride = 4 * size_of::<f32>() as i32;
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                1,
                2,
                gl::FLOAT,
                gl::FALSE,
                stride,
                (2 * size_of::<f32>()) as *const _,
            );
            gl::EnableVertexAttribArray(1);

            // Unbind
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            self.frame_texture_id = create_texture(
                RenderConfig::WIDTH as i32,
                RenderConfig::HEIGHT as i32,
                gl::RGB565,
                gl::RGB,
                gl::UNSIGNED_SHORT_5_6_5,
            );
            self.vao = vao;
            self.vbo = vbo;
        }

        self.shader_program = program;

        // Uniforms below go to the bound program; nothing binds it after linking.
        unsafe {
            gl::UseProgram(program);
        }

        self.uniform_locations = get_uniform_locations(self.shader_program);
        self.uniform_locations.send_image();
        self.uniform_locations
            .send_frame_blend_mode(frame_blend_mode);

        if frame_blend_mode != ShaderFrameBlendMode::None {
            self.uniform_locations.send_prev_image();
            self.prev_frame_texture_id = create_texture(
                RenderConfig::WIDTH as i32,
                RenderConfig::HEIGHT as i32,
                gl::RGB565,
                gl::RGB,
                gl::UNSIGNED_SHORT_5_6_5,
            );
            self.prev_buffer =
                vec![
                    0;
                    RenderConfig::WIDTH * RenderConfig::HEIGHT * core::ppu::PPU_BYTES_PER_PIXEL
                ]
                .into_boxed_slice();
        } else if frame_blend_mode == ShaderFrameBlendMode::None && !self.prev_buffer.is_empty() {
            self.prev_buffer = Box::new([]);
        }

        Ok(())
    }

    fn update_game_rect(&mut self, scale_mode: ScaleMode) {
        let (win_width, win_height) = self.gl.window.size();
        self.game_rect = new_scaled_rect(scale_mode, win_width, win_height);
    }
}

fn get_uniform_locations(program: u32) -> UniformLocations {
    unsafe {
        UniformLocations {
            image: gl::GetUniformLocation(program, c"image".as_ptr() as *const _),
            input_resolution: gl::GetUniformLocation(
                program,
                c"input_resolution".as_ptr() as *const _,
            ),
            out_resolution: gl::GetUniformLocation(
                program,
                c"output_resolution".as_ptr() as *const _,
            ),
            origin: gl::GetUniformLocation(program, c"origin".as_ptr() as *const _),
            frame_blending_mode: gl::GetUniformLocation(
                program,
                c"frame_blending_mode".as_ptr() as *const _,
            ),
            prev_image: gl::GetUniformLocation(program, c"previous_image".as_ptr() as *const _),
        }
    }
}

#[derive(Default)]
struct UniformLocations {
    pub image: i32,
    pub input_resolution: i32,
    pub out_resolution: i32,
    pub origin: i32,
    pub frame_blending_mode: i32,
    pub prev_image: i32,
}

impl UniformLocations {
    pub fn send_image(&self) {
        unsafe {
            gl::Uniform1i(self.image, 0);
        }
    }

    pub fn send_prev_image(&self) {
        unsafe {
            gl::Uniform1i(self.prev_image, 1);
        }
    }

    pub fn send_in_resolution(&self, w: f32, h: f32) {
        unsafe {
            gl::Uniform2f(self.input_resolution, w, h);
        }
    }

    pub fn send_out_resolution(&self, w: f32, h: f32) {
        unsafe {
            gl::Uniform2f(self.out_resolution, w, h);
        }
    }

    pub fn send_origin(&self, x: f32, y: f32) {
        unsafe {
            gl::Uniform2f(self.origin, x, y);
        }
    }

    pub fn send_frame_blend_mode(&self, mode: ShaderFrameBlendMode) {
        let mode = mode as i32;

        unsafe {
            gl::Uniform1i(self.frame_blending_mode, mode);
        }
    }
}

pub struct GLSetup {
    _video_subsystem: VideoSubsystem,
    _context: GLContext,
    pub window: Window,
    pub shader_version: &'static str,
    pub gles: Option<Gles>,
    /// Same context as the `gl` crate's, loaded through glow for egui.
    #[cfg(feature = "frontend-modern")]
    pub glow: Arc<egui_sdl2::egui_glow::glow::Context>,
}

#[derive(Debug)]
pub struct Gles {
    pub is_version_2: bool,
    // chosen precisions: "highp" or "mediump"
    pub vertex_precision: &'static str,
    pub fragment_precision: &'static str,
}

pub fn create_gl_with_fallback(sdl: &Sdl, width: u32, height: u32) -> Result<GLSetup, String> {
    let video = sdl.video()?;

    let attempts = [
        (GLProfile::Core, 3, 3, "#version 330 core", false, false),
        (GLProfile::Core, 3, 2, "#version 150 core", false, false),
        (GLProfile::GLES, 3, 0, "#version 300 es", true, false),
        (GLProfile::GLES, 2, 0, "#version 100", true, true),
    ];

    for &(profile, major, minor, shader_version, use_gles, is_gles2) in &attempts {
        log::info!("Trying GL profile: {profile:?} {major}.{minor}");

        video.gl_attr().set_context_profile(profile);
        video.gl_attr().set_context_version(major, minor);

        let window = match video
            .window("oxGBC GL", width, height)
            .position_centered()
            .opengl()
            .resizable()
            .build()
        {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Failed to create GL window: {e}");
                continue;
            }
        };

        let context = match window.gl_create_context() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to create GL context: {e}");
                continue;
            }
        };

        gl::load_with(|s| video.gl_get_proc_address(s) as *const _);
        #[cfg(feature = "frontend-modern")]
        let glow = Arc::new(unsafe {
            egui_sdl2::egui_glow::glow::Context::from_loader_function(|s| {
                video.gl_get_proc_address(s) as *const _
            })
        });

        let gles = if use_gles && is_gles2 {
            let mut frag_prec = "mediump";
            let mut vert_prec = "mediump";

            unsafe {
                let mut range: [GLint; 2] = [0, 0];
                let mut precision_val: GLint = 0;
                gl::GetShaderPrecisionFormat(
                    gl::FRAGMENT_SHADER,
                    gl::HIGH_FLOAT,
                    range.as_mut_ptr(),
                    &mut precision_val,
                );

                if precision_val > 0 {
                    frag_prec = "highp";
                }

                let mut v_range: [GLint; 2] = [0, 0];
                let mut v_precision_val: GLint = 0;
                gl::GetShaderPrecisionFormat(
                    gl::VERTEX_SHADER,
                    gl::HIGH_FLOAT,
                    v_range.as_mut_ptr(),
                    &mut v_precision_val,
                );

                if v_precision_val > 0 {
                    vert_prec = "highp";
                }
            }

            Some(Gles {
                is_version_2: true,
                vertex_precision: vert_prec,
                fragment_precision: frag_prec,
            })
        } else if use_gles {
            // GLES3 / desktop: vertex shaders generally have highp by default (no need to emit),
            // but we'll still set values so caller can inspect them if needed.
            Some(Gles {
                is_version_2: false,
                vertex_precision: "highp",
                fragment_precision: "highp",
            })
        } else {
            None
        };

        print_gl_versions();
        log::info!("Using {profile:?} {major}.{minor} -> {shader_version}, GLES2: {gles:?}");

        return Ok(GLSetup {
            _context: context,
            _video_subsystem: video,
            #[cfg(feature = "frontend-modern")]
            glow,
            window,
            shader_version,
            gles,
        });
    }

    Err("No suitable GL/GLES context found!".to_string())
}

fn print_gl_versions() {
    unsafe {
        let version = CStr::from_ptr(gl::GetString(gl::VERSION) as *const _)
            .to_str()
            .unwrap();
        let shading_lang = CStr::from_ptr(gl::GetString(gl::SHADING_LANGUAGE_VERSION) as *const _)
            .to_str()
            .unwrap();

        log::info!("OpenGL version: {version}. GLSL version: {shading_lang}");
    }
}

pub fn create_texture(
    w: i32,
    h: i32,
    inner_format: GLenum,
    format: GLenum,
    data_type: GLenum,
) -> u32 {
    unsafe {
        let mut texture = 0;
        gl::GenTextures(1, &mut texture);
        gl::BindTexture(gl::TEXTURE_2D, texture);

        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            inner_format as GLint,
            w,
            h,
            0,
            format,
            data_type,
            ptr::null(),
        );

        texture
    }
}
