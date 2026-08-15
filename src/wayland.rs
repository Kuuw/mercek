use std::num::NonZeroU32;
use std::process::Command;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

use crate::render;

const MIN_ZOOM_SCALE: i64 = 25; // 2.5 * 10
const ZOOM_STEP: i64 = 5;       // 0.5 * 10
const MAX_ZOOM_SCALE: i64 = 320; // 32.0 * 10

/// Application state for the color picker overlay.
pub struct ColorPicker {
    // State
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,

    // Surface
    pool: SlotPool,
    layer: LayerSurface,
    width: u32,
    height: u32,
    first_configure: bool,

    // Input
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keyboard_focus: bool,
    pointer: Option<wl_pointer::WlPointer>,

    // Cursor
    cursor_x: f64,
    cursor_y: f64,
    // Lens Anchor position
    lens_x: f64,
    lens_y: f64,

    // Screenshot
    screenshot_rgba: Vec<u8>,
    screenshot_width: u32,
    screenshot_height: u32,

    // Rendering
    /// This locks the magnified area so that a color can be more easily hovered over.
    locked: bool,
    /// The zoom scale for the magnifier (scaled by 10, e.g. 25 = 2.5x).
    zoom_scale: i64,
    /// Set to true when the cursor moves or zoom changes; cleared after draw.
    needs_redraw: bool,
    /// True while we're waiting for a frame callback from the compositor.
    frame_pending: bool,

    // Result
    picked_color: Option<[u8; 4]>,
    exit: bool,
    frame_count: u64,
}

impl ColorPicker {
    /// Draws the overlay frame
    fn draw(&mut self, qh: &QueueHandle<Self>) {
        self.needs_redraw = false;
        self.frame_count += 1;

        let width = self.width;
        let height = self.height;
        if width == 0 || height == 0 {
            return;
        }

        let stride = width as i32 * 4;

        let (buffer, canvas) = self
            .pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("create buffer");

        let zoom_factor = self.zoom_scale as f32 / 10.0;

        // Render the full frame
        let color = render::render_frame(
            canvas,
            width,
            height,
            &self.screenshot_rgba,
            self.screenshot_width,
            self.screenshot_height,
            self.cursor_x,
            self.cursor_y,
            self.lens_x,
            self.lens_y,
            zoom_factor,
            self.locked,
        );

        self.picked_color = Some(color);

        // Damage the entire surface
        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);

        // Request a frame callback
        self.layer.wl_surface().frame(
            qh,
            FrameCallbackData(self.layer.wl_surface().clone()),
        );
        self.frame_pending = true;

        // Attach and commit
        buffer.attach_to(self.layer.wl_surface()).expect("buffer attach");
        self.layer.commit();
    }

    /// Copies the picked color to the Wayland clipboard via wl-copy.
    fn copy_to_clipboard(&self, color: [u8; 4]) {
        let hex = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
        println!("Picked color: {}", hex);

        match Command::new("wl-copy").arg(&hex).spawn() {
            Ok(_) => println!("Copied {} to clipboard", hex),
            Err(e) => eprintln!("Failed to copy to clipboard (is wl-copy installed?): {}", e),
        }
    }
}

impl CompositorHandler for ColorPicker {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.frame_pending = false;

        if self.needs_redraw {
            self.draw(qh);
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for ColorPicker {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = NonZeroU32::new(configure.new_size.0).map_or(self.width, NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(self.height, NonZeroU32::get);

        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
    }
}

impl SeatHandler for ColorPicker {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("Failed to create keyboard");
            self.keyboard = Some(keyboard);
        }

        if capability == Capability::Pointer && self.pointer.is_none() {
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
        }

        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for ColorPicker {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _keysyms: &[Keysym],
    ) {
        if self.layer.wl_surface() == surface {
            self.keyboard_focus = true;
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.layer.wl_surface() == surface {
            self.keyboard_focus = false;
        }
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            self.exit = true;
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _event: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: Modifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
    }
}

impl PointerHandler for ColorPicker {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }

            match &event.kind {
                PointerEventKind::Enter { serial } => {
                    _pointer.set_cursor(*serial, None, 0, 0);
                    self.cursor_x = event.position.0;
                    self.cursor_y = event.position.1;
                    if !self.locked {
                        self.lens_x = self.cursor_x;
                        self.lens_y = self.cursor_y;
                    }
                    self.needs_redraw = true;
                }
                PointerEventKind::Motion { .. } => {
                    self.cursor_x = event.position.0;
                    self.cursor_y = event.position.1;
                    if !self.locked {
                        self.lens_x = self.cursor_x;
                        self.lens_y = self.cursor_y;
                    }
                    self.needs_redraw = true;
                }
                PointerEventKind::Axis { vertical, .. } => {
                    let mut changed = false;

                    if vertical.discrete != 0 {
                        if vertical.discrete < 0 {
                            // Scrolled up -> Zoom In
                            self.zoom_scale = (self.zoom_scale + ZOOM_STEP).min(MAX_ZOOM_SCALE);
                            changed = true;
                        } else if vertical.discrete > 0 {
                            // Scrolled down -> Zoom Out
                            self.zoom_scale = (self.zoom_scale - ZOOM_STEP).max(MIN_ZOOM_SCALE);
                            changed = true;
                        }
                    } else if vertical.absolute != 0.0 {
                        // Smooth touchpad scrolling
                        if vertical.absolute < 0.0 {
                            self.zoom_scale = (self.zoom_scale + ZOOM_STEP).min(MAX_ZOOM_SCALE);
                            changed = true;
                        } else {
                            self.zoom_scale = (self.zoom_scale - ZOOM_STEP).max(MIN_ZOOM_SCALE);
                            changed = true;
                        }
                    }

                    if changed {
                        // Set locked state based on zoom scale
                        self.locked = self.zoom_scale > MIN_ZOOM_SCALE;

                        // If returning to minimum scale, snap the lens back to the cursor
                        if !self.locked {
                            self.lens_x = self.cursor_x;
                            self.lens_y = self.cursor_y;
                        }
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Press { button, .. } => {
                    if *button == 0x110 {
                        if let Some(color) = self.picked_color {
                            self.copy_to_clipboard(color);
                        }
                        self.exit = true;
                    }
                    if *button == 0x111 {
                        self.exit = true;
                    }
                }
                _ => {}
            }
        }

        if self.needs_redraw && !self.frame_pending {
            self.draw(qh);
        }
    }
}

impl ShmHandler for ColorPicker {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl OutputHandler for ColorPicker {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

delegate_registry!(ColorPicker);

impl ProvidesRegistryState for ColorPicker {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(ColorPicker);

// Entry Point

pub fn run_overlay(
    screenshot_rgba: Vec<u8>,
    screenshot_width: u32,
    screenshot_height: u32,
) -> Result<Option<[u8; 4]>, Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell not available");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm not available");

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some("colorful-picker"),
        None,
    );

    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_exclusive_zone(-1);
    layer.commit();

    let initial_pool_size = (screenshot_width as usize * screenshot_height as usize * 4) * 3;
    let pool = SlotPool::new(initial_pool_size.max(256 * 256 * 4), &shm)
        .expect("Failed to create SHM pool");

    let initial_x = screenshot_width as f64 / 2.0;
    let initial_y = screenshot_height as f64 / 2.0;

    let mut picker = ColorPicker {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,

        pool,
        layer,
        width: screenshot_width,
        height: screenshot_height,
        first_configure: true,

        keyboard: None,
        keyboard_focus: false,
        pointer: None,

        cursor_x: initial_x,
        cursor_y: initial_y,
        lens_x: initial_x,
        lens_y: initial_y,

        screenshot_rgba,
        screenshot_width,
        screenshot_height,

        locked: false,
        zoom_scale: MIN_ZOOM_SCALE,
        needs_redraw: false,
        frame_pending: false,

        picked_color: None,
        exit: false,
        frame_count: 0,
    };

    loop {
        event_queue.blocking_dispatch(&mut picker).unwrap();

        if picker.exit {
            break;
        }
    }

    Ok(picker.picked_color)
}