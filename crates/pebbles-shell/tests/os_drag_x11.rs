//! The X11 XDND drag source against a real X server (Xvfb) and a real XDND target window:
//! enter / position / status / drop / selection conversion / finished, the `text/uri-list`
//! body byte for byte, and a release over nothing cancels. Skips when Xvfb is missing.
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use pebbles_shell::os_drag::x11::{Atoms, Outcome, run};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

struct Xvfb(Child, String);

impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn xvfb() -> Option<Xvfb> {
    let n = 40 + std::process::id() % 400;
    let display = format!(":{n}");
    let child = Command::new("Xvfb")
        .args([display.as_str(), "-screen", "0", "800x600x24", "-nolisten", "tcp"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let x = Xvfb(child, display);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if RustConnection::connect(Some(&x.1)).is_ok() {
            return Some(x);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// A mapped XdndAware window at (0, 0) 400×400 on a new connection.
fn target_window(display: &str) -> (RustConnection, Window, Atoms) {
    let (c, screen) = RustConnection::connect(Some(display)).unwrap();
    let root = c.setup().roots[screen].root;
    let w = c.generate_id().unwrap();
    c.create_window(
        0,
        w,
        root,
        0,
        0,
        400,
        400,
        0,
        WindowClass::INPUT_OUTPUT,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .unwrap();
    let atoms = Atoms::new(&c).unwrap();
    c.change_property32(PropMode::REPLACE, w, atoms.aware, AtomEnum::ATOM, &[5]).unwrap();
    c.map_window(w).unwrap();
    c.flush().unwrap();
    (c, w, atoms)
}

/// Play the target: accept positions, convert the selection on drop, finish. Returns the
/// offered types and the dropped `text/uri-list` bytes.
fn serve(c: RustConnection, w: Window, atoms: Atoms) -> (Vec<u32>, Vec<u8>) {
    let mut types = Vec::new();
    let mut source = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    let prop = c.intern_atom(false, b"PEBBLES_DROP").unwrap().reply().unwrap().atom;
    while Instant::now() < deadline {
        let Some(ev) = c.poll_for_event().unwrap() else {
            std::thread::sleep(Duration::from_millis(3));
            continue;
        };
        match ev {
            Event::ClientMessage(m) if m.type_ == atoms.enter => {
                let d = m.data.as_data32();
                source = d[0];
                assert_eq!(d[1] >> 24, 5, "XDND version");
                types = d[2..].to_vec();
            }
            Event::ClientMessage(m) if m.type_ == atoms.position => {
                let d = m.data.as_data32();
                assert_eq!(d[4], atoms.action_copy);
                let ev = ClientMessageEvent::new(32, d[0], atoms.status, [w, 1, 0, 0, atoms.action_copy]);
                c.send_event(false, d[0], EventMask::NO_EVENT, ev).unwrap();
                c.flush().unwrap();
            }
            Event::ClientMessage(m) if m.type_ == atoms.drop => {
                let d = m.data.as_data32();
                assert_eq!(d[0], source);
                c.convert_selection(w, atoms.selection, atoms.uri_list, prop, d[2]).unwrap();
                c.flush().unwrap();
            }
            Event::SelectionNotify(n) => {
                assert_eq!(n.property, prop);
                let r = c.get_property(true, w, prop, AtomEnum::ANY, 0, 1 << 20).unwrap().reply().unwrap();
                assert_eq!(r.type_, atoms.uri_list);
                let ev = ClientMessageEvent::new(32, source, atoms.finished, [w, 1, atoms.action_copy, 0, 0]);
                c.send_event(false, source, EventMask::NO_EVENT, ev).unwrap();
                c.flush().unwrap();
                return (types, r.value);
            }
            _ => {}
        }
    }
    panic!("no drop arrived");
}

#[test]
fn xdnd_source_drops_a_uri_list_on_an_xdnd_target() {
    let Some(x) = xvfb() else {
        eprintln!("skipping: Xvfb not available");
        return;
    };
    let (tc, tw, atoms) = target_window(&x.1);
    let target = std::thread::spawn(move || serve(tc, tw, atoms));

    let (c, screen) = RustConnection::connect(Some(&x.1)).unwrap();
    let root = c.setup().roots[screen].root;
    // The press happened in the app; the pointer moves over the target, then releases.
    c.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 120, 80).unwrap();
    c.flush().unwrap();
    let mut polls = 0;
    let mut pointer = |c: &RustConnection| {
        polls += 1;
        let r = c.query_pointer(root).ok()?.reply().ok()?;
        Some((r.root_x, r.root_y, polls < 10))
    };
    let paths = vec![PathBuf::from("/tmp/a b/é.txt"), PathBuf::from("/tmp/c")];
    let outcome = run(&c, root, 0, &paths, &mut pointer, Duration::from_secs(20)).unwrap();
    assert_eq!(outcome, Outcome::Dropped(tw));
    let (types, data) = target.join().unwrap();
    assert_eq!(types, vec![atoms.uri_list, atoms.utf8, atoms.text_plain]);
    assert_eq!(String::from_utf8(data).unwrap(), "file:///tmp/a%20b/%C3%A9.txt\r\nfile:///tmp/c\r\n");

    // Released over the bare root window: nothing accepts it.
    c.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 700, 500).unwrap();
    c.flush().unwrap();
    let mut pointer = |c: &RustConnection| {
        let r = c.query_pointer(root).ok()?.reply().ok()?;
        Some((r.root_x, r.root_y, false))
    };
    let outcome = run(&c, root, 0, &paths, &mut pointer, Duration::from_secs(5)).unwrap();
    assert_eq!(outcome, Outcome::Cancelled);

    // The app's own window is never a target, even though it is XdndAware.
    c.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 120, 80).unwrap();
    c.flush().unwrap();
    let mut pointer = |c: &RustConnection| {
        let r = c.query_pointer(root).ok()?.reply().ok()?;
        Some((r.root_x, r.root_y, false))
    };
    let outcome = run(&c, root, tw, &paths, &mut pointer, Duration::from_secs(5)).unwrap();
    assert_eq!(outcome, Outcome::Cancelled);
}
