mod args;
mod renderer;

use std::ffi::CString;
use std::path::PathBuf;
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
use raw_window_handle::HasWindowHandle;
use renderer::Renderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const DISPLAY_VERT: &str = include_str!("../shaders/display.vert");
const DISPLAY_FRAG: &str = include_str!("../shaders/display.frag");

struct App {
    args: Args,
    window: Option<Window>,
    gl_context: Option<glutin::context::PossiblyCurrentContext>,
    gl_surface: Option<glutin::surface::Surface<glutin::surface::WindowSurface>>,
    renderer: Option<Renderer>,
    compute_path: PathBuf,
    start: Instant,
    last_frame: Instant,
    frame: u32,
    exiting: bool,
}

impl App {
    fn new(args: Args) -> Self {
        let compute_path = args.compute_shader();
        Self {
            args,
            window: None,
            gl_context: None,
            gl_surface: None,
            renderer: None,
            compute_path,
            start: Instant::now(),
            last_frame: Instant::now(),
            frame: 0,
            exiting: false,
        }
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

    fn reload_shader(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        match std::fs::read_to_string(&self.compute_path) {
            Ok(source) => {
                if let Err(err) = renderer.reload_compute(&source) {
                    eprintln!("Shader reload failed: {err}");
                } else {
                    self.start = Instant::now();
                    self.last_frame = Instant::now();
                    self.frame = 0;
                    eprintln!("Reloaded {}", self.compute_path.display());
                }
            }
            Err(err) => eprintln!("Failed to read {}: {err}", self.compute_path.display()),
        }
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

        let compute_source = std::fs::read_to_string(&self.compute_path).unwrap_or_else(|err| {
            panic!("Failed to read {}: {err}", self.compute_path.display());
        });

        let size = window.inner_size();
        let renderer = Renderer::new(
            gl,
            size.width,
            size.height,
            &compute_source,
            DISPLAY_VERT,
            DISPLAY_FRAG,
        )
        .unwrap_or_else(|err| panic!("Renderer init failed: {err}"));

        self.window = Some(window);
        self.gl_context = Some(context);
        self.gl_surface = Some(surface);
        self.renderer = Some(renderer);
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
                renderer.draw(elapsed, delta, self.frame);
                self.frame = self.frame.wrapping_add(1);

                surface.swap_buffers(context).unwrap();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => self.request_exit(event_loop),
                    Key::Character(ref ch) if ch.as_str() == "r" => self.reload_shader(),
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
