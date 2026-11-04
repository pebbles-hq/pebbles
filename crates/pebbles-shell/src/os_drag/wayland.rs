//! Wayland drag source (see the module docs in `os_drag`).

use std::io::Write as _;
use std::os::fd::AsFd as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use wayland_client::backend::{Backend, ObjectId};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::WlDataOffer;
use wayland_client::protocol::wl_data_source::{self, WlDataSource};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, event_created_child};

const URI_LIST: &str = "text/uri-list";
const PLAIN: &str = "text/plain;charset=utf-8";
const PLAIN_LEGACY: &str = "text/plain";
const UTF8: &str = "UTF8_STRING";

/// What the UI thread needs to start a drag.
struct Shared {
    conn: Connection,
    qh: QueueHandle<State>,
    manager: Option<WlDataDeviceManager>,
    device: Option<WlDataDevice>,
    /// The serial of the last primary-button press (the implicit grab a drag continues).
    press_serial: Option<u32>,
    /// The offered data of the drag in progress.
    payload: Option<(String, String)>,
}

static SHARED: OnceLock<Arc<Mutex<Shared>>> = OnceLock::new();

#[derive(Default)]
struct State {
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
}

fn shared() -> Option<&'static Arc<Mutex<Shared>>> {
    SHARED.get()
}

/// Bind the seat / pointer / data device on winit's display once, and dispatch them on a
/// helper thread.
pub(super) fn init(display: *mut std::ffi::c_void) {
    if SHARED.get().is_some() {
        return;
    }
    // SAFETY: winit's wl_display outlives the app (it lives in the event loop), and this
    // backend never closes it (foreign display).
    let backend = unsafe { Backend::from_foreign_display(display.cast()) };
    let conn = Connection::from_backend(backend);
    let mut queue: EventQueue<State> = conn.new_event_queue();
    let qh = queue.handle();
    let display = conn.display();
    let registry = display.get_registry(&qh, ());
    let _ = registry;
    let shared = Arc::new(Mutex::new(Shared {
        conn: conn.clone(),
        qh: qh.clone(),
        manager: None,
        device: None,
        press_serial: None,
        payload: None,
    }));
    if SHARED.set(shared).is_err() {
        return;
    }
    let mut state = State::default();
    // Globals, then the seat's capabilities (the pointer), before the first drag.
    if queue.roundtrip(&mut state).is_err() || queue.roundtrip(&mut state).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("pebbles-wl-drag".into())
        .spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {});
}

/// Start dragging `paths` from winit's `surface`.
pub(super) fn start(surface: *mut std::ffi::c_void, paths: Vec<PathBuf>) -> bool {
    let Some(sh) = shared() else { return false };
    let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
    let (Some(manager), Some(device), Some(serial)) = (s.manager.clone(), s.device.clone(), s.press_serial)
    else {
        return false;
    };
    // SAFETY: `surface` is winit's live wl_surface for this window.
    let Ok(id) = (unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }) else {
        return false;
    };
    let Ok(origin) = WlSurface::from_id(&s.conn, id) else { return false };
    s.payload = Some((super::uri_list(&paths), super::plain_text(&paths)));
    let source = manager.create_data_source(&s.qh, ());
    for m in [URI_LIST, PLAIN, PLAIN_LEGACY, UTF8] {
        source.offer(m.to_string());
    }
    if source.version() >= 3 {
        source.set_actions(DndAction::Copy);
    }
    device.start_drag(Some(&source), &origin, None, serial);
    let _ = s.conn.flush();
    true
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global { name, interface, version } = event else { return };
        let Some(sh) = shared() else { return };
        match interface.as_str() {
            "wl_seat" if state.seat.is_none() => {
                let seat: WlSeat = registry.bind(name, version.min(7), qh, ());
                state.seat = Some(seat);
            }
            "wl_data_device_manager" => {
                let m: WlDataDeviceManager = registry.bind(name, version.min(3), qh, ());
                sh.lock().unwrap_or_else(|e| e.into_inner()).manager = Some(m);
            }
            _ => return,
        }
        let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
        if s.device.is_none()
            && let (Some(m), Some(seat)) = (s.manager.clone(), state.seat.clone())
        {
            s.device = Some(m.get_data_device(&seat, qh, ()));
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(c) } = event {
            if c.contains(wl_seat::Capability::Pointer) {
                if state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                }
            } else if let Some(p) = state.pointer.take()
                && p.version() >= 3
            {
                p.release();
            }
        }
    }
}

impl Dispatch<WlPointer, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // BTN_LEFT (linux/input-event-codes.h).
        if let wl_pointer::Event::Button {
            serial,
            button: 0x110,
            state: WEnum::Value(wl_pointer::ButtonState::Pressed),
            ..
        } = event
            && let Some(sh) = shared()
        {
            sh.lock().unwrap_or_else(|e| e.into_inner()).press_serial = Some(serial);
        }
    }
}

impl Dispatch<WlDataSource, ()> for State {
    fn event(
        _: &mut Self,
        source: &WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let Some(sh) = shared() else { return };
                let data =
                    {
                        let s = sh.lock().unwrap_or_else(|e| e.into_inner());
                        s.payload.as_ref().map(|(uris, text)| {
                            if mime_type == URI_LIST { uris.clone() } else { text.clone() }
                        })
                    };
                if let Some(d) = data
                    && let Ok(owned) = fd.as_fd().try_clone_to_owned()
                {
                    let mut f = std::fs::File::from(owned);
                    let _ = f.write_all(d.as_bytes());
                }
            }
            wl_data_source::Event::Cancelled | wl_data_source::Event::DndFinished => {
                source.destroy();
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // This device only sources drags; offers to it (selection / drops) are winit's
        // business on its own device — release ours.
        if let wl_data_device::Event::DataOffer { id } = event {
            id.destroy();
        }
    }

    event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataOffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlDataOffer,
        _: wayland_client::protocol::wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlDataDeviceManager,
        _: wayland_client::protocol::wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
