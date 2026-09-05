//! Web IME bridge — opt-in via [`App::web_ime`](crate::App::web_ime).
//!
//! winit's web backend cannot deliver IME/composition or a soft keyboard, because
//! a `<canvas>` element can't receive a `CompositionEvent` (winit#4424). The
//! standard workaround (egui does the same) is a **hidden `<input>`**: while a
//! Pebbles text editor is focused we focus that input, capture composition + typed
//! text + editing keys there, and forward them as [`KeyInput`] into the focused
//! editor. Latin typing, CJK/IME composition, and the mobile soft keyboard all go
//! through it.
//!
//! **Off by default** — enabling it is `App::web_ime(true)`. When off, nothing here
//! runs and text input is winit's `KeyboardInput` exactly as before. When on, the
//! shell suppresses winit's key handling *while an editor is focused* (the hidden
//! input owns it), so there's no double entry.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use pebbles_core::keyboard::{KeyInput, Motion};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CompositionEvent, HtmlInputElement, InputEvent, KeyboardEvent};

use crate::app::runner::PebblesUserEvent;
use winit::event_loop::EventLoopProxy;

struct WebIme {
    input: HtmlInputElement,
    focused: bool,
    /// Keep the JS closures alive for the element's lifetime.
    _closures: Vec<Closure<dyn FnMut(web_sys::Event)>>,
}

thread_local! {
    static IME: RefCell<Option<WebIme>> = const { RefCell::new(None) };
    static QUEUE: RefCell<VecDeque<KeyInput>> = const { RefCell::new(VecDeque::new()) };
    /// True between a `compositionend` and the trailing `input` some browsers fire,
    /// so we don't insert the committed text twice.
    static SKIP_NEXT_INPUT: Cell<bool> = const { Cell::new(false) };
}

fn push(proxy: &EventLoopProxy<PebblesUserEvent>, key: KeyInput) {
    QUEUE.with(|q| q.borrow_mut().push_back(key));
    // Wake the winit loop so it drains + dispatches this on the UI turn.
    let _ = proxy.send_event(PebblesUserEvent::WebImeInput);
}

/// Drain the queued edit intents (called by the runner on `WebImeInput`).
pub(crate) fn drain() -> Vec<KeyInput> {
    QUEUE.with(|q| q.borrow_mut().drain(..).collect())
}

/// Create the hidden input + listeners (once). No-op if already installed or if the
/// DOM isn't available.
pub(crate) fn enable(proxy: EventLoopProxy<PebblesUserEvent>) {
    if IME.with(|i| i.borrow().is_some()) {
        return;
    }
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Some(body) = doc.body() else { return };
    let Ok(el) = doc.create_element("input") else { return };
    let Ok(input) = el.dyn_into::<HtmlInputElement>() else { return };
    input.set_type("text");
    input.set_attribute("autocapitalize", "off").ok();
    input.set_attribute("autocorrect", "off").ok();
    input.set_attribute("autocomplete", "off").ok();
    input.set_attribute("spellcheck", "false").ok();
    // Invisible but focusable (so it raises the soft keyboard + hosts IME), and out
    // of the layout so it never shows or scrolls the page.
    let _ = input.set_attribute(
        "style",
        "position:absolute; top:0; left:0; width:1px; height:1px; opacity:0; \
         border:0; padding:0; margin:0; z-index:-1; pointer-events:none;",
    );
    let _ = body.append_child(&input);

    let mut closures: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();

    // compositionstart / compositionupdate → preedit (underlined, uncommitted).
    for evt in ["compositionstart", "compositionupdate"] {
        let p = proxy.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let data = e.dyn_ref::<CompositionEvent>().and_then(|c| c.data()).unwrap_or_default();
            push(&p, KeyInput::Preedit(data));
        });
        let _ = input.add_event_listener_with_callback(evt, cb.as_ref().unchecked_ref());
        closures.push(cb);
    }

    // compositionend → commit the composed text; clear preedit + the input buffer.
    {
        let p = proxy.clone();
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let data = e.dyn_ref::<CompositionEvent>().and_then(|c| c.data()).unwrap_or_default();
            push(&p, KeyInput::Preedit(String::new()));
            if !data.is_empty() {
                push(&p, KeyInput::Insert(data));
            }
            SKIP_NEXT_INPUT.with(|s| s.set(true));
            input_c.set_value("");
        });
        let _ = input.add_event_listener_with_callback("compositionend", cb.as_ref().unchecked_ref());
        closures.push(cb);
    }

    // input (not composing) → a committed character/paste; forward + clear buffer.
    {
        let p = proxy.clone();
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let ie = e.dyn_ref::<InputEvent>();
            if ie.is_some_and(|i| i.is_composing()) {
                return; // handled by compositionupdate
            }
            if SKIP_NEXT_INPUT.with(|s| s.replace(false)) {
                return; // the trailing input after compositionend
            }
            let text = ie.and_then(|i| i.data()).unwrap_or_else(|| input_c.value());
            if !text.is_empty() {
                push(&p, KeyInput::Insert(text));
            }
            input_c.set_value("");
        });
        let _ = input.add_event_listener_with_callback("input", cb.as_ref().unchecked_ref());
        closures.push(cb);
    }

    // keydown → editing/navigation keys (the browser won't turn these into `input`).
    {
        let p = proxy.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let Some(ke) = e.dyn_ref::<KeyboardEvent>() else { return };
            if ke.is_composing() {
                return; // let the IME own it
            }
            let (ctrl, shift, meta) = (ke.ctrl_key(), ke.shift_key(), ke.meta_key());
            let cmd = ctrl || meta; // Ctrl (Win/Linux) or ⌘ (macOS)
            let mv = |m: Motion| KeyInput::Move { motion: m, extend: shift };
            let key = match ke.key().as_str() {
                "Backspace" => Some(if cmd { KeyInput::DeleteWordBack } else { KeyInput::Backspace }),
                "Delete" => Some(if cmd { KeyInput::DeleteWordForward } else { KeyInput::Delete }),
                "Enter" => Some(KeyInput::Enter),
                "Escape" => Some(KeyInput::Escape),
                "ArrowLeft" => Some(mv(if cmd { Motion::WordLeft } else { Motion::Left })),
                "ArrowRight" => Some(mv(if cmd { Motion::WordRight } else { Motion::Right })),
                "ArrowUp" => Some(mv(Motion::Up)),
                "ArrowDown" => Some(mv(Motion::Down)),
                "Home" => Some(mv(if cmd { Motion::DocStart } else { Motion::LineStart })),
                "End" => Some(mv(if cmd { Motion::DocEnd } else { Motion::LineEnd })),
                "a" | "A" if cmd => Some(KeyInput::SelectAll),
                "c" | "C" if cmd => Some(KeyInput::Copy),
                "x" | "X" if cmd => Some(KeyInput::Cut),
                "v" | "V" if cmd => Some(KeyInput::Paste),
                "z" | "Z" if cmd => Some(if shift { KeyInput::Redo } else { KeyInput::Undo }),
                "y" | "Y" if cmd => Some(KeyInput::Redo),
                _ => None,
            };
            if let Some(k) = key {
                e.prevent_default(); // don't let the browser also act on the input buffer
                push(&p, k);
            }
        });
        let _ = input.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
        closures.push(cb);
    }

    IME.with(|i| *i.borrow_mut() = Some(WebIme { input, focused: false, _closures: closures }));
}

/// Match the hidden input's focus to whether a Pebbles text editor is focused, so
/// the soft keyboard + IME appear exactly when editing. Called each turn.
pub(crate) fn sync_focus() {
    let want = pebbles_core::focus::focused_is_editor();
    IME.with(|i| {
        if let Some(ime) = i.borrow_mut().as_mut()
            && ime.focused != want
        {
            ime.focused = want;
            if want {
                let _ = ime.input.focus();
            } else {
                ime.input.set_value("");
                let _ = ime.input.blur();
            }
        }
    });
}
