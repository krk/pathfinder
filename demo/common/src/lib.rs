// pathfinder/demo/common/src/lib.rs
//
// Copyright © 2019 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A demo app for Pathfinder.

use crate::ui::{DemoUI, UIAction, UIEvent};
use clap::{App, Arg};
use euclid::Size2D;
use gl::types::GLsizei;
use jemallocator;
use pathfinder_geometry::basic::point::{Point2DF32, Point2DI32, Point3DF32};
use pathfinder_geometry::basic::rect::RectF32;
use pathfinder_geometry::basic::transform2d::Transform2DF32;
use pathfinder_geometry::basic::transform3d::{Perspective, Transform3DF32};
use pathfinder_gl::device::{Buffer, BufferTarget, BufferUploadMode, Device, Program, Uniform};
use pathfinder_gl::device::{VertexArray, VertexAttr};
use pathfinder_gl::renderer::Renderer;
use pathfinder_renderer::builder::{RenderOptions, RenderTransform, SceneBuilder};
use pathfinder_renderer::gpu_data::BuiltScene;
use pathfinder_renderer::paint::ColorU;
use pathfinder_renderer::post::{DEFRINGING_KERNEL_CORE_GRAPHICS, STEM_DARKENING_FACTORS};
use pathfinder_renderer::scene::Scene;
use pathfinder_renderer::z_buffer::ZBuffer;
use pathfinder_svg::SceneExt;
use rayon::ThreadPoolBuilder;
use sdl2::{EventPump, Sdl, VideoSubsystem};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::{GLContext, GLProfile, Window};
use std::f32::consts::FRAC_PI_4;
use std::panic;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use usvg::{Options as UsvgOptions, Tree};

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

static DEFAULT_SVG_FILENAME: &'static str = "ghostscript-tiger-big-opt.svg";

const MAIN_FRAMEBUFFER_WIDTH: u32 = 1067;
const MAIN_FRAMEBUFFER_HEIGHT: u32 = 800;

const MOUSELOOK_ROTATION_SPEED: f32 = 0.007;
const CAMERA_VELOCITY: f32 = 25.0;

// How much the scene is scaled when a scale gesture is performed.
const CAMERA_SCALE_SPEED_2D: f32 = 2.0;
// How much the scene is scaled when a zoom button is clicked.
const CAMERA_ZOOM_AMOUNT_2D: f32 = 0.1;

const BACKGROUND_COLOR:   ColorU = ColorU { r: 32,  g: 32,  b: 32,  a: 255 };
const GROUND_SOLID_COLOR: ColorU = ColorU { r: 80,  g: 80,  b: 80,  a: 255 };
const GROUND_LINE_COLOR:  ColorU = ColorU { r: 127, g: 127, b: 127, a: 255 };

const APPROX_FONT_SIZE: f32 = 16.0;

const WORLD_SCALE: f32 = 800.0;
const GROUND_SCALE: f32 = 2.0;
const GRIDLINE_COUNT: u8 = 10;

mod ui;

pub struct DemoApp {
    window: Window,
    #[allow(dead_code)]
    sdl_context: Sdl,
    #[allow(dead_code)]
    sdl_video: VideoSubsystem,
    sdl_event_pump: EventPump,
    #[allow(dead_code)]
    gl_context: GLContext,

    scale_factor: f32,

    camera: Camera,
    frame_counter: u32,
    events: Vec<Event>,
    exit: bool,
    mouselook_enabled: bool,
    dirty: bool,

    ui: DemoUI,
    scene_thread_proxy: SceneThreadProxy,
    renderer: Renderer,

    device: DemoDevice,
    ground_program: GroundProgram,
    ground_solid_vertex_array: GroundSolidVertexArray,
    ground_line_vertex_array: GroundLineVertexArray,
}

impl DemoApp {
    pub fn new() -> DemoApp {
        let sdl_context = sdl2::init().unwrap();
        let sdl_video = sdl_context.video().unwrap();

        let gl_attributes = sdl_video.gl_attr();
        gl_attributes.set_context_profile(GLProfile::Core);
        gl_attributes.set_context_version(3, 3);
        gl_attributes.set_depth_size(24);
        gl_attributes.set_stencil_size(8);

        let window =
            sdl_video.window("Pathfinder Demo", MAIN_FRAMEBUFFER_WIDTH, MAIN_FRAMEBUFFER_HEIGHT)
                    .opengl()
                    .resizable()
                    .allow_highdpi()
                    .build()
                    .unwrap();

        let gl_context = window.gl_create_context().unwrap();
        gl::load_with(|name| sdl_video.gl_get_proc_address(name) as *const _);

        let sdl_event_pump = sdl_context.event_pump().unwrap();

        let device = Device::new();
        let options = Options::get(&device);

        let (window_width, _) = window.size();
        let (drawable_width, drawable_height) = window.drawable_size();
        let drawable_size = Size2D::new(drawable_width, drawable_height);

        let base_scene = load_scene(&options.input_path);
        let renderer = Renderer::new(&device, &drawable_size);
        let scene_thread_proxy = SceneThreadProxy::new(base_scene, options.clone());
        update_drawable_size(&window, &scene_thread_proxy);

        let camera = if options.threed { Camera::three_d() } else { Camera::two_d() };

        let ground_program = GroundProgram::new(&device);
        let ground_solid_vertex_array =
            GroundSolidVertexArray::new(&ground_program, &renderer.quad_vertex_positions_buffer());
        let ground_line_vertex_array = GroundLineVertexArray::new(&ground_program);

        DemoApp {
            window,
            sdl_context,
            sdl_video,
            sdl_event_pump,
            gl_context,

            scale_factor: drawable_width as f32 / window_width as f32,

            camera,
            frame_counter: 0,
            events: vec![],
            exit: false,
            mouselook_enabled: false,
            dirty: true,

            ui: DemoUI::new(&device, options),
            scene_thread_proxy,
            renderer,

            device: DemoDevice { device },
            ground_program,
            ground_solid_vertex_array,
            ground_line_vertex_array,
        }
    }

    pub fn run(&mut self) {
        while !self.exit {
            // Update the scene.
            self.build_scene();

            // Handle events.
            // FIXME(pcwalton): This can cause us to miss UI events if things get backed up...
            let ui_event = self.handle_events();

            // Draw the scene.
            let render_msg = self.scene_thread_proxy.receiver.recv().unwrap();
            self.draw_scene(render_msg, ui_event);
        }
    }

    fn build_scene(&mut self) {
        let (drawable_width, drawable_height) = self.window.drawable_size();
        let drawable_size = Point2DI32::new(drawable_width as i32, drawable_height as i32);

        let render_transform = match self.camera {
            Camera::ThreeD { ref mut transform, ref mut velocity } => {
                if transform.offset(*velocity) {
                    self.dirty = true;
                }
                RenderTransform::Perspective(transform.to_perspective(drawable_size, true))
            }
            Camera::TwoD(transform) => RenderTransform::Transform2D(transform),
        };

        let count = if self.frame_counter == 0 { 2 } else { 1 };
        for _ in 0..count {
            self.scene_thread_proxy.sender.send(MainToSceneMsg::Build(BuildOptions {
                render_transform: render_transform.clone(),
                stem_darkening_font_size: if self.ui.stem_darkening_effect_enabled {
                    Some(APPROX_FONT_SIZE * self.scale_factor)
                } else {
                    None
                },
            })).unwrap();
        }

        if count == 2 {
            self.dirty = true;
        }
    }

    fn handle_events(&mut self) -> UIEvent {
        let mut ui_event = UIEvent::None;

        if !self.dirty {
            self.events.push(self.sdl_event_pump.wait_event());
        } else {
            self.dirty = false;
        }

        for event in self.sdl_event_pump.poll_iter() {
            self.events.push(event);
        }

        for event in self.events.drain(..) {
            match event {
                Event::Quit { .. } |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    self.exit = true;
                    self.dirty = true;
                }
                Event::Window { win_event: WindowEvent::SizeChanged(..), .. } => {
                    let drawable_size = update_drawable_size(&self.window,
                                                             &self.scene_thread_proxy);
                    self.renderer.set_main_framebuffer_size(&drawable_size);
                    self.dirty = true;
                }
                Event::MouseButtonDown { x, y, .. } => {
                    let point = Point2DI32::new(x, y).scale(self.scale_factor as i32);
                    ui_event = UIEvent::MouseDown(point);
                }
                Event::MouseMotion { xrel, yrel, .. } if self.mouselook_enabled => {
                    if let Camera::ThreeD { ref mut transform, .. } = self.camera {
                        transform.yaw += xrel as f32 * MOUSELOOK_ROTATION_SPEED;
                        transform.pitch += yrel as f32 * MOUSELOOK_ROTATION_SPEED;
                        self.dirty = true;
                    }
                }
                Event::MouseMotion { x, y, xrel, yrel, mousestate, .. } if mousestate.left() => {
                    let absolute_position = Point2DI32::new(x, y).scale(self.scale_factor as i32);
                    let relative_position =
                        Point2DI32::new(xrel, yrel).scale(self.scale_factor as i32);
                    ui_event = UIEvent::MouseDragged { absolute_position, relative_position };
                    self.dirty = true;
                }
                Event::MultiGesture { d_dist, .. } => {
                    if let Camera::TwoD(ref mut transform) = self.camera {
                        let mouse_state = self.sdl_event_pump.mouse_state();
                        let position = Point2DI32::new(mouse_state.x(), mouse_state.y());
                        let position = position.to_f32().scale(self.scale_factor);
                        *transform = transform.post_translate(-position);
                        let scale_delta = 1.0 + d_dist * CAMERA_SCALE_SPEED_2D;
                        *transform = transform.post_scale(Point2DF32::splat(scale_delta));
                        *transform = transform.post_translate(position);
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::W), .. } => {
                    if let Camera::ThreeD { ref mut velocity, .. } = self.camera {
                        velocity.set_z(-CAMERA_VELOCITY);
                        self.dirty = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::S), .. } => {
                    if let Camera::ThreeD { ref mut velocity, .. } = self.camera {
                        velocity.set_z(CAMERA_VELOCITY);
                        self.dirty = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::A), .. } => {
                    if let Camera::ThreeD { ref mut velocity, .. } = self.camera {
                        velocity.set_x(-CAMERA_VELOCITY);
                        self.dirty = true;
                    }
                }
                Event::KeyDown { keycode: Some(Keycode::D), .. } => {
                    if let Camera::ThreeD { ref mut velocity, .. } = self.camera {
                        velocity.set_x(CAMERA_VELOCITY);
                        self.dirty = true;
                    }
                }
                Event::KeyUp { keycode: Some(Keycode::W), .. } |
                Event::KeyUp { keycode: Some(Keycode::S), .. } => {
                    if let Camera::ThreeD { ref mut velocity, .. } = self.camera {
                        velocity.set_z(0.0);
                        self.dirty = true;
                    }
                }
                Event::KeyUp { keycode: Some(Keycode::A), .. } |
                Event::KeyUp { keycode: Some(Keycode::D), .. } => {
                    if let Camera::ThreeD { ref mut velocity, .. } = self.camera {
                        velocity.set_x(0.0);
                        self.dirty = true;
                    }
                }
                _ => continue,
            }
        }

        ui_event
    }

    fn draw_scene(&mut self, render_msg: SceneToMainMsg, mut ui_event: UIEvent) {
        let SceneToMainMsg::Render { built_scene, tile_time } = render_msg;

        self.device.clear();
        self.draw_environment();
        self.render_vector_scene(&built_scene);

        let rendering_time = self.renderer.shift_timer_query();
        self.renderer.debug_ui.add_sample(tile_time, rendering_time);
        self.renderer.debug_ui.draw();

        if !ui_event.is_none() {
            self.dirty = true;
        }

        let mut ui_action = UIAction::None;
        self.ui.update(&mut self.renderer.debug_ui, &mut ui_event, &mut ui_action);
        self.handle_ui_action(&mut ui_action);

        // Switch camera mode (2D/3D) if requested.
        //
        // FIXME(pcwalton): This mess should really be an MVC setup.
        match (&self.camera, self.ui.threed_enabled) {
            (&Camera::TwoD { .. }, true) => self.camera = Camera::three_d(),
            (&Camera::ThreeD { .. }, false) => self.camera = Camera::two_d(),
            _ => {}
        }

        match ui_event {
            UIEvent::MouseDown(_) if self.camera.is_3d() => {
                // If nothing handled the mouse-down event, toggle mouselook.
                self.mouselook_enabled = !self.mouselook_enabled;
            }
            UIEvent::MouseDragged { relative_position, .. } => {
                if let Camera::TwoD(ref mut transform) = self.camera {
                    *transform = transform.post_translate(relative_position.to_f32());
                }
            }
            _ => {}
        }

        self.window.gl_swap_window();
        self.frame_counter += 1;
    }

    fn draw_environment(&self) {
        let transform = match self.camera {
            Camera::TwoD(..) => return,
            Camera::ThreeD { ref transform, .. } => *transform,
        };

        let (drawable_width, drawable_height) = self.window.drawable_size();
        let drawable_size = Point2DI32::new(drawable_width as i32, drawable_height as i32);
        let perspective = transform.to_perspective(drawable_size, false);

        unsafe {
            // Use the stencil buffer to avoid Z-fighting with the gridlines.
            let mut transform = perspective.transform;
            let gridline_scale = GROUND_SCALE / GRIDLINE_COUNT as f32;
            transform = transform.post_mul(&Transform3DF32::from_scale(gridline_scale,
                                                                       1.0,
                                                                       gridline_scale));
            gl::BindVertexArray(self.ground_line_vertex_array.vertex_array.gl_vertex_array);
            gl::UseProgram(self.ground_program.program.gl_program);
            gl::UniformMatrix4fv(self.ground_program.transform_uniform.location,
                                 1,
                                 gl::FALSE,
                                 transform.as_ptr());
            let color = GROUND_LINE_COLOR.to_f32();
            gl::Uniform4f(self.ground_program.color_uniform.location,
                          color.r(),
                          color.g(),
                          color.b(),
                          color.a());
            gl::DepthFunc(gl::LESS);
            gl::DepthMask(gl::FALSE);
            gl::Enable(gl::DEPTH_TEST);
            gl::StencilFunc(gl::ALWAYS, 1, !0);
            gl::StencilOp(gl::KEEP, gl::KEEP, gl::REPLACE);
            gl::Enable(gl::STENCIL_TEST);
            gl::Disable(gl::BLEND);
            gl::DrawArrays(gl::LINES, 0, (GRIDLINE_COUNT as GLsizei + 1) * 4);
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::STENCIL_TEST);

            let mut transform = perspective.transform;
            transform =
                transform.post_mul(&Transform3DF32::from_scale(GROUND_SCALE, 1.0, GROUND_SCALE));
            gl::BindVertexArray(self.ground_solid_vertex_array.vertex_array.gl_vertex_array);
            gl::UseProgram(self.ground_program.program.gl_program);
            gl::UniformMatrix4fv(self.ground_program.transform_uniform.location,
                                 1,
                                 gl::FALSE,
                                 transform.as_ptr());
            let color = GROUND_SOLID_COLOR.to_f32();
            gl::Uniform4f(self.ground_program.color_uniform.location,
                          color.r(),
                          color.g(),
                          color.b(),
                          color.a());
            gl::DepthFunc(gl::LESS);
            gl::DepthMask(gl::TRUE);
            gl::Enable(gl::DEPTH_TEST);
            gl::StencilFunc(gl::NOTEQUAL, 1, !0);
            gl::StencilOp(gl::KEEP, gl::KEEP, gl::KEEP);
            gl::Enable(gl::STENCIL_TEST);
            gl::Disable(gl::BLEND);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::STENCIL_TEST);
        }
    }

    fn render_vector_scene(&mut self, built_scene: &BuiltScene) {
        if self.ui.gamma_correction_effect_enabled {
            self.renderer.enable_gamma_correction(BACKGROUND_COLOR);
        } else {
            self.renderer.disable_gamma_correction();
        }

        if self.ui.subpixel_aa_effect_enabled {
            self.renderer.enable_subpixel_aa(&DEFRINGING_KERNEL_CORE_GRAPHICS);
        } else {
            self.renderer.disable_subpixel_aa();
        }

        self.renderer.render_scene(&built_scene);
    }

    fn handle_ui_action(&mut self, ui_action: &mut UIAction) {
        match ui_action {
            UIAction::None => {}
            UIAction::OpenFile(ref path) => {
                let scene = load_scene(&path);
                self.scene_thread_proxy.load_scene(scene);
                update_drawable_size(&self.window, &self.scene_thread_proxy);
                self.dirty = true;
            }
            UIAction::ZoomIn => {
                if let Camera::TwoD(ref mut transform) = self.camera {
                    let scale = Point2DF32::splat(1.0 + CAMERA_ZOOM_AMOUNT_2D);
                    let center = center_of_window(&self.window);
                    *transform = transform.post_translate(-center)
                                          .post_scale(scale)
                                          .post_translate(center);
                    self.dirty = true;
                }
            }
            UIAction::ZoomOut => {
                if let Camera::TwoD(ref mut transform) = self.camera {
                    let scale = Point2DF32::splat(1.0 - CAMERA_ZOOM_AMOUNT_2D);
                    let center = center_of_window(&self.window);
                    *transform = transform.post_translate(-center)
                                          .post_scale(scale)
                                          .post_translate(center);
                    self.dirty = true;
                }
            }
            UIAction::Rotate(theta) => {
                if let Camera::TwoD(ref mut transform) = self.camera {
                    let old_rotation = transform.rotation();
                    let center = center_of_window(&self.window);
                    *transform = transform.post_translate(-center)
                                          .post_rotate(*theta - old_rotation)
                                          .post_translate(center);
                }
            }
        }
    }
}

struct SceneThreadProxy {
    sender: Sender<MainToSceneMsg>,
    receiver: Receiver<SceneToMainMsg>,
}

impl SceneThreadProxy {
    fn new(scene: Scene, options: Options) -> SceneThreadProxy {
        let (main_to_scene_sender, main_to_scene_receiver) = mpsc::channel();
        let (scene_to_main_sender, scene_to_main_receiver) = mpsc::channel();
        SceneThread::new(scene, scene_to_main_sender, main_to_scene_receiver, options);
        SceneThreadProxy { sender: main_to_scene_sender, receiver: scene_to_main_receiver }
    }

    fn load_scene(&self, scene: Scene) {
        self.sender.send(MainToSceneMsg::LoadScene(scene)).unwrap();
    }

    fn set_drawable_size(&self, drawable_size: &Size2D<u32>) {
        self.sender.send(MainToSceneMsg::SetDrawableSize(*drawable_size)).unwrap();
    }
}

struct SceneThread {
    scene: Scene,
    sender: Sender<SceneToMainMsg>,
    receiver: Receiver<MainToSceneMsg>,
    options: Options,
}

impl SceneThread {
    fn new(scene: Scene,
           sender: Sender<SceneToMainMsg>,
           receiver: Receiver<MainToSceneMsg>,
           options: Options) {
        thread::spawn(move || (SceneThread { scene, sender, receiver, options }).run());
    }

    fn run(mut self) {
        while let Ok(msg) = self.receiver.recv() {
            match msg {
                MainToSceneMsg::LoadScene(scene) => self.scene = scene,
                MainToSceneMsg::SetDrawableSize(size) => {
                    self.scene.view_box =
                        RectF32::new(Point2DF32::default(),
                                     Point2DF32::new(size.width as f32, size.height as f32));
                }
                MainToSceneMsg::Build(build_options) => {
                    let start_time = Instant::now();
                    let built_scene = build_scene(&self.scene, build_options, self.options.jobs);
                    let tile_time = Instant::now() - start_time;
                    self.sender.send(SceneToMainMsg::Render { built_scene, tile_time }).unwrap();
                }
            }
        }
    }
}

enum MainToSceneMsg {
    LoadScene(Scene),
    SetDrawableSize(Size2D<u32>),
    Build(BuildOptions),
}

struct BuildOptions {
    render_transform: RenderTransform,
    stem_darkening_font_size: Option<f32>,
}

enum SceneToMainMsg {
    Render { built_scene: BuiltScene, tile_time: Duration }
}

#[derive(Clone)]
pub struct Options {
    jobs: Option<usize>,
    threed: bool,
    input_path: PathBuf,
}

impl Options {
    fn get(device: &Device) -> Options {
        let matches = App::new("tile-svg")
            .arg(
                Arg::with_name("jobs")
                    .short("j")
                    .long("jobs")
                    .value_name("THREADS")
                    .takes_value(true)
                    .help("Number of threads to use"),
            )
            .arg(
                Arg::with_name("3d")
                    .short("3")
                    .long("3d")
                    .help("Run in 3D"),
            )
            .arg(Arg::with_name("INPUT").help("Path to the SVG file to render").index(1))
            .get_matches();

        let jobs: Option<usize> = matches
            .value_of("jobs")
            .map(|string| string.parse().unwrap());
        let threed = matches.is_present("3d");

        let input_path = match matches.value_of("INPUT") {
            Some(path) => PathBuf::from(path),
            None => {
                let mut path = device.resources_directory.clone();
                path.push("svg");
                path.push(DEFAULT_SVG_FILENAME);
                path
            }
        };

        // Set up Rayon.
        let mut thread_pool_builder = ThreadPoolBuilder::new();
        if let Some(jobs) = jobs {
            thread_pool_builder = thread_pool_builder.num_threads(jobs);
        }
        thread_pool_builder.build_global().unwrap();

        Options { jobs, threed, input_path }
    }
}

fn load_scene(input_path: &Path) -> Scene {
    let usvg = Tree::from_file(input_path, &UsvgOptions::default()).unwrap();
    let scene = Scene::from_tree(usvg);
    println!("Scene bounds: {:?}", scene.bounds);
    println!("{} objects, {} paints", scene.objects.len(), scene.paints.len());
    scene
}

fn build_scene(scene: &Scene, build_options: BuildOptions, jobs: Option<usize>) -> BuiltScene {
    let z_buffer = ZBuffer::new(scene.view_box);

    let render_options = RenderOptions {
        transform: build_options.render_transform,
        dilation: match build_options.stem_darkening_font_size {
            None => Point2DF32::default(),
            Some(font_size) => {
                let (x, y) = (STEM_DARKENING_FACTORS[0], STEM_DARKENING_FACTORS[1]);
                Point2DF32::new(x, y).scale(font_size)
            }
        },
    };

    let built_options = render_options.prepare(scene.bounds);
    let quad = built_options.quad();

    let built_objects = panic::catch_unwind(|| {
         match jobs {
            Some(1) => scene.build_objects_sequentially(built_options, &z_buffer),
            _ => scene.build_objects(built_options, &z_buffer),
        }
    });

    let built_objects = match built_objects {
        Ok(built_objects) => built_objects,
        Err(_) => {
            eprintln!("Scene building crashed! Dumping scene:");
            println!("{:?}", scene);
            process::exit(1);
        }
    };

    let mut built_scene = BuiltScene::new(scene.view_box, &quad);
    built_scene.shaders = scene.build_shaders();

    let mut scene_builder = SceneBuilder::new(built_objects, z_buffer, scene.view_box);
    built_scene.solid_tiles = scene_builder.build_solid_tiles();
    while let Some(batch) = scene_builder.build_batch() {
        built_scene.batches.push(batch);
    }

    built_scene
}

fn update_drawable_size(window: &Window, scene_thread_proxy: &SceneThreadProxy) -> Size2D<u32> {
    let (drawable_width, drawable_height) = window.drawable_size();
    let drawable_size = Size2D::new(drawable_width as u32, drawable_height as u32);
    scene_thread_proxy.set_drawable_size(&drawable_size);
    drawable_size
}

fn center_of_window(window: &Window) -> Point2DF32 {
    let (drawable_width, drawable_height) = window.drawable_size();
    Point2DI32::new(drawable_width as i32, drawable_height as i32).to_f32().scale(0.5)
}

enum Camera {
    TwoD(Transform2DF32),
    ThreeD { transform: CameraTransform3D, velocity: Point3DF32 },
}

impl Camera {
    fn two_d() -> Camera {
        Camera::TwoD(Transform2DF32::default())
    }

    fn three_d() -> Camera {
        Camera::ThreeD { transform: CameraTransform3D::new(), velocity: Point3DF32::default() }
    }

    fn is_3d(&self) -> bool {
        match *self { Camera::ThreeD { .. } => true, Camera::TwoD { .. } => false }
    }
}

#[derive(Clone, Copy)]
struct CameraTransform3D {
    position: Point3DF32,
    yaw: f32,
    pitch: f32,
}

impl CameraTransform3D {
    fn new() -> CameraTransform3D {
        CameraTransform3D {
            position: Point3DF32::new(500.0, 500.0, 3000.0, 1.0),
            yaw: 0.0,
            pitch: 0.0,
        }
    }

    fn offset(&mut self, vector: Point3DF32) -> bool {
        let update = !vector.is_zero();
        if update {
            let rotation = Transform3DF32::from_rotation(-self.yaw, -self.pitch, 0.0);
            self.position = self.position + rotation.transform_point(vector);
        }
        update
    }

    fn to_perspective(&self, drawable_size: Point2DI32, flip_y: bool) -> Perspective {
        let aspect = drawable_size.x() as f32 / drawable_size.y() as f32;
        let mut transform = Transform3DF32::from_perspective(FRAC_PI_4, aspect, 0.025, 100.0);

        let scale_inv = 1.0 / WORLD_SCALE;
        transform = transform.post_mul(&Transform3DF32::from_rotation(self.yaw, self.pitch, 0.0));
        transform = transform.post_mul(&Transform3DF32::from_uniform_scale(scale_inv));
        transform = transform.post_mul(&Transform3DF32::from_translation(-self.position.x(),
                                                                         -self.position.y(),
                                                                         -self.position.z()));

        if flip_y {
            transform = transform.post_mul(&Transform3DF32::from_scale(1.0, -1.0, 1.0));
            transform =
                transform.post_mul(&Transform3DF32::from_translation(0.0, -WORLD_SCALE, 0.0));
        }

        let drawable_size = Size2D::new(drawable_size.x() as u32, drawable_size.y() as u32);
        Perspective::new(&transform, &drawable_size)
    }
}

struct DemoDevice {
    #[allow(dead_code)]
    device: Device,
}

impl DemoDevice {
    fn clear(&self) {
        let color = BACKGROUND_COLOR.to_f32();
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::ClearColor(color.r(), color.g(), color.b(), color.a());
            gl::ClearDepth(1.0);
            gl::ClearStencil(0);
            gl::DepthMask(gl::TRUE);
            gl::StencilMask(!0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
        }
    }
}

struct GroundProgram {
    program: Program,
    transform_uniform: Uniform,
    color_uniform: Uniform,
}

impl GroundProgram {
    fn new(device: &Device) -> GroundProgram {
        let program = device.create_program("demo_ground");
        let transform_uniform = Uniform::new(&program, "Transform");
        let color_uniform = Uniform::new(&program, "Color");
        GroundProgram { program, transform_uniform, color_uniform }
    }
}

struct GroundSolidVertexArray {
    vertex_array: VertexArray,
}

impl GroundSolidVertexArray {
    fn new(ground_program: &GroundProgram, quad_vertex_positions_buffer: &Buffer)
           -> GroundSolidVertexArray {
        let vertex_array = VertexArray::new();
        unsafe {
            let position_attr = VertexAttr::new(&ground_program.program, "Position");

            gl::BindVertexArray(vertex_array.gl_vertex_array);
            gl::UseProgram(ground_program.program.gl_program);
            gl::BindBuffer(gl::ARRAY_BUFFER, quad_vertex_positions_buffer.gl_buffer);
            position_attr.configure_float(2, gl::UNSIGNED_BYTE, false, 0, 0, 0);
        }

        GroundSolidVertexArray { vertex_array }
    }
}

struct GroundLineVertexArray {
    vertex_array: VertexArray,
    #[allow(dead_code)]
    grid_vertex_positions_buffer: Buffer,
}

impl GroundLineVertexArray {
    fn new(ground_program: &GroundProgram) -> GroundLineVertexArray {
        let grid_vertex_positions_buffer = Buffer::new();
        grid_vertex_positions_buffer.upload(&create_grid_vertex_positions(),
                                            BufferTarget::Vertex,
                                            BufferUploadMode::Static);

        let vertex_array = VertexArray::new();
        unsafe {
            let position_attr = VertexAttr::new(&ground_program.program, "Position");

            gl::BindVertexArray(vertex_array.gl_vertex_array);
            gl::UseProgram(ground_program.program.gl_program);
            gl::BindBuffer(gl::ARRAY_BUFFER, grid_vertex_positions_buffer.gl_buffer);
            position_attr.configure_float(2, gl::UNSIGNED_BYTE, false, 0, 0, 0);
        }

        GroundLineVertexArray { vertex_array, grid_vertex_positions_buffer }
    }
}

fn create_grid_vertex_positions() -> Vec<(u8, u8)> {
    let mut positions = vec![];
    for index in 0..(GRIDLINE_COUNT + 1) {
        positions.extend_from_slice(&[
            (0, index), (GRIDLINE_COUNT, index),
            (index, 0), (index, GRIDLINE_COUNT),
        ]);
    }
    positions
}