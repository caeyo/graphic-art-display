use std::rc::Rc;

use glow::HasContext;

use crate::module::{ModuleKind, ModuleState};

pub struct ModuleRenderConfig {
    pub kind: ModuleKind,
    pub workgroup: [u32; 3],
    pub state: ModuleState,
}

pub struct Renderer {
    gl: Rc<glow::Context>,
    art_program: glow::Program,
    art_kind: ModuleKind,
    workgroup: [u32; 3],
    state_mode: ModuleState,
    display_program: glow::Program,
    output_texture: glow::Texture,
    output_fbo: glow::Framebuffer,
    state_textures: Option<(glow::Texture, glow::Texture)>,
    read_state_a: bool,
    vao: glow::VertexArray,
    ubo: glow::Buffer,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct FrameUniforms {
    time: f32,
    delta_time: f32,
    frame: u32,
    seed: u32,
    resolution: [f32; 2],
    _pad1: [f32; 2],
}

impl Renderer {
    pub fn new(
        gl: Rc<glow::Context>,
        width: u32,
        height: u32,
        art_source: &str,
        art_config: &ModuleRenderConfig,
        display_vert: &str,
        display_frag: &str,
    ) -> Result<Self, String> {
        unsafe {
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }

        let art_program = compile_art(&gl, art_source, art_config.kind)?;
        let display_program = compile_display(&gl, display_vert, display_frag)?;

        let output_texture = create_output_texture(&gl, width, height);
        let output_fbo = create_output_fbo(&gl, output_texture);
        let state_textures = if module_needs_state(art_config.state) {
            Some(create_state_pair(&gl, width, height, art_config.state))
        } else {
            None
        };

        let vao = unsafe {
            let vao = gl.create_vertex_array().map_err(map_gl_err)?;
            gl.bind_vertex_array(Some(vao));
            vao
        };

        let ubo = unsafe {
            let buffer = gl.create_buffer().map_err(map_gl_err)?;
            gl.bind_buffer(glow::UNIFORM_BUFFER, Some(buffer));
            gl.buffer_data_size(
                glow::UNIFORM_BUFFER,
                std::mem::size_of::<FrameUniforms>() as i32,
                glow::DYNAMIC_DRAW,
            );
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(buffer));
            buffer
        };

        Ok(Self {
            gl,
            art_program,
            art_kind: art_config.kind,
            workgroup: art_config.workgroup,
            state_mode: art_config.state,
            display_program,
            output_texture,
            output_fbo,
            state_textures,
            read_state_a: true,
            vao,
            ubo,
            width,
            height,
        })
    }

    pub fn destroy(self) {
        unsafe {
            self.gl.delete_program(self.art_program);
            self.gl.delete_program(self.display_program);
            self.gl.delete_texture(self.output_texture);
            self.gl.delete_framebuffer(self.output_fbo);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_buffer(self.ubo);
            if let Some((a, b)) = self.state_textures {
                self.gl.delete_texture(a);
                self.gl.delete_texture(b);
            }
        }
    }

    pub fn load_module(
        &mut self,
        source: &str,
        config: &ModuleRenderConfig,
    ) -> Result<(), String> {
        let program = compile_art(&self.gl, source, config.kind)?;
        unsafe {
            self.gl.delete_program(self.art_program);
        }
        self.art_program = program;
        self.art_kind = config.kind;
        self.workgroup = config.workgroup;
        self.state_mode = config.state;

        if module_needs_state(config.state) {
            if let Some((a, b)) = self.state_textures.take() {
                unsafe {
                    self.gl.delete_texture(a);
                    self.gl.delete_texture(b);
                }
            }
            self.state_textures = Some(create_state_pair(
                &self.gl,
                self.width,
                self.height,
                config.state,
            ));
            self.read_state_a = true;
        } else if let Some((a, b)) = self.state_textures.take() {
            unsafe {
                self.gl.delete_texture(a);
                self.gl.delete_texture(b);
            }
        }

        Ok(())
    }

    pub fn reset_module_state(&mut self) {
        self.read_state_a = true;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        unsafe {
            self.gl.delete_texture(self.output_texture);
            self.gl.delete_framebuffer(self.output_fbo);
        }
        self.output_texture = create_output_texture(&self.gl, width, height);
        self.output_fbo = create_output_fbo(&self.gl, self.output_texture);

        if module_needs_state(self.state_mode) {
            if let Some((a, b)) = self.state_textures.take() {
                unsafe {
                    self.gl.delete_texture(a);
                    self.gl.delete_texture(b);
                }
            }
            self.state_textures = Some(create_state_pair(
                &self.gl,
                width,
                height,
                self.state_mode,
            ));
            self.read_state_a = true;
        }
    }

    pub fn draw(&mut self, time: f32, delta_time: f32, frame: u32, seed: u32) {
        let uniforms = FrameUniforms {
            time,
            delta_time,
            frame,
            seed,
            resolution: [self.width as f32, self.height as f32],
            _pad1: [0.0; 2],
        };

        unsafe {
            self.gl.viewport(0, 0, self.width as i32, self.height as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);

            self.gl.bind_buffer(glow::UNIFORM_BUFFER, Some(self.ubo));
            let bytes = std::slice::from_raw_parts(
                &uniforms as *const _ as *const u8,
                std::mem::size_of::<FrameUniforms>(),
            );
            self.gl.buffer_sub_data_u8_slice(glow::UNIFORM_BUFFER, 0, bytes);

            match self.art_kind {
                ModuleKind::Compute => self.draw_compute(frame),
                ModuleKind::Fragment => self.draw_fragment_art(),
            }

            self.gl.use_program(Some(self.display_program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.output_texture));
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    unsafe fn draw_compute(&mut self, frame: u32) {
        self.gl.use_program(Some(self.art_program));
        self.gl.bind_image_texture(
            0,
            Some(self.output_texture),
            0,
            false,
            0,
            glow::WRITE_ONLY,
            glow::RGBA8,
        );

        if self.state_mode != ModuleState::None && self.state_textures.is_none() {
            self.state_textures = Some(create_state_pair(
                &self.gl,
                self.width,
                self.height,
                self.state_mode,
            ));
        }

        let steps = if self.state_mode == ModuleState::PingPong && frame > 0 {
            3
        } else {
            1
        };

        let groups_x = (self.width + self.workgroup[0] - 1) / self.workgroup[0];
        let groups_y = (self.height + self.workgroup[1] - 1) / self.workgroup[1];

        for _ in 0..steps {
            if self.state_mode != ModuleState::None {
                let Some((tex_a, tex_b)) = self.state_textures else {
                    return;
                };
                let (read_tex, write_tex) = if self.read_state_a {
                    (tex_a, tex_b)
                } else {
                    (tex_b, tex_a)
                };
                let state_format = state_image_format(self.state_mode);

                self.gl.bind_image_texture(
                    1,
                    Some(read_tex),
                    0,
                    false,
                    0,
                    glow::READ_ONLY,
                    state_format,
                );
                self.gl.bind_image_texture(
                    2,
                    Some(write_tex),
                    0,
                    false,
                    0,
                    glow::WRITE_ONLY,
                    state_format,
                );
            }

            self.gl.dispatch_compute(groups_x, groups_y, self.workgroup[2]);
            self.gl
                .memory_barrier(glow::SHADER_IMAGE_ACCESS_BARRIER_BIT);

            if self.state_mode != ModuleState::None {
                self.read_state_a = !self.read_state_a;
            }
        }
    }

    unsafe fn draw_fragment_art(&self) {
        self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.output_fbo));
        self.gl.viewport(0, 0, self.width as i32, self.height as i32);
        self.gl.use_program(Some(self.art_program));
        self.gl.bind_vertex_array(Some(self.vao));
        self.gl.draw_arrays(glow::TRIANGLES, 0, 3);
        self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }
}

fn create_output_texture(gl: &glow::Context, width: u32, height: u32) -> glow::Texture {
    unsafe {
        let texture = gl.create_texture().expect("create texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, width as i32, height as i32);
        texture
    }
}

fn create_output_fbo(gl: &glow::Context, texture: glow::Texture) -> glow::Framebuffer {
    unsafe {
        let fbo = gl.create_framebuffer().expect("create framebuffer");
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(texture),
            0,
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        fbo
    }
}

fn module_needs_state(state: ModuleState) -> bool {
    matches!(state, ModuleState::PingPong | ModuleState::Trail)
}

fn state_image_format(state: ModuleState) -> u32 {
    match state {
        ModuleState::PingPong | ModuleState::Trail => glow::RGBA32F,
        ModuleState::None => glow::RGBA8,
    }
}

fn create_state_pair(
    gl: &glow::Context,
    width: u32,
    height: u32,
    state: ModuleState,
) -> (glow::Texture, glow::Texture) {
    (
        create_state_texture(gl, width, height, state),
        create_state_texture(gl, width, height, state),
    )
}

fn create_state_texture(gl: &glow::Context, width: u32, height: u32, state: ModuleState) -> glow::Texture {
    let internal_format = state_image_format(state);
    unsafe {
        let texture = gl.create_texture().expect("create state texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.tex_storage_2d(
            glow::TEXTURE_2D,
            1,
            internal_format,
            width as i32,
            height as i32,
        );
        texture
    }
}

fn compile_art(
    gl: &glow::Context,
    source: &str,
    kind: ModuleKind,
) -> Result<glow::Program, String> {
    match kind {
        ModuleKind::Compute => compile_compute(gl, source),
        ModuleKind::Fragment => compile_fragment(gl, source),
    }
}

fn compile_compute(gl: &glow::Context, source: &str) -> Result<glow::Program, String> {
    unsafe {
        let shader = gl
            .create_shader(glow::COMPUTE_SHADER)
            .map_err(map_gl_err)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            return Err(gl.get_shader_info_log(shader));
        }

        let program = gl.create_program().map_err(map_gl_err)?;
        gl.attach_shader(program, shader);
        gl.link_program(program);
        gl.detach_shader(program, shader);
        gl.delete_shader(shader);

        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }

        Ok(program)
    }
}

fn compile_fragment(gl: &glow::Context, source: &str) -> Result<glow::Program, String> {
    const VERT: &str = include_str!("../shaders/display.vert");
    unsafe {
        let vert = gl.create_shader(glow::VERTEX_SHADER).map_err(map_gl_err)?;
        gl.shader_source(vert, VERT);
        gl.compile_shader(vert);
        if !gl.get_shader_compile_status(vert) {
            return Err(gl.get_shader_info_log(vert));
        }

        let frag = gl.create_shader(glow::FRAGMENT_SHADER).map_err(map_gl_err)?;
        gl.shader_source(frag, source);
        gl.compile_shader(frag);
        if !gl.get_shader_compile_status(frag) {
            gl.delete_shader(vert);
            return Err(gl.get_shader_info_log(frag));
        }

        let program = gl.create_program().map_err(map_gl_err)?;
        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }

        Ok(program)
    }
}

fn compile_display(
    gl: &glow::Context,
    vert_source: &str,
    frag_source: &str,
) -> Result<glow::Program, String> {
    unsafe {
        let vert = gl.create_shader(glow::VERTEX_SHADER).map_err(map_gl_err)?;
        gl.shader_source(vert, vert_source);
        gl.compile_shader(vert);
        if !gl.get_shader_compile_status(vert) {
            return Err(gl.get_shader_info_log(vert));
        }

        let frag = gl.create_shader(glow::FRAGMENT_SHADER).map_err(map_gl_err)?;
        gl.shader_source(frag, frag_source);
        gl.compile_shader(frag);
        if !gl.get_shader_compile_status(frag) {
            gl.delete_shader(vert);
            return Err(gl.get_shader_info_log(frag));
        }

        let program = gl.create_program().map_err(map_gl_err)?;
        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }

        let texture_loc = gl.get_uniform_location(program, "uTexture");
        gl.use_program(Some(program));
        if let Some(loc) = texture_loc {
            gl.uniform_1_i32(Some(&loc), 0);
        }

        Ok(program)
    }
}

fn map_gl_err(err: String) -> String {
    err
}
