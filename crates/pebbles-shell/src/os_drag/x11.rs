//! X11 XDND (version 5) drag source (see the module docs in `os_drag`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ButtonMask, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, PropMode,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// The atoms the protocol uses.
#[derive(Clone, Copy, Debug)]
pub struct Atoms {
    pub aware: Atom,
    pub selection: Atom,
    pub enter: Atom,
    pub position: Atom,
    pub status: Atom,
    pub leave: Atom,
    pub drop: Atom,
    pub finished: Atom,
    pub action_copy: Atom,
    pub uri_list: Atom,
    pub utf8: Atom,
    pub text_plain: Atom,
}

impl Atoms {
    /// Intern them on `conn`.
    pub fn new(conn: &impl Connection) -> Result<Atoms, Box<dyn std::error::Error>> {
        let a = |n: &str| -> Result<Atom, Box<dyn std::error::Error>> {
            Ok(conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        Ok(Atoms {
            aware: a("XdndAware")?,
            selection: a("XdndSelection")?,
            enter: a("XdndEnter")?,
            position: a("XdndPosition")?,
            status: a("XdndStatus")?,
            leave: a("XdndLeave")?,
            drop: a("XdndDrop")?,
            finished: a("XdndFinished")?,
            action_copy: a("XdndActionCopy")?,
            uri_list: a("text/uri-list")?,
            utf8: a("UTF8_STRING")?,
            text_plain: a("text/plain")?,
        })
    }
}

/// How a drag ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Dropped on a target that accepted and finished.
    Dropped(Window),
    /// Released over nothing that accepted it.
    Cancelled,
    /// The target never answered the drop in time.
    TimedOut,
}

/// Start an XDND drag of `paths` from winit's window `own` on a helper thread.
pub(super) fn start(own: u32, paths: Vec<PathBuf>) -> bool {
    std::thread::Builder::new()
        .name("pebbles-xdnd".into())
        .spawn(move || {
            let Ok((conn, screen)) = RustConnection::connect(None) else { return };
            let root = conn.setup().roots[screen].root;
            let mut pointer = |c: &RustConnection| {
                let r = c.query_pointer(root).ok()?.reply().ok()?;
                let held = u16::from(r.mask) & u16::from(ButtonMask::M1) != 0;
                Some((r.root_x, r.root_y, held))
            };
            let _ = run(&conn, root, own, &paths, &mut pointer, Duration::from_secs(120));
        })
        .is_ok()
}

fn client_message(conn: &impl Connection, to: Window, kind: Atom, data: [u32; 5]) {
    let ev = ClientMessageEvent::new(32, to, kind, data);
    let _ = conn.send_event(false, to, EventMask::NO_EVENT, ev);
}

/// The XdndAware window under the pointer (descending from the root), with its version.
fn target_at(conn: &impl Connection, root: Window, atoms: &Atoms) -> Option<(Window, u32)> {
    let mut w = root;
    for _ in 0..32 {
        let r = conn.query_pointer(w).ok()?.reply().ok()?;
        if r.child == x11rb::NONE {
            return None;
        }
        w = r.child;
        if let Ok(p) = conn
            .get_property(false, w, atoms.aware, AtomEnum::ATOM, 0, 1)
            .map_err(|_| ())
            .and_then(|c| c.reply().map_err(|_| ()))
            && let Some(v) = p.value32().and_then(|mut i| i.next())
        {
            return Some((w, v.min(5)));
        }
    }
    None
}

/// Answer a `SelectionRequest` for `XdndSelection`.
fn answer(conn: &impl Connection, atoms: &Atoms, e: &SelectionRequestEvent, uris: &str, text: &str) {
    let prop = if e.property == x11rb::NONE { e.target } else { e.property };
    let data = if e.target == atoms.uri_list {
        Some(uris)
    } else if e.target == atoms.utf8
        || e.target == atoms.text_plain
        || e.target == u32::from(AtomEnum::STRING)
    {
        Some(text)
    } else {
        None
    };
    let property = match data {
        Some(d) => {
            let _ = conn.change_property8(PropMode::REPLACE, e.requestor, prop, e.target, d.as_bytes());
            prop
        }
        None => x11rb::NONE,
    };
    let ev = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: e.time,
        requestor: e.requestor,
        selection: e.selection,
        target: e.target,
        property,
    };
    let _ = conn.send_event(false, e.requestor, EventMask::NO_EVENT, ev);
    let _ = conn.flush();
}

/// Run one drag on `conn`: `pointer` reports `(root_x, root_y, button_held)` each poll;
/// `own` (the app's window) is never a target. Blocks until the drop finishes.
pub fn run<C: Connection>(
    conn: &C,
    root: Window,
    own: Window,
    paths: &[PathBuf],
    pointer: &mut dyn FnMut(&C) -> Option<(i16, i16, bool)>,
    limit: Duration,
) -> Result<Outcome, Box<dyn std::error::Error>> {
    let atoms = Atoms::new(conn)?;
    let src = conn.generate_id()?;
    conn.create_window(
        0,
        src,
        root,
        -10,
        -10,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new(),
    )?;
    conn.set_selection_owner(src, atoms.selection, x11rb::CURRENT_TIME)?;
    conn.flush()?;
    let (uris, text) = (super::uri_list(paths), super::plain_text(paths));
    let types = [atoms.uri_list, atoms.utf8, atoms.text_plain];
    let deadline = Instant::now() + limit;
    let mut current: Option<(Window, u32)> = None;
    let mut accepted = false;
    let mut last_pos = None;
    let mut outcome = Outcome::Cancelled;
    let mut dropped_at: Option<Instant> = None;
    // Released before the target's XdndStatus arrived: a few polls' grace for it.
    let mut grace = 0;
    loop {
        while let Some(ev) = conn.poll_for_event()? {
            match ev {
                Event::SelectionRequest(e) if e.selection == atoms.selection => {
                    answer(conn, &atoms, &e, &uris, &text)
                }
                Event::ClientMessage(m) if m.type_ == atoms.status => {
                    let d = m.data.as_data32();
                    if current.is_some_and(|(w, _)| w == d[0]) {
                        accepted = d[1] & 1 != 0;
                    }
                }
                Event::ClientMessage(m) if m.type_ == atoms.finished => {
                    let d = m.data.as_data32();
                    if dropped_at.is_some() && current.is_some_and(|(w, _)| w == d[0]) {
                        outcome = Outcome::Dropped(d[0]);
                        dropped_at = None;
                        current = None;
                    }
                }
                _ => {}
            }
        }
        if let Some(t) = dropped_at {
            if outcome != Outcome::Cancelled || t.elapsed() > Duration::from_secs(5) {
                if outcome == Outcome::Cancelled {
                    outcome = Outcome::TimedOut;
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        if matches!(outcome, Outcome::Dropped(_)) || Instant::now() > deadline {
            break;
        }
        let Some((x, y, held)) = pointer(conn) else { break };
        let under = target_at(conn, root, &atoms).filter(|(w, _)| *w != own);
        if under.map(|u| u.0) != current.map(|c| c.0) {
            if let Some((w, _)) = current {
                client_message(conn, w, atoms.leave, [src, 0, 0, 0, 0]);
            }
            current = under;
            accepted = false;
            last_pos = None;
            if let Some((w, v)) = current {
                client_message(conn, w, atoms.enter, [src, v << 24, types[0], types[1], types[2]]);
            }
        }
        if let Some((w, _)) = current
            && last_pos != Some((x, y))
        {
            let xy = ((x as u16 as u32) << 16) | (y as u16 as u32);
            client_message(conn, w, atoms.position, [src, 0, xy, x11rb::CURRENT_TIME, atoms.action_copy]);
            last_pos = Some((x, y));
        }
        conn.flush()?;
        if !held && current.is_some() && !accepted && grace < 25 {
            grace += 1;
            std::thread::sleep(Duration::from_millis(8));
            continue;
        }
        if !held {
            match current {
                Some((w, _)) if accepted => {
                    client_message(conn, w, atoms.drop, [src, 0, x11rb::CURRENT_TIME, 0, 0]);
                    conn.flush()?;
                    dropped_at = Some(Instant::now());
                    continue;
                }
                Some((w, _)) => {
                    client_message(conn, w, atoms.leave, [src, 0, 0, 0, 0]);
                    conn.flush()?;
                    break;
                }
                None => break,
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    let _ = conn.destroy_window(src);
    let _ = conn.flush();
    Ok(outcome)
}
