//! The desktop app runner: the public [`App`] builder. The winit
//! [`ApplicationHandler`](winit::application::ApplicationHandler) engine that
//! drives it lives in the private [`runner`] child module.

use pebbles_core::IntoWidget;
use pebbles_foundation::{Color, TextDirection, palette};
use pebbles_widgets::MenuBar;
use winit::event_loop::EventLoop;

mod runner;

use runner::Runner;

/// A Pebbles desktop application. Configure it fluently, then [`run`](App::run).
///
/// ```ignore
/// App::new(my_root_widget())
///     .title("Counter")
///     .size(480, 320)
///     .run()?;
/// ```
pub struct App {
    title: String,
    background: Color,
    size: (u32, u32),
    min_size: Option<(u32, u32)>,
    max_size: Option<(u32, u32)>,
    position: Option<(i32, i32)>,
    resizable: bool,
    maximized: bool,
    decorations: bool,
    root: Option<pebbles_core::AnyWidget>,
    /// B3 native menu bar spec. Consumed by the shell only when the `native-menus`
    /// feature is on; retained (unused) otherwise so app code compiles either way.
    menu: Option<MenuBar>,
    /// D2 global text direction, applied at mount (default LTR).
    text_direction: TextDirection,
}

impl App {
    /// Create an app with `root` as its top-level widget. The root is wrapped in an
    /// [`OverlayHost`](pebbles_widgets::OverlayHost) so dropdowns/menus/popovers can
    /// paint above everything.
    pub fn new(root: impl IntoWidget) -> Self {
        App {
            title: "Pebbles".to_owned(),
            background: palette::WHITE,
            size: (800, 600),
            min_size: None,
            max_size: None,
            position: None,
            resizable: true,
            maximized: false,
            decorations: true,
            root: Some(pebbles_widgets::OverlayHost::wrap(root).into_widget()),
            menu: None,
            text_direction: TextDirection::Ltr,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// The window background color (also the root `View`'s fill).
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = (width, height);
        self
    }

    /// The smallest the user can resize the window to (logical px).
    pub fn min_size(mut self, width: u32, height: u32) -> Self {
        self.min_size = Some((width, height));
        self
    }

    /// The largest the user can resize the window to (logical px).
    pub fn max_size(mut self, width: u32, height: u32) -> Self {
        self.max_size = Some((width, height));
        self
    }

    /// The window's initial top-left position (logical px).
    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }

    /// Whether the user can resize the window (default `true`).
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Open the window maximized.
    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    /// Whether the OS draws the title bar / borders (default `true`).
    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Attach a native OS menu bar (B3) — macOS global menu / Windows window menu.
    /// Built from [`menu_bar`](pebbles_widgets::menu_bar); only takes effect when the
    /// `native-menus` feature is enabled (otherwise the spec is retained but unused,
    /// and the in-window [`menubar`](pebbles_widgets::components::menubar) stays the
    /// cross-platform form).
    ///
    /// ```ignore
    /// use pebbles_widgets::{menu, menu_bar};
    /// App::new(root).menu(menu_bar([
    ///     menu("File", [menu_item("Quit").shortcut("Mod+Q").into()]),
    /// ]))
    /// ```
    pub fn menu(mut self, bar: MenuBar) -> Self {
        self.menu = Some(bar);
        self
    }

    /// Set the global text direction (D2). `Rtl` reverses Row child order + mirrors
    /// Start/End alignment, and sets paragraphs' bidi base direction. Applied at mount;
    /// toggle at runtime with [`pebbles_widgets::set_text_direction`].
    pub fn text_direction(mut self, dir: TextDirection) -> Self {
        self.text_direction = dir;
        self
    }

    /// Register a user-supplied font (F4), repeatable. `bytes` is `'static` (embed with
    /// `include_bytes!` or leak an `Arc`) so it outlives every window's font collection.
    /// Every window then resolves the font's family via `style().font_family("…")`.
    /// Registered globally at call time, so call this before [`run`](App::run).
    pub fn font(self, bytes: &'static [u8]) -> Self {
        pebbles_render::register_user_font(bytes);
        self
    }

    /// Open the window and run the event loop until the window closes.
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        use pebbles_core::log;
        log::init();
        // A panic anywhere in the UI dumps the whole event log first, so we always
        // see what the UI was doing in the run-up to the crash — then the normal
        // panic message/backtrace.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error(log::Cat::General, format!("PANIC: {info}"));
            log::dump("panic");
            prev(info);
        }));
        log::info(log::Cat::General, "pebbles app starting");
        if log::dev_mode() {
            log::info(
                log::Cat::General,
                "dev mode — devtools: Mod+Shift+I inspect widget · Mod+Shift+D dump render tree + logs",
            );
        }
        let event_loop = EventLoop::new()?;
        let mut runner = Runner::new(self);
        event_loop.run_app(&mut runner)?;
        log::info(log::Cat::General, "pebbles app exited cleanly");
        Ok(())
    }
}
