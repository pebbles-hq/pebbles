//! [`ImageView`] — display an image from an **asset** (file), a **URL** (fetched
//! and decoded on a background thread), **base64** (incl. `data:` URIs), or raw
//! **memory** bytes. Network loads show a `placeholder` while in flight and an
//! `error` widget on failure. Fit modes mirror CSS `object-fit` ([`ImageFit`]).
//!
//! Async model: a network load runs in the background via [`pebbles_core::spawn`];
//! the decoded result is delivered back on the UI thread (drained by the shared task
//! pump once per frame) into the result signal, so the view re-renders.

#[cfg(not(target_family = "wasm"))]
use std::io::Read;
#[cfg(not(target_family = "wasm"))]
use std::time::Duration;

use base64::Engine;
use pebbles_foundation::{Alignment, EdgeInsets};
use pebbles_render::{BorderRadius, BoxDecoration, Image, ImageFit, image_from_rgba8};

use crate::theme::theme;
use crate::widgets::{Container, Opacity, spinner, stack, text};
#[cfg(not(target_family = "wasm"))]
use pebbles_core::spawn;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animated, component_props, create_effect, create_signal};

// ---------------------------------------------------------------------------
// Async network loader
// ---------------------------------------------------------------------------

/// The load state of a network image.
#[derive(Clone)]
pub enum ImageState {
    Loading,
    Loaded(Image),
    Failed(String),
}

/// Decode PNG/JPEG/GIF/WebP bytes into a paintable [`Image`].
fn decode(bytes: &[u8]) -> Result<Image, String> {
    let rgba = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(image_from_rgba8(w, h, rgba.into_raw()))
}

/// Decode base64 (optionally a `data:...;base64,` URI) into an [`Image`].
fn decode_base64(data: &str) -> Result<Image, String> {
    let payload = data.rsplit_once("base64,").map(|(_, b)| b).unwrap_or(data);
    let bytes =
        base64::engine::general_purpose::STANDARD.decode(payload.trim()).map_err(|e| e.to_string())?;
    decode(&bytes)
}

/// Fetch a URL and decode it (runs on a background thread). Native uses the
/// blocking `ureq` client; the 32 MB cap guards against a runaway response.
#[cfg(not(target_family = "wasm"))]
fn fetch(url: &str) -> Result<Image, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(32 * 1024 * 1024) // 32 MB cap
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    decode(&bytes)
}

/// On the web there is no blocking HTTP client (`ureq`/`ring` don't build for wasm), so
/// remote images load via the browser's own `fetch`: GET the URL, read the response as an
/// ArrayBuffer, and decode with the same pure-Rust `image` codecs. Async (returns a
/// future) — the caller drives it with `spawn_local_future` so the result lands via the
/// normal task pump. Cross-origin URLs must send CORS headers, as for any web fetch.
#[cfg(target_family = "wasm")]
async fn fetch(url: String) -> Result<Image, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let resp_value =
        JsFuture::from(window.fetch_with_str(&url)).await.map_err(|e| format!("fetch failed: {e:?}"))?;
    let resp: web_sys::Response =
        resp_value.dyn_into().map_err(|_| "fetch did not return a Response".to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }
    let buffer = JsFuture::from(resp.array_buffer().map_err(|e| format!("no body: {e:?}"))?)
        .await
        .map_err(|e| format!("reading body failed: {e:?}"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    decode(&bytes)
}

/// The network load state: the load effect is position-stable, so the URL is a
/// SIGNAL it reads — a source change re-fetches, and nothing else re-runs it.
/// The fetch runs in the background via [`pebbles_core::spawn`]; its result is
/// delivered back on the UI thread into `state`.
fn use_network(url: Signal<String>) -> Signal<ImageState> {
    let state = create_signal(ImageState::Loading);
    create_effect(move || {
        let url = url.get(); // subscribe — the effect re-runs when the URL changes
        if url.is_empty() {
            return;
        }
        state.set(ImageState::Loading);
        load(url, state);
    });
    state
}

/// Kick off the URL load and deliver the result into `state` on the UI thread. Native
/// runs the blocking `fetch` on a background thread via [`spawn`]; the web drives the
/// async browser `fetch` via `spawn_local_future` — both land through the same task pump.
#[cfg(not(target_family = "wasm"))]
fn load(url: String, state: Signal<ImageState>) {
    spawn(
        move || match fetch(&url) {
            Ok(img) => ImageState::Loaded(img),
            Err(e) => ImageState::Failed(e),
        },
        move |result| state.set(result), // UI thread; a no-op if the view unmounted
    );
}

#[cfg(target_family = "wasm")]
fn load(url: String, state: Signal<ImageState>) {
    pebbles_core::spawn_local_future(fetch(url), move |result| {
        state.set(match result {
            Ok(img) => ImageState::Loaded(img),
            Err(e) => ImageState::Failed(e),
        });
    });
}

// ---------------------------------------------------------------------------
// ImageView
// ---------------------------------------------------------------------------

enum Source {
    /// Decoded synchronously at construction (asset / memory / base64 / direct).
    Ready(Result<Image, String>),
    /// Fetched + decoded asynchronously.
    Network(String),
}

/// Displays an image. Build with [`ImageView::asset`] / [`network`](ImageView::network)
/// / [`memory`](ImageView::memory) / [`base64`](ImageView::base64).
pub struct ImageView {
    source: Source,
    fit: ImageFit,
    width: Option<f64>,
    height: Option<f64>,
    radius: Option<BorderRadius>,
    placeholder: Option<AnyWidget>,
    error: Option<AnyWidget>,
}

fn make(source: Source) -> ImageView {
    ImageView {
        source,
        fit: ImageFit::Cover,
        width: None,
        height: None,
        radius: None,
        placeholder: None,
        error: None,
    }
}

impl ImageView {
    /// An image decoded from a file on disk.
    pub fn asset(path: impl AsRef<std::path::Path>) -> Self {
        let result = std::fs::read(path).map_err(|e| e.to_string()).and_then(|bytes| decode(&bytes));
        make(Source::Ready(result))
    }
    /// An image fetched from a URL (loads on a background thread).
    ///
    /// # Platform support
    /// Native (desktop/mobile) only for now. On **web** a remote URL renders the
    /// `error` widget until the browser `fetch` backend lands — use
    /// [`asset`](Self::asset)/[`memory`](Self::memory)/[`base64`](Self::base64) or
    /// a `data:` URI, which work on every platform. See `PLATFORMS.md`.
    pub fn network(url: impl Into<String>) -> Self {
        make(Source::Network(url.into()))
    }
    /// An image decoded from in-memory encoded bytes (PNG/JPEG/…).
    pub fn memory(bytes: impl AsRef<[u8]>) -> Self {
        make(Source::Ready(decode(bytes.as_ref())))
    }
    /// An image decoded from a base64 string or `data:` URI.
    pub fn base64(data: impl AsRef<str>) -> Self {
        make(Source::Ready(decode_base64(data.as_ref())))
    }
    /// An already-decoded [`Image`].
    pub fn image(image: Image) -> Self {
        make(Source::Ready(Ok(image)))
    }

    /// How the image scales to its box (default: `Cover`).
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }
    pub fn size(self, width: f64, height: f64) -> Self {
        self.width(width).height(height)
    }
    /// Round the image's corners (and clip to them).
    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = Some(radius);
        self
    }
    /// A widget shown while a network image is loading.
    pub fn placeholder(mut self, widget: impl IntoWidget) -> Self {
        self.placeholder = Some(widget.into_widget());
        self
    }
    /// A widget shown if the image fails to load/decode.
    pub fn error(mut self, widget: impl IntoWidget) -> Self {
        self.error = Some(widget.into_widget());
        self
    }
}

struct Props {
    source: Source,
    fit: ImageFit,
    width: Option<f64>,
    height: Option<f64>,
    radius: Option<BorderRadius>,
    placeholder: Option<AnyWidget>,
    error: Option<AnyWidget>,
}

impl IntoWidget for ImageView {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_image_view,
            Props {
                source: self.source,
                fit: self.fit,
                width: self.width,
                height: self.height,
                radius: self.radius,
                placeholder: self.placeholder,
                error: self.error,
            },
        )
        .into_widget()
    }
}

fn sized(mut c: Container, p: &Props) -> Container {
    if let Some(w) = p.width {
        c = c.width(w);
    }
    if let Some(h) = p.height {
        c = c.height(h);
    }
    c
}

fn image_box(img: &Image, p: &Props) -> AnyWidget {
    let mut deco = BoxDecoration::new().image(img.clone()).image_fit(p.fit);
    if let Some(r) = p.radius {
        deco = deco.radius(r);
    }
    let mut c = Container::new().decoration(deco);
    if p.radius.is_some() {
        c = c.clip();
    }
    sized(c, p).into_widget()
}

/// A neutral filler used when no placeholder/error widget is supplied.
fn filler(p: &Props, child: AnyWidget) -> AnyWidget {
    let c = theme().colors;
    let mut deco = BoxDecoration::new().color(c.secondary);
    if let Some(r) = p.radius {
        deco = deco.radius(r);
    }
    sized(Container::new().decoration(deco).alignment(Alignment::CENTER).child(child), p).into_widget()
}

fn placeholder_box(p: &Props) -> AnyWidget {
    match &p.placeholder {
        Some(w) => w.clone(),
        None => filler(p, spinner(22.0).color(theme().colors.muted_foreground).into_widget()),
    }
}

fn error_box(p: &Props) -> AnyWidget {
    match &p.error {
        Some(w) => w.clone(),
        None => filler(
            p,
            Container::new()
                .padding(EdgeInsets::all(6.0))
                .child(text("⚠").size(20.0).color(theme().colors.muted_foreground))
                .into_widget(),
        ),
    }
}

fn render_image_view(p: &Props) -> AnyWidget {
    // The network source as a signal, re-seeded when the source prop changes —
    // the load effect reads it and re-fetches (position-stable effects freeze
    // plain captured values, so the URL must travel through a signal).
    let net_url = create_signal(String::new());
    if let Source::Network(u) = &p.source
        && net_url.peek() != *u
    {
        net_url.set(u.clone());
    }
    match &p.source {
        Source::Ready(Ok(img)) => image_box(img, p),
        Source::Ready(Err(_)) => error_box(p),
        Source::Network(_) => match use_network(net_url).get() {
            ImageState::Loading => placeholder_box(p),
            ImageState::Loaded(img) => image_box(&img, p),
            ImageState::Failed(_) => error_box(p),
        },
    }
}

// ---------------------------------------------------------------------------
// FadeInImage — a placeholder that cross-fades to a network image on load
// ---------------------------------------------------------------------------

/// A network image that **fades in over a placeholder** once it decodes — Flutter's
/// `FadeInImage`. The placeholder (any widget: a low-res asset [`ImageView`], a
/// solid box, a shimmer) shows immediately; the loaded image cross-fades on top.
/// Build with [`fade_in_image`].
pub struct FadeInImage {
    url: String,
    placeholder: Option<AnyWidget>,
    fit: ImageFit,
    width: Option<f64>,
    height: Option<f64>,
    radius: Option<BorderRadius>,
    fade_secs: f64,
}

/// A [`FadeInImage`] that loads `url` from the network (native only — see
/// [`ImageView::network`]).
pub fn fade_in_image(url: impl Into<String>) -> FadeInImage {
    FadeInImage {
        url: url.into(),
        placeholder: None,
        fit: ImageFit::Cover,
        width: None,
        height: None,
        radius: None,
        fade_secs: 0.4,
    }
}

impl FadeInImage {
    /// The widget shown until (and behind) the loaded image.
    pub fn placeholder(mut self, widget: impl IntoWidget) -> Self {
        self.placeholder = Some(widget.into_widget());
        self
    }
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }
    pub fn size(self, width: f64, height: f64) -> Self {
        self.width(width).height(height)
    }
    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = Some(radius);
        self
    }
    /// The cross-fade duration in seconds (default `0.4`).
    pub fn fade(mut self, secs: f64) -> Self {
        self.fade_secs = secs.max(0.0);
        self
    }
}

struct FadeProps {
    url: String,
    placeholder: Option<AnyWidget>,
    fit: ImageFit,
    width: Option<f64>,
    height: Option<f64>,
    radius: Option<BorderRadius>,
    fade_secs: f64,
}

impl IntoWidget for FadeInImage {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_fade_in_image,
            FadeProps {
                url: self.url,
                placeholder: self.placeholder,
                fit: self.fit,
                width: self.width,
                height: self.height,
                radius: self.radius,
                fade_secs: self.fade_secs,
            },
        )
        .into_widget()
    }
}

fn fade_sized(mut c: Container, p: &FadeProps) -> Container {
    if let Some(w) = p.width {
        c = c.width(w);
    }
    if let Some(h) = p.height {
        c = c.height(h);
    }
    c
}

fn fade_image_box(img: &Image, p: &FadeProps) -> AnyWidget {
    let mut deco = BoxDecoration::new().image(img.clone()).image_fit(p.fit);
    if let Some(r) = p.radius {
        deco = deco.radius(r);
    }
    let mut c = Container::new().decoration(deco);
    if p.radius.is_some() {
        c = c.clip();
    }
    fade_sized(c, p).into_widget()
}

fn fade_placeholder(p: &FadeProps) -> AnyWidget {
    match &p.placeholder {
        Some(w) => w.clone(),
        None => {
            let mut deco = BoxDecoration::new().color(theme().colors.secondary);
            if let Some(r) = p.radius {
                deco = deco.radius(r);
            }
            fade_sized(Container::new().decoration(deco), p).into_widget()
        }
    }
}

fn render_fade_in_image(p: &FadeProps) -> AnyWidget {
    let net_url = create_signal(String::new());
    if net_url.peek() != p.url {
        net_url.set(p.url.clone());
    }
    let loaded = match use_network(net_url).get() {
        ImageState::Loaded(img) => Some(img),
        _ => None, // loading or failed → the placeholder holds
    };
    // Animate opacity 0 → 1 once the image is available (the cross-fade).
    let opacity = animated(if loaded.is_some() { 1.0 } else { 0.0 }, p.fade_secs) as f32;

    let mut layers: Vec<AnyWidget> = vec![fade_placeholder(p)];
    if let Some(img) = loaded {
        layers.push(Opacity::new(opacity, fade_image_box(&img, p)).into_widget());
    }
    stack(layers).into_widget()
}
