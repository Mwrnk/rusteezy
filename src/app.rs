//! Window, event loop, fixed-step clock and HUD. Implement [`Scene`] and call [`run`].

use std::{
    borrow::Cow,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    atlas::{DEFAULT_EXTRA, GlyphAtlas},
    clock::FixedClock,
    color::Rgb,
    grid::Grid,
    render::{RenderSettings, RenderTiming, Renderer},
};

/// Record frames to a raw RGB file at a fixed frame rate (for GIFs/videos).
///
/// Time advances exactly `1/fps` per frame regardless of how long rendering
/// takes, so recordings are smooth and reproducible. Frames are appended to
/// `path` as packed 8-bit RGB; a sidecar `<path>.txt` holds `width height fps`.
#[derive(Clone, Debug)]
pub struct Record {
    pub path: PathBuf,
    pub fps: u32,
    /// Start capturing at this time (seconds).
    pub start: f64,
    /// How many seconds to capture.
    pub seconds: f64,
}

/// Window and runtime configuration.
pub struct Config {
    pub title: String,
    /// Logical window size.
    pub width: f64,
    pub height: f64,
    /// TTF/OTF bytes of a monospace font.
    pub font: Cow<'static, [u8]>,
    /// Fonts to take glyphs from when `font` lacks them, in order.
    pub fallback_fonts: Vec<Cow<'static, [u8]>>,
    /// Font size in logical pixels (multiplied by the display scale factor).
    pub font_px: f32,
    /// Non-ASCII glyphs to rasterize (printable ASCII is always included).
    pub extra_glyphs: String,
    /// Fixed simulation rate.
    pub sim_hz: f64,
    /// Wait for vblank. `false` = uncapped, for measuring headroom.
    pub vsync: bool,
    pub show_hud: bool,
    /// Print adapter / present-mode info at startup.
    pub verbose: bool,
    /// Save a PNG of the frame at each (time in seconds, path), in order.
    pub captures: Vec<(f64, PathBuf)>,
    /// Quit once all captures are written.
    pub exit_after_captures: bool,
    /// Benchmark: render this many frames off-screen (no vsync, GPU waited on
    /// every frame), print timing stats, quit.
    pub bench_frames: Option<u32>,
    /// Record frames to disk, then quit.
    pub record: Option<Record>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: "rusteezy".into(),
            width: 1280.0,
            height: 800.0,
            #[cfg(feature = "default-font")]
            font: Cow::Borrowed(crate::fonts::CASCADIA_MONO),
            #[cfg(not(feature = "default-font"))]
            font: Cow::Borrowed(&[]),
            #[cfg(feature = "default-font")]
            fallback_fonts: vec![Cow::Borrowed(crate::fonts::DEJAVU_SANS_MONO)],
            #[cfg(not(feature = "default-font"))]
            fallback_fonts: Vec::new(),
            font_px: 10.0,
            extra_glyphs: DEFAULT_EXTRA.into(),
            sim_hz: 240.0,
            vsync: true,
            show_hud: false,
            verbose: false,
            captures: Vec::new(),
            exit_after_captures: false,
            bench_frames: None,
            record: None,
        }
    }
}

impl Config {
    /// Default config with a window title.
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), ..Default::default() }
    }
}

/// What a scene gets each render frame.
pub struct Frame<'a> {
    pub grid: &'a mut Grid,
    /// Smooth render time in seconds (between fixed steps).
    pub time: f64,
    /// Interpolation factor between the last two fixed steps (for particles).
    pub alpha: f32,
    /// Time since the previous frame.
    pub dt: f32,
    /// Size of one cell in physical pixels (use for aspect-correct projection).
    pub cell_w: f32,
    pub cell_h: f32,
    pub atlas: &'a GlyphAtlas,
    pub settings: &'a mut RenderSettings,
}

impl Frame<'_> {
    /// Grid size in cells.
    pub fn size(&self) -> (usize, usize) {
        (self.grid.width(), self.grid.height())
    }

    /// Width / height of one cell (≈ 0.5 for most monospace fonts).
    pub fn cell_aspect(&self) -> f32 {
        self.cell_w / self.cell_h
    }
}

/// Your animation. `update` runs at a fixed rate; `draw` once per displayed frame.
pub trait Scene {
    /// Advance the simulation by exactly `dt` seconds; `time` is the sim time after the step.
    fn update(&mut self, _dt: f32, _time: f64) {}
    /// Fill the grid for this frame. The grid has been cleared to black.
    fn draw(&mut self, frame: &mut Frame);
    /// A key was pressed (Escape quits and F1 toggles the HUD before this is called).
    fn key(&mut self, _key: &Key) {}
    /// Extra text for the HUD line.
    fn hud(&self) -> String {
        String::new()
    }
}

/// Open a window and run `scene` until it's closed.
pub fn run<S: Scene + 'static>(config: Config, scene: S) -> Result<(), String> {
    if config.font.is_empty() {
        return Err("no font: set Config::font or enable the `default-font` feature".into());
    }
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut handler = Handler { config, scene, state: None, error: None };
    event_loop.run_app(&mut handler).map_err(|e| e.to_string())?;
    match handler.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

struct Handler<S: Scene> {
    config: Config,
    scene: S,
    state: Option<State>,
    error: Option<String>,
}

struct State {
    window: Arc<Window>,
    renderer: Renderer,
    grid: Grid,
    clock: FixedClock,
    last: Instant,
    hud: Hud,
    bench: Vec<f64>,
    recorder: Option<Recorder>,
    done: bool,
}

struct Recorder {
    out: BufWriter<File>,
    frames: u32,
    size: (u32, u32),
}

struct Hud {
    show: bool,
    frames: u32,
    window_start: Instant,
    work_acc: f64,
    render_acc: f64,
    wait_acc: f64,
    fps: f64,
    work_ms: f64,
    render_ms: f64,
    wait_ms: f64,
}

impl Hud {
    fn new(show: bool) -> Self {
        Self {
            show,
            frames: 0,
            window_start: Instant::now(),
            work_acc: 0.0,
            render_acc: 0.0,
            wait_acc: 0.0,
            fps: 0.0,
            work_ms: 0.0,
            render_ms: 0.0,
            wait_ms: 0.0,
        }
    }

    fn record(&mut self, work: f64, t: RenderTiming) {
        self.frames += 1;
        self.work_acc += work;
        self.render_acc += t.cpu;
        self.wait_acc += t.acquire;
        let el = self.window_start.elapsed().as_secs_f64();
        if el >= 0.5 {
            let n = self.frames as f64;
            self.fps = n / el;
            self.work_ms = self.work_acc * 1000.0 / n;
            self.render_ms = self.render_acc * 1000.0 / n;
            self.wait_ms = self.wait_acc * 1000.0 / n;
            self.frames = 0;
            self.work_acc = 0.0;
            self.render_acc = 0.0;
            self.wait_acc = 0.0;
            self.window_start = Instant::now();
        }
    }
}

fn build_atlas(config: &Config, scale: f64) -> Result<GlyphAtlas, String> {
    let fallbacks: Vec<&[u8]> = config.fallback_fonts.iter().map(|f| f.as_ref()).collect();
    GlyphAtlas::with_fallbacks(
        &config.font,
        &fallbacks,
        (config.font_px as f64 * scale).round() as f32,
        &config.extra_glyphs,
    )
}

/// Bench, record and screenshot runs render off-screen: they must not depend on
/// the window being visible (an occluded window gets no swapchain images).
fn offscreen(c: &Config) -> bool {
    c.bench_frames.is_some() || c.record.is_some() || c.exit_after_captures
}

impl<S: Scene> Handler<S> {
    fn init(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let attrs = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(LogicalSize::new(self.config.width, self.config.height));
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        let atlas = build_atlas(&self.config, window.scale_factor())?;
        let offscreen = offscreen(&self.config);
        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            atlas,
            self.config.vsync && !offscreen,
            self.config.verbose,
        ))?;
        let (gw, gh) = renderer.grid_dims();
        self.state = Some(State {
            window,
            renderer,
            grid: Grid::new(gw, gh),
            clock: FixedClock::new(self.config.sim_hz),
            last: Instant::now(),
            hud: Hud::new(self.config.show_hud),
            bench: Vec::new(),
            recorder: None,
            done: false,
        });
        Ok(())
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        let Some(st) = self.state.as_mut() else { return };
        if st.done {
            return;
        }
        let start = Instant::now();
        let mut real_dt = (start - st.last).as_secs_f64();
        st.last = start;
        if let Some(rec) = &self.config.record {
            real_dt = 1.0 / rec.fps.max(1) as f64;
        }

        // Fixed-step simulation.
        let steps = st.clock.advance(real_dt);
        let step = st.clock.step();
        let t0 = st.clock.time() - steps as f64 * step;
        for i in 0..steps {
            self.scene.update(step as f32, t0 + (i + 1) as f64 * step);
        }

        // Draw into the grid.
        let (gw, gh) = st.renderer.grid_dims();
        if (gw, gh) != (st.grid.width(), st.grid.height()) {
            st.grid.resize(gw, gh);
        }
        st.grid.clear(Rgb::BLACK);
        let (cell_w, cell_h) = (st.renderer.atlas().cell_w as f32, st.renderer.atlas().cell_h as f32);
        {
            let (atlas, settings) = st.renderer.parts();
            let mut frame = Frame {
                grid: &mut st.grid,
                time: st.clock.render_time(),
                alpha: st.clock.alpha(),
                dt: real_dt as f32,
                cell_w,
                cell_h,
                atlas,
                settings,
            };
            self.scene.draw(&mut frame);
        }

        if st.hud.show {
            let h = &st.hud;
            let cpu = h.work_ms + h.render_ms;
            let line = format!(
                " {:5.1} fps | cpu {:.2} ms (sim+draw {:.2}, gpu submit {:.2}) | vsync wait {:.2} ms | {}x{} | {} ",
                h.fps,
                cpu,
                h.work_ms,
                h.render_ms,
                h.wait_ms,
                st.grid.width(),
                st.grid.height(),
                self.scene.hud(),
            );
            st.grid.put_str(0, 0, &line, Rgb::new(0.75, 0.8, 0.85), Some(Rgb::BLACK));
        }

        let work = start.elapsed().as_secs_f64();
        let timing = if offscreen(&self.config) {
            st.renderer.render_offscreen_blocking(&st.grid)
        } else {
            st.renderer.render(&st.grid)
        };
        st.hud.record(work, timing);

        if let Some(total) = self.config.bench_frames {
            st.bench.push(work + timing.cpu);
            if st.bench.len() as u32 == total {
                print_bench(&st.bench, &st.grid, st.renderer.size());
                st.done = true;
                el.exit();
            }
        }

        if let Some(rec) = &self.config.record
            && st.clock.render_time() >= rec.start
            && let Err(e) = record_frame(st, rec)
        {
            self.error = Some(e);
            st.done = true;
            el.exit();
        }
        if st.done {
            if self.config.record.is_some() {
                el.exit();
            }
            return;
        }

        if let Some((t, _)) = self.config.captures.first()
            && st.clock.render_time() >= *t
        {
            let (_, path) = self.config.captures.remove(0);
            match st.renderer.capture(&path) {
                Ok(()) => eprintln!("captured {} at t={:.2}s", path.display(), st.clock.render_time()),
                Err(e) => eprintln!("capture failed: {e}"),
            }
            if self.config.captures.is_empty() && self.config.exit_after_captures {
                el.exit();
            }
        }
    }
}

fn record_frame(st: &mut State, rec: &Record) -> Result<(), String> {
    let (w, h, rgb) = st.renderer.capture_rgb()?;
    if st.recorder.is_none() {
        let f = File::create(&rec.path).map_err(|e| format!("{}: {e}", rec.path.display()))?;
        st.recorder = Some(Recorder { out: BufWriter::new(f), frames: 0, size: (w, h) });
    }
    let r = st.recorder.as_mut().unwrap();
    if (w, h) != r.size {
        return Err("window was resized during recording".into());
    }
    r.out.write_all(&rgb).map_err(|e| e.to_string())?;
    r.frames += 1;
    if r.frames as f64 >= rec.seconds * rec.fps as f64 {
        r.out.flush().map_err(|e| e.to_string())?;
        let meta = rec.path.with_extension("rgb.txt");
        std::fs::write(&meta, format!("{w} {h} {}\n", rec.fps)).map_err(|e| e.to_string())?;
        eprintln!("recorded {} frames ({w}x{h} @ {} fps) to {}", r.frames, rec.fps, rec.path.display());
        st.done = true;
    }
    Ok(())
}

fn print_bench(samples: &[f64], grid: &Grid, px: (u32, u32)) {
    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let avg = v.iter().sum::<f64>() / v.len() as f64;
    let p = |q: f64| v[((v.len() - 1) as f64 * q) as usize] * 1000.0;
    println!(
        "bench: {} frames, {}x{} cells, {}x{} px | avg {:.2} ms ({:.0} fps) | p50 {:.2} ms | p99 {:.2} ms ({:.0} fps) | max {:.2} ms",
        v.len(),
        grid.width(),
        grid.height(),
        px.0,
        px.1,
        avg * 1000.0,
        1.0 / avg,
        p(0.5),
        p(0.99),
        1000.0 / p(0.99),
        p(1.0),
    );
}

impl<S: Scene> ApplicationHandler for Handler<S> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        if let Err(e) = self.init(el) {
            self.error = Some(e);
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                if let Some(st) = self.state.as_mut() {
                    st.renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(st) = self.state.as_mut() {
                    match build_atlas(&self.config, scale_factor) {
                        Ok(a) => st.renderer.set_atlas(a),
                        Err(e) => eprintln!("atlas rebuild failed: {e}"),
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => el.exit(),
                    Key::Named(NamedKey::F1) => {
                        if let Some(st) = self.state.as_mut() {
                            st.hud.show = !st.hud.show;
                        }
                    }
                    k => self.scene.key(k),
                }
            }
            WindowEvent::RedrawRequested => self.frame(el),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(st) = &self.state {
            st.window.request_redraw();
        }
    }
}
