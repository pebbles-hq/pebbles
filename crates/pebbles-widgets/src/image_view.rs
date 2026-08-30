//! [`ImageView`] — display an image from an **asset** (file), a **URL** (fetched
//! and decoded on a background thread), **base64** (incl. `data:` URIs), or raw
//! **memory** bytes. Network loads show a `placeholder` while in flight and an
//! `error` widget on failure. Fit modes mirror CSS `object-fit` ([`ImageFit`]).
//!
//! Async model: a network load runs in the background via [`pebbles_core::spawn`];
//! the decoded result is delivered back on the UI thread (drained by the shared task
//! pump once per frame) into the result signal, so the view re-renders.

use std::io::Read;
use std::time::Duration;

use base64::Engine;
use pebbles_foundation::{Alignment, EdgeInsets};
use pebbles_render::{
    BorderRadius, BoxDecoration, Image, ImageFit, image_from_rgba8,
};

use crate::theme::theme;
use crate::widgets::{Container, spinner, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, component_props, create_effect, create_signal, spawn};

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
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| e.to_string())?;
    decode(&bytes)
}

/// Fetch a URL and decode it (runs on a background thread).
fn fetch(url: &str) -> Result<Image, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(32 * 1024 * 1024) // 32 MB cap
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    decode(&bytes)
}

/// A component hook: start (once) a network load for `url` and return its state. The
/// fetch runs in the background via [`pebbles_core::spawn`]; its result is delivered
/// back on the UI thread into `state`.
fn use_network(url: &str) -> Signal<ImageState> {
    let state = create_signal(ImageState::Loading);
    let url = url.to_string();
    create_effect(move || {
        let url = url.clone();
        spawn(
            move || match fetch(&url) {
                Ok(img) => ImageState::Loaded(img),
                Err(e) => ImageState::Failed(e),
            },
            move |result| state.set(result), // UI thread; a no-op if the view unmounted
        );
    });
    state
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
        let result = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| decode(&bytes));
        make(Source::Ready(result))
    }
    /// An image fetched from a URL (loads on a background thread).
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
    sized(
        Container::new().decoration(deco).alignment(Alignment::CENTER).child(child),
        p,
    )
    .into_widget()
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
    match &p.source {
        Source::Ready(Ok(img)) => image_box(img, p),
        Source::Ready(Err(_)) => error_box(p),
        Source::Network(url) => match use_network(url).get() {
            ImageState::Loading => placeholder_box(p),
            ImageState::Loaded(img) => image_box(&img, p),
            ImageState::Failed(_) => error_box(p),
        },
    }
}
