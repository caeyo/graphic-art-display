use std::rc::Rc;

use glow::HasContext;

pub struct Renderer {
    gl: Rc<glow::Context>,
    compute_program: glow::Program,
    display_program: glow::Program,
    output_texture: glow::Texture,
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
    _pad0: f32,
    resolution: [f32; 2],
    _pad1: [f32; 2],
}

impl Renderer {
    pub fn new(
        gl: Rc<glow::Context>,
        width: u32,
        height: u32,
        compute_source: &str,
        display_vert: &str,
        display_frag: &str,
    ) -> Result<Self, String> {
        unsafe {
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }

        let compute_program = compile_compute(&gl, compute_source)?;
        let display_program = compile_display(&gl, display_vert, display_frag)?;

        let output_texture = create_output_texture(&gl, width, height);
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
            compute_program,
            display_program,
            output_texture,
            vao,
            ubo,
            width,
            height,
        })
    }

    pub fn destroy(self) {
        unsafe {
            self.gl.delete_program(self.compute_program);
            self.gl.delete_program(self.display_program);
            self.gl.delete_texture(self.output_texture);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_buffer(self.ubo);
        }
    }

    pub fn reload_compute(&mut self, source: &str) -> Result<(), String> {
        let program = compile_compute(&self.gl, source)?;
        unsafe {
            self.gl.delete_program(self.compute_program);
        }
        self.compute_program = program;
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        unsafe {
            self.gl.delete_texture(self.output_texture);
        }
        self.output_texture = create_output_texture(&self.gl, width, height);
    }

    pub fn draw(&self, time: f32, delta_time: f32, frame: u32) {
        let uniforms = FrameUniforms {
            time,
            delta_time,
            frame,
            _pad0: 0.0,
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

            self.gl.use_program(Some(self.compute_program));
            self.gl.bind_image_texture(
                0,
                Some(self.output_texture),
                0,
                false,
                0,
                glow::WRITE_ONLY,
                glow::RGBA8,
            );

            let groups_x = (self.width + 15) / 16;
            let groups_y = (self.height + 15) / 16;
            self.gl.dispatch_compute(groups_x, groups_y, 1);
            self.gl.memory_barrier(glow::SHADER_IMAGE_ACCESS_BARRIER_BIT);

            self.gl.use_program(Some(self.display_program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.output_texture));
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
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
