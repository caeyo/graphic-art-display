mod args;
mod module;
mod renderer;

use std::ffi::CString;
use std::rc::Rc;
use std::time::Instant;

use args::Args;
use clap::Parser;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, Version};
use glutin::context::PossiblyCurrentGlContext;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin_winit::{DisplayBuilder, GlWindow};
use module::{discover_modules, LoadedModule};
use raw_window_handle::HasWindowHandle;
use renderer::{ModuleRenderConfig, Renderer};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const DISPLAY_VERT: &str = include_str!("../shaders/display.vert");
const DISPLAY_FRAG: &str = include_str!("../shaders/display.frag");

fn generate_seed() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let lo = nanos as u32;
    let hi = (nanos >> 32) as u32;
    lo ^ hi.rotate_left(17) ^ hi.wrapping_mul(0x9E37_79B9)
}

struct App {
    args: Args,
    modules: Vec<LoadedModule>,
    module_index: usize,
    window: Option<Window>,
    gl_context: Option<glutin::context::PossiblyCurrentContext>,
    gl_surface: Option<glutin::surface::Surface<glutin::surface::WindowSurface>>,
    renderer: Option<Renderer>,
    start: Instant,
    last_frame: Instant,
    frame: u32,
    seed: u32,
    exiting: bool,
}

impl App {
    fn new(args: Args) -> Self {
        let modules_path = args.modules_path();
        let modules = discover_modules(&modules_path).unwrap_or_else(|err| {
            panic!("Failed to load modules from {}: {err}", modules_path.display())
        });

        eprintln!(
            "Loaded {} module(s) from {}:",
            modules.len(),
            modules_path.display()
        );
        for (index, module) in modules.iter().enumerate() {
            eprintln!("  [{index}] {}", module.manifest.name);
        }

        Self {
            args,
            modules,
            module_index: 0,
            window: None,
            gl_context: None,
            gl_surface: None,
            renderer: None,
            start: Instant::now(),
            last_frame: Instant::now(),
            frame: 0,
            seed: generate_seed(),
            exiting: false,
        }
    }

    fn current_module(&self) -> &LoadedModule {
        &self.modules[self.module_index]
    }

    fn render_config(module: &LoadedModule) -> ModuleRenderConfig {
        ModuleRenderConfig {
            kind: module.manifest.kind,
            workgroup: module.manifest.workgroup,
            state: module.manifest.state,
        }
    }

    fn reset_animation(&mut self) {
        self.start = Instant::now();
        self.last_frame = Instant::now();
        self.frame = 0;
        self.seed = generate_seed();
    }

    fn update_window_title(&self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let module = self.current_module();
        let title = format!(
            "Art Display — {} ({}/{})",
            module.manifest.name,
            self.module_index + 1,
            self.modules.len()
        );
        window.set_title(&title);
    }

    fn load_current_module(&mut self) {
        let module = self.current_module().clone();
        let source = match module.read_shader_source() {
            Ok(source) => source,
            Err(err) => {
                eprintln!("{err}");
                return;
            }
        };
        let config = Self::render_config(&module);

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        match renderer.load_module(&source, &config) {
            Ok(()) => {
                renderer.reset_module_state();
                self.reset_animation();
                self.update_window_title();
                eprintln!(
                    "Active module: {} ({})",
                    module.manifest.name,
                    module.dir.display()
                );
            }
            Err(err) => eprintln!("Failed to load {}: {err}", module.manifest.name),
        }
    }

    fn reload_current_module(&mut self) {
        let module = self.current_module().clone();
        let source = match module.read_shader_source() {
            Ok(source) => source,
            Err(err) => {
                eprintln!("{err}");
                return;
            }
        };
        let config = Self::render_config(&module);

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        if let Err(err) = renderer.load_module(&source, &config) {
            eprintln!("Shader reload failed: {err}");
        } else {
            renderer.reset_module_state();
            self.reset_animation();
            eprintln!("Reloaded {}", module.shader_path().display());
        }
    }

    fn next_module(&mut self) {
        if self.modules.len() <= 1 {
            return;
        }
        self.module_index = (self.module_index + 1) % self.modules.len();
        self.load_current_module();
    }

    fn prev_module(&mut self) {
        if self.modules.len() <= 1 {
            return;
        }
        self.module_index = (self.module_index + self.modules.len() - 1) % self.modules.len();
        self.load_current_module();
    }

    fn cleanup(&mut self) {
        if let Some(renderer) = self.renderer.take() {
            renderer.destroy();
        }

        if let Some(context) = self.gl_context.take() {
            let _ = context.make_not_current();
        }

        self.gl_surface.take();
        self.window.take();
    }

    fn request_exit(&mut self, event_loop: &ActiveEventLoop) {
        if self.exiting {
            return;
        }
        self.exiting = true;
        self.cleanup();
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let (width, height) = self.args.resolution();
        let mut window_attributes = Window::default_attributes()
            .with_title("Art Display")
            .with_inner_size(LogicalSize::new(width, height));

        if self.args.fullscreen {
            window_attributes = window_attributes.with_fullscreen(Some(
                winit::window::Fullscreen::Borderless(None),
            ));
        }

        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(false);

        let display_builder =
            DisplayBuilder::new().with_window_attributes(Some(window_attributes));

        let (window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs
                    .filter(|config| config.srgb_capable())
                    .min_by_key(|config| config.num_samples())
                    .unwrap()
            })
            .unwrap();

        let window = window.expect("Failed to create window");

        let gl_display = gl_config.display();

        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 6))))
            .with_profile(GlProfile::Core)
            .build(Some(
                window
                    .window_handle()
                    .expect("window handle")
                    .as_raw(),
            ));

        let context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap()
        };

        let attrs = window
            .build_surface_attributes(Default::default())
            .unwrap();
        let surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &attrs)
                .unwrap()
        };

        let context = context.make_current(&surface).unwrap();

        let gl = Rc::new(unsafe {
            glow::Context::from_loader_function(|symbol| {
                let symbol = CString::new(symbol).expect("invalid OpenGL symbol");
                gl_display.get_proc_address(symbol.as_c_str()).cast()
            })
        });

        let module = self.current_module().clone();
        let art_source = module
            .read_shader_source()
            .unwrap_or_else(|err| panic!("{err}"));
        let art_config = App::render_config(&module);

        let size = window.inner_size();
        let renderer = Renderer::new(
            gl,
            size.width,
            size.height,
            &art_source,
            &art_config,
            DISPLAY_VERT,
            DISPLAY_FRAG,
        )
        .unwrap_or_else(|err| panic!("Renderer init failed: {err}"));

        self.window = Some(window);
        self.gl_context = Some(context);
        self.gl_surface = Some(surface);
        self.renderer = Some(renderer);
        self.update_window_title();
        eprintln!(
            "Active module: {} ({})",
            module.manifest.name,
            module.dir.display()
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => self.request_exit(event_loop),
            WindowEvent::RedrawRequested => {
                if self.exiting {
                    return;
                }
                let now = Instant::now();
                let elapsed = now.duration_since(self.start).as_secs_f32();
                let delta = now.duration_since(self.last_frame).as_secs_f32();
                self.last_frame = now;

                let Some(window) = self.window.as_ref() else {
                    return;
                };
                let size = window.inner_size();
                if size.width == 0 || size.height == 0 {
                    return;
                }

                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                let Some(surface) = self.gl_surface.as_ref() else {
                    return;
                };
                let Some(context) = self.gl_context.as_ref() else {
                    return;
                };

                renderer.resize(size.width, size.height);
                renderer.draw(elapsed, delta, self.frame, self.seed);
                self.frame = self.frame.wrapping_add(1);

                surface.swap_buffers(context).unwrap();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => self.request_exit(event_loop),
                    Key::Named(NamedKey::ArrowRight) => self.next_module(),
                    Key::Named(NamedKey::ArrowLeft) => self.prev_module(),
                    Key::Character(ref ch) => match ch.as_str() {
                        "n" => self.next_module(),
                        "p" => self.prev_module(),
                        "r" => self.reload_current_module(),
                        _ => {}
                    },
                    _ => {}
                }
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let (Some(window), Some(surface), Some(context)) = (
                        self.window.as_ref(),
                        self.gl_surface.as_ref(),
                        self.gl_context.as_ref(),
                    ) {
                        window.resize_surface(surface, context);
                    }
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.resize(size.width, size.height);
                        renderer.reset_module_state();
                    }
                    self.reset_animation();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.exiting {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn main() {
    let args = Args::parse();
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(args);
    event_loop.run_app(&mut app).unwrap();
}
