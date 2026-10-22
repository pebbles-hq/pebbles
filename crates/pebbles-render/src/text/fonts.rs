//! The cosmic-text font registry (03 §3.3): bundled faces first and renamed to their builtin
//! family, zero-copy (`&'static` bytes shared by fontdb and `peniko::Blob`), generics mapped
//! to bundled families, and no system scan unless asked for (T08).

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::fontdb;

use crate::fonts::BUILTIN_FAMILIES;

/// Font bytes shared zero-copy between fontdb (shaping) and peniko::Blob (glyph ops).
pub(crate) type FontBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// What a glyph op needs for one (face, weight): the face bytes + variation coords.
#[derive(Clone, Debug)]
pub(crate) struct FaceRef {
    pub(crate) font: peniko::FontData,
    pub(crate) coords: Arc<[i16]>,
}

/// Generic CSS families -> bundled family (§3.3.4): deterministic on every platform.
const GENERICS: &[(&str, &str)] = &[
    ("serif", "Lora"),
    ("ui-serif", "Lora"),
    ("sans-serif", "Inter"),
    ("system-ui", "Inter"),
    ("ui-sans-serif", "Inter"),
    ("-apple-system", "Inter"),
    ("blinkmacsystemfont", "Inter"),
    ("monospace", "JetBrains Mono"),
    ("ui-monospace", "JetBrains Mono"),
    ("cursive", "Inter"),
    ("fantasy", "Inter"),
    ("emoji", "Inter"),
    ("math", "Inter"),
    ("fangsong", "Inter"),
    ("ui-rounded", "Inter"),
];

/// Split a CSS family list on commas outside quotes; trim, strip one level of matching
/// quotes, skip empty entries.
fn split_css_families(css: &str) -> smallvec::SmallVec<[&str; 4]> {
    let mut out: smallvec::SmallVec<[&str; 4]> = smallvec::SmallVec::new();
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (i, c) in css.char_indices() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, ',') => {
                out.push(&css[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&css[start..]);
    out.into_iter()
        .map(str::trim)
        .map(|e| {
            let unquoted =
                ['\'', '"'].iter().find_map(|q| e.strip_prefix(*q).and_then(|r| r.strip_suffix(*q)));
            unquoted.unwrap_or(e).trim()
        })
        .filter(|e| !e.is_empty())
        .collect()
}

/// Fixed aliases (§3.3.3): lowercase key -> canonical family.
const ALIASES: &[(&str, &str)] = &[("inter variable", "Inter"), ("space grotesk light", "Space Grotesk")];

pub(crate) struct FontRegistry {
    /// Built with `new_with_locale_and_db` — never scans.
    system: cosmic_text::FontSystem,
    /// +1 on every face addition.
    generation: u64,
    /// Builtins (declaration order) then others (case-insensitive sort).
    families: Vec<String>,
    /// Lowercase name/alias -> canonical family name.
    canonical: HashMap<String, String>,
    /// CSS family string -> resolved family (cleared on generation bump).
    resolved: HashMap<String, cosmic_text::FamilyOwned>,
    /// Face data, created at registration (binary) or lazily (system).
    faces: HashMap<fontdb::ID, peniko::FontData>,
    /// Normalized wght coords per (face, weight).
    coords: HashMap<(fontdb::ID, u16), Arc<[i16]>>,
    /// Lowercase builtin names + alias keys + user family names (system faces with these
    /// names are dropped on merge).
    protected: Vec<String>,
    /// Log-once set for unresolved family strings.
    warned_families: std::collections::HashSet<String>,
    /// Log-once flag for runtime font bytes that failed to parse.
    warned_bad_font: bool,
    /// Log-once flag for a face whose data could not be shared (glyph runs skip it).
    warned_unshared: bool,
    /// The background system-font scan ("pebbles-font-scan"), merged on the next `poll`.
    #[cfg(not(target_family = "wasm"))]
    loader: Option<std::thread::JoinHandle<fontdb::Database>>,
}

/// Load `bytes` into `db`, renaming every face to `rename` when given; returns the new ids
/// with their face data. Unparseable bytes add nothing (logged once by the caller).
fn load(
    db: &mut fontdb::Database,
    faces: &mut HashMap<fontdb::ID, peniko::FontData>,
    bytes: &'static [u8],
    rename: Option<&str>,
) -> usize {
    let data: FontBytes = Arc::new(bytes);
    let ids = db.load_font_source(fontdb::Source::Binary(data.clone()));
    let mut added = 0;
    for id in ids {
        let Some(mut info) = db.face(id).cloned() else { continue };
        let id = match rename {
            Some(family) => {
                db.remove_face(id);
                info.families = vec![(family.to_string(), fontdb::Language::English_UnitedStates)];
                db.push_face_info(info.clone())
            }
            None => id,
        };
        faces.insert(id, peniko::FontData::new(peniko::Blob::new(data.clone()), info.index));
        added += 1;
    }
    added
}

impl FontRegistry {
    /// Bundled faces (renamed to their builtin family) + `App::font` faces, generics mapped
    /// to bundled families. No I/O.
    pub(crate) fn new_bundled(user_fonts: &[&'static [u8]], locale: String) -> FontRegistry {
        Self::build(true, user_fonts, locale)
    }

    /// A registry with no faces at all (fuzzing: only the bytes under test, S31).
    pub(crate) fn empty() -> FontRegistry {
        Self::build(false, &[], "en-US".into())
    }

    fn build(builtins: bool, user_fonts: &[&'static [u8]], locale: String) -> FontRegistry {
        let mut db = fontdb::Database::new();
        let mut faces = HashMap::new();
        for (family, bytes) in crate::fonts::builtin_fonts().iter().filter(|_| builtins) {
            if load(&mut db, &mut faces, bytes, Some(family)) == 0 {
                pebbles_foundation::log::warn(
                    pebbles_foundation::log::Cat::General,
                    format!("bundled font for {family} failed to parse"),
                );
            }
        }
        let mut user_families = Vec::new();
        for bytes in user_fonts {
            let before: Vec<fontdb::ID> = db.faces().map(|f| f.id).collect();
            if load(&mut db, &mut faces, bytes, None) == 0 {
                pebbles_foundation::log::warn(
                    pebbles_foundation::log::Cat::General,
                    "an App::font face failed to parse".to_string(),
                );
            }
            for face in db.faces().filter(|f| !before.contains(&f.id)) {
                user_families.extend(face.families.iter().map(|(n, _)| n.to_lowercase()));
            }
        }
        db.set_sans_serif_family("Inter");
        db.set_serif_family("Lora");
        db.set_monospace_family("JetBrains Mono");
        db.set_cursive_family("Inter");
        db.set_fantasy_family("Inter");
        let system = cosmic_text::FontSystem::new_with_locale_and_db(locale, db);
        let mut protected: Vec<String> = BUILTIN_FAMILIES.iter().map(|f| f.to_lowercase()).collect();
        protected.extend(ALIASES.iter().map(|(k, _)| k.to_string()));
        protected.extend(user_families);
        protected.sort();
        protected.dedup();
        let mut registry = FontRegistry {
            system,
            generation: 1,
            families: Vec::new(),
            canonical: HashMap::new(),
            resolved: HashMap::new(),
            faces,
            coords: HashMap::new(),
            protected,
            warned_families: Default::default(),
            warned_bad_font: false,
            warned_unshared: false,
            #[cfg(not(target_family = "wasm"))]
            loader: None,
        };
        registry.rebuild_names();
        registry
    }

    /// Rebuild `families` and `canonical` from the database (§3.3.3).
    fn rebuild_names(&mut self) {
        let db = self.system.db();
        let mut names: Vec<String> = Vec::new();
        for face in db.faces() {
            for (name, _) in &face.families {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        let mut families: Vec<String> = BUILTIN_FAMILIES
            .iter()
            .filter(|b| names.iter().any(|n| n == *b))
            .map(|b| b.to_string())
            .collect();
        let mut others: Vec<String> =
            names.into_iter().filter(|n| !BUILTIN_FAMILIES.contains(&n.as_str())).collect();
        others.sort_by_key(|n| n.to_lowercase());
        families.extend(others);
        let mut canonical = HashMap::new();
        for (key, family) in ALIASES {
            canonical.insert(key.to_string(), family.to_string());
        }
        for family in &families {
            canonical.entry(family.to_lowercase()).or_insert_with(|| family.clone());
        }
        self.families = families;
        self.canonical = canonical;
    }

    /// Scan the system fonts now and merge them (capture/offscreen: screenshots match the app).
    /// A no-op on wasm (fontdb has no wasm arm).
    pub(crate) fn scan_blocking(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        {
            let mut tmp = fontdb::Database::new();
            tmp.load_system_fonts();
            self.merge(tmp);
        }
    }

    /// Start the system-font scan on a background thread; [`poll`](Self::poll) merges it.
    /// A no-op on wasm (no threads); a spawn failure logs once and keeps bundled fonts only.
    pub(crate) fn start_background(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        {
            let spawned = std::thread::Builder::new().name("pebbles-font-scan".into()).spawn(|| {
                let mut tmp = fontdb::Database::new();
                tmp.load_system_fonts();
                tmp
            });
            match spawned {
                Ok(handle) => self.loader = Some(handle),
                Err(e) => pebbles_foundation::log::warn(
                    pebbles_foundation::log::Cat::General,
                    format!("system font scan thread failed to start ({e}); using bundled fonts only"),
                ),
            }
        }
    }

    /// Merge a scanned database: faces whose family collides with a protected name (builtins,
    /// aliases, user fonts) are dropped. Returns the number of faces added.
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // scans are native-only
    fn merge(&mut self, tmp: fontdb::Database) -> usize {
        let mut added = 0;
        for face in tmp.faces() {
            if face.families.iter().any(|(n, _)| self.protected.contains(&n.to_lowercase())) {
                continue;
            }
            // db_mut() also clears cosmic-text's font-match cache.
            self.system.db_mut().push_face_info(face.clone());
            added += 1;
        }
        if added > 0 {
            self.generation += 1;
            self.rebuild_names();
            self.resolved.clear();
            crate::fonts::refresh_families_from(&self.families);
        }
        added
    }

    /// Test seam for [`merge`](Self::merge).
    #[cfg(test)]
    pub(crate) fn merge_for_test(&mut self, tmp: fontdb::Database) -> usize {
        self.merge(tmp)
    }

    /// Merge a finished background scan; `true` iff the generation changed. Never blocks.
    pub(crate) fn poll(&mut self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        if self.loader.as_ref().is_some_and(std::thread::JoinHandle::is_finished) {
            let before = self.generation;
            match self.loader.take().map(std::thread::JoinHandle::join) {
                Some(Ok(tmp)) => {
                    self.merge(tmp);
                }
                Some(Err(_)) => pebbles_foundation::log::warn(
                    pebbles_foundation::log::Cat::General,
                    "system font scan panicked; using bundled fonts only".to_string(),
                ),
                None => {}
            }
            return self.generation != before;
        }
        false
    }

    /// Normalized coords for `weight` plus every axis in `variations` (`wght` there overrides
    /// the weight); empty when all axes sit at their defaults.
    pub(crate) fn coords_for(
        &mut self,
        font: &peniko::FontData,
        weight: u16,
        variations: &[crate::objects::FontVariation],
    ) -> Option<Arc<[i16]>> {
        use cosmic_text::skrifa::{FontRef, MetadataProvider, Tag};
        let font_ref = FontRef::from_index(font.data.data(), font.index).ok()?;
        let mut settings: Vec<(Tag, f32)> = vec![(Tag::new(b"wght"), f32::from(weight))];
        for v in variations {
            let tag = Tag::new(&v.axis);
            settings.retain(|(t, _)| *t != tag);
            settings.push((tag, v.value));
        }
        let loc = font_ref.axes().location(settings);
        let bits: Vec<i16> = loc.coords().iter().map(|c| c.to_bits()).collect();
        Some(if bits.iter().all(|&b| b == 0) { Arc::from([]) } else { Arc::from(bits) })
    }

    /// [`face`](Self::face) with extra variation axes (not memoized: variation sets are rare).
    pub(crate) fn face_with(
        &mut self,
        id: fontdb::ID,
        weight: u16,
        variations: &[crate::objects::FontVariation],
    ) -> Option<FaceRef> {
        let base = self.face(id, weight)?;
        if variations.is_empty() {
            return Some(base);
        }
        let coords = self.coords_for(&base.font, weight, variations)?;
        Some(FaceRef { font: base.font, coords })
    }

    /// Face data for a glyph op (§3.3.6): the face's one registry-owned Blob and the normalized
    /// `wght` coordinates for `weight` (empty at the default instance). System faces are
    /// memory-mapped by cosmic-text on first use; a face that still cannot be shared is `None`
    /// (the caller skips the run).
    pub(crate) fn face(&mut self, id: fontdb::ID, weight: u16) -> Option<FaceRef> {
        let font = match self.faces.get(&id) {
            Some(font) => font.clone(),
            None => {
                let font = self.shared_face_data(id)?;
                self.faces.insert(id, font.clone());
                font
            }
        };
        let coords = match self.coords.get(&(id, weight)) {
            Some(c) => c.clone(),
            None => {
                use cosmic_text::skrifa::{FontRef, MetadataProvider, Tag};
                let font_ref = FontRef::from_index(font.data.data(), font.index).ok()?;
                let loc = font_ref.axes().location([(Tag::new(b"wght"), f32::from(weight))]);
                let bits: Vec<i16> = loc.coords().iter().map(|c| c.to_bits()).collect();
                let coords: Arc<[i16]> =
                    if bits.iter().all(|&b| b == 0) { Arc::from([]) } else { Arc::from(bits) };
                self.coords.insert((id, weight), coords.clone());
                coords
            }
        };
        Some(FaceRef { font, coords })
    }

    /// A `FontData` over the face's shared bytes (binary sources directly; a file source after
    /// cosmic-text memory-maps it).
    fn shared_face_data(&mut self, id: fontdb::ID) -> Option<peniko::FontData> {
        for attempt in 0..2 {
            let (source, index) = self.system.db().face_source(id)?;
            match source {
                fontdb::Source::Binary(arc) | fontdb::Source::SharedFile(_, arc) => {
                    return Some(peniko::FontData::new(peniko::Blob::new(arc), index));
                }
                fontdb::Source::File(_) if attempt == 0 => {
                    // cosmic maps the file and converts the source to SharedFile.
                    let _ = self.system.get_font(id, fontdb::Weight(400));
                }
                fontdb::Source::File(path) => {
                    if !self.warned_unshared {
                        self.warned_unshared = true;
                        pebbles_foundation::log::warn(
                            pebbles_foundation::log::Cat::General,
                            format!(
                                "font face {} could not be memory-mapped; its text is skipped",
                                path.display()
                            ),
                        );
                    }
                }
            }
        }
        None
    }

    /// Ascent and descent (px, both positive) of the regular face of CSS family `css` at `size`.
    pub(crate) fn vertical_metrics(&mut self, css: &str, size: f32) -> Option<(f32, f32)> {
        use cosmic_text::skrifa::{
            FontRef, MetadataProvider,
            instance::{LocationRef, Size},
        };
        let cosmic_text::FamilyOwned::Name(name) = self.resolve_family(css) else { return None };
        let id = self.system.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name(&name)],
            weight: fontdb::Weight(400),
            stretch: fontdb::Stretch::Normal,
            style: fontdb::Style::Normal,
        })?;
        let face = self.face(id, 400)?;
        let font = FontRef::from_index(face.font.data.data(), face.font.index).ok()?;
        let m = font.metrics(Size::new(size), LocationRef::default());
        Some((m.ascent, -m.descent))
    }

    /// Register runtime font bytes (one or more faces) under their own family names; returns
    /// the number of faces added. Unparseable bytes add nothing (logged once).
    pub(crate) fn register(&mut self, data: FontBytes) -> usize {
        let ids = self.system.db_mut().load_font_source(fontdb::Source::Binary(data.clone()));
        for id in &ids {
            let Some(face) = self.system.db().face(*id) else { continue };
            self.protected.extend(face.families.iter().map(|(n, _)| n.to_lowercase()));
            self.faces.insert(*id, peniko::FontData::new(peniko::Blob::new(data.clone()), face.index));
        }
        if ids.is_empty() {
            if !self.warned_bad_font {
                self.warned_bad_font = true;
                pebbles_foundation::log::warn(
                    pebbles_foundation::log::Cat::General,
                    "registered font bytes did not parse as a font".to_string(),
                );
            }
            return 0;
        }
        self.protected.sort();
        self.protected.dedup();
        self.generation += 1;
        self.rebuild_names();
        self.resolved.clear();
        crate::fonts::refresh_families_from(&self.families);
        ids.len()
    }

    /// Resolve a CSS family list to one concrete bundled/registered family (§3.3.4): generics
    /// first, then the canonical table; no hit -> [`DEFAULT_FAMILY`](crate::fonts::DEFAULT_FAMILY),
    /// logged once per distinct string. Memoized until the next generation bump.
    pub(crate) fn resolve_family(&mut self, css: &str) -> cosmic_text::FamilyOwned {
        if let Some(hit) = self.resolved.get(css) {
            return hit.clone();
        }
        let found = split_css_families(css).into_iter().find_map(|entry| {
            let key = entry.to_lowercase();
            GENERICS
                .iter()
                .find(|(g, _)| *g == key)
                .map(|(_, family)| family.to_string())
                .or_else(|| self.canonical.get(&key).cloned())
        });
        let family = found.unwrap_or_else(|| {
            if self.warned_families.insert(css.to_string()) {
                pebbles_foundation::log::warn(
                    pebbles_foundation::log::Cat::General,
                    format!("font family `{css}` not found; using {}", crate::fonts::DEFAULT_FAMILY),
                );
            }
            crate::fonts::DEFAULT_FAMILY.to_string()
        });
        let resolved = cosmic_text::FamilyOwned::Name(family.into());
        self.resolved.insert(css.to_string(), resolved.clone());
        resolved
    }

    pub(crate) fn font_system(&mut self) -> &mut cosmic_text::FontSystem {
        &mut self.system
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn families(&self) -> &[String] {
        &self.families
    }

    /// Census: faces in the database.
    #[cfg_attr(not(debug_assertions), allow(dead_code))] // read by the debug census
    pub(crate) fn faces_len(&self) -> usize {
        self.system.db().len()
    }

    /// Census: faces with glyph-op data.
    #[cfg_attr(not(debug_assertions), allow(dead_code))] // read by the debug census
    pub(crate) fn face_data_len(&self) -> usize {
        self.faces.len()
    }

    /// Census: memoized (face, weight) coords.
    #[cfg_attr(not(debug_assertions), allow(dead_code))] // read by the debug census
    pub(crate) fn coords_len(&self) -> usize {
        self.coords.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontdb::{Family, Query, Stretch, Style, Weight};

    fn registry() -> FontRegistry {
        FontRegistry::new_bundled(&[], "en-US".into())
    }

    fn query(r: &mut FontRegistry, name: &str, style: Style) -> Option<fontdb::ID> {
        r.font_system().db().query(&Query {
            families: &[Family::Name(name)],
            weight: Weight(400),
            stretch: Stretch::Normal,
            style,
        })
    }

    #[test]
    fn bundled_faces_resolve_by_builtin_name() {
        let mut r = registry();
        for name in BUILTIN_FAMILIES {
            let id = query(&mut r, name, Style::Normal).unwrap_or_else(|| panic!("{name} resolves"));
            assert_eq!(r.font_system().db().face(id).expect("face").families[0].0, *name);
        }
    }

    #[test]
    fn italic_faces_exist() {
        let mut r = registry();
        for name in ["Inter", "JetBrains Mono"] {
            let id = query(&mut r, name, Style::Italic).expect("italic query");
            assert_eq!(r.font_system().db().face(id).expect("face").style, Style::Italic, "{name}");
        }
    }

    #[test]
    fn no_system_scan() {
        assert_eq!(registry().font_system().db().len(), 6);
    }

    #[test]
    fn bundled_face_data_is_zero_copy() {
        let r = registry();
        let builtin: Vec<*const u8> = crate::fonts::builtin_fonts().iter().map(|(_, b)| b.as_ptr()).collect();
        assert_eq!(r.faces.len(), builtin.len());
        for font in r.faces.values() {
            assert!(builtin.contains(&font.data.data().as_ptr()), "face data was copied");
        }
    }

    #[test]
    fn families_builtins_first() {
        assert_eq!(registry().families(), ["Inter", "JetBrains Mono", "Space Grotesk", "Lora"]);
    }

    #[test]
    fn resolve_family_table() {
        let mut r = registry();
        for (css, want) in [
            ("jetbrains mono", "JetBrains Mono"),
            ("monospace", "JetBrains Mono"),
            ("ui-monospace", "JetBrains Mono"),
            ("serif", "Lora"),
            ("sans-serif", "Inter"),
            ("system-ui", "Inter"),
            ("'Not A Font', Lora", "Lora"),
            ("\"Space Grotesk\"", "Space Grotesk"),
            ("Inter Variable", "Inter"),
            ("Space Grotesk Light", "Space Grotesk"),
            ("Nope", "Inter"),
            ("", "Inter"),
            (" , ,", "Inter"),
        ] {
            assert_eq!(r.resolve_family(css), cosmic_text::FamilyOwned::Name(want.into()), "{css:?}");
        }
    }

    #[test]
    fn split_is_quote_aware() {
        assert_eq!(split_css_families("'a, b', \"c\" ,d,,").as_slice(), ["a, b", "c", "d"]);
    }

    #[test]
    fn resolve_is_memoized() {
        let mut r = registry();
        r.resolve_family("serif");
        r.resolve_family("serif");
        assert_eq!(r.resolved.len(), 1);
    }

    #[test]
    fn register_user_font_adds_faces_and_bumps_generation() {
        let mut r = registry();
        let before = r.generation();
        let lora = crate::fonts::builtin_fonts()[5].1.to_vec();
        assert_eq!(r.register(Arc::new(lora)), 1);
        assert_eq!(r.generation(), before + 1);
    }

    #[test]
    fn register_garbage_returns_zero() {
        let mut r = registry();
        let before = r.generation();
        assert_eq!(r.register(Arc::new(vec![0u8; 64])), 0);
        assert_eq!(r.generation(), before);
    }

    fn renamed(bytes: &'static [u8], family: &str) -> fontdb::Database {
        let mut db = fontdb::Database::new();
        for id in db.load_font_source(fontdb::Source::Binary(Arc::new(bytes))) {
            let mut info = db.face(id).cloned().expect("face");
            db.remove_face(id);
            info.families = vec![(family.to_string(), fontdb::Language::English_UnitedStates)];
            db.push_face_info(info);
        }
        db
    }

    /// Inter Variable (protected alias, unmodified), a face renamed "Inter" (protected builtin)
    /// and Lora renamed "System Lora Copy".
    fn system_db() -> fontdb::Database {
        let inter = crate::fonts::builtin_fonts()[0].1;
        let mut db = fontdb::Database::new();
        db.load_font_source(fontdb::Source::Binary(Arc::new(inter)));
        for face in renamed(inter, "Inter")
            .faces()
            .chain(renamed(crate::fonts::builtin_fonts()[5].1, "System Lora Copy").faces())
        {
            db.push_face_info(face.clone());
        }
        db
    }

    #[test]
    fn merge_skips_builtin_family_collisions() {
        let mut r = registry();
        let inter = query(&mut r, "Inter", Style::Normal);
        assert_eq!(r.merge_for_test(system_db()), 1);
        assert_eq!(query(&mut r, "Inter", Style::Normal), inter, "the bundled Inter still wins");
    }

    #[test]
    fn merge_bumps_generation_and_families() {
        let mut r = registry();
        let before = r.generation();
        r.merge_for_test(system_db());
        assert_eq!(r.generation(), before + 1);
        assert_eq!(r.families(), ["Inter", "JetBrains Mono", "Space Grotesk", "Lora", "System Lora Copy"]);
    }

    #[test]
    fn poll_without_loader_is_false() {
        assert!(!registry().poll());
    }

    #[test]
    #[ignore = "scans the host's system fonts"]
    #[cfg(not(target_family = "wasm"))]
    fn background_scan_completes() {
        let mut r = registry();
        let before = r.faces_len();
        r.start_background();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if r.poll() {
                assert!(r.faces_len() > before);
                return;
            }
            if r.loader.is_none() {
                break; // finished without adding a face (zero system fonts)
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(r.faces_len(), before, "the scan neither merged nor finished in 30 s");
        assert!(r.loader.is_none(), "the scan did not finish in 30 s");
    }

    fn id_of(r: &mut FontRegistry, family: &str) -> fontdb::ID {
        query(r, family, Style::Normal).expect("face")
    }

    #[test]
    fn lora_coords() {
        let mut r = registry();
        let id = id_of(&mut r, "Lora");
        assert!(r.face(id, 400).expect("face").coords.is_empty());
        assert_eq!(&*r.face(id, 550).expect("face").coords, &[8192]);
        assert_eq!(&*r.face(id, 700).expect("face").coords, &[16384]);
    }

    #[test]
    fn face_blob_is_stable() {
        let mut r = registry();
        let id = id_of(&mut r, "Lora");
        let a = r.face(id, 400).expect("face").font.data.id();
        assert_eq!(r.face(id, 400).expect("face").font.data.id(), a);
        assert_eq!(r.face(id, 700).expect("face").font.data.id(), a);
    }

    #[test]
    fn inter_bold_has_coords() {
        let mut r = registry();
        let id = id_of(&mut r, "Inter");
        let c = r.face(id, 700).expect("face").coords;
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0], 0, "opsz stays at its default");
    }

    #[test]
    #[ignore = "scans the host's system fonts"]
    #[cfg(not(target_family = "wasm"))]
    fn system_file_face_is_shared() {
        let mut r = registry();
        r.scan_blocking();
        let file = r.font_system().db().faces().find_map(|f| match &f.source {
            fontdb::Source::File(path) => Some((f.id, path.clone())),
            _ => None,
        });
        let Some((id, path)) = file else { return };
        let face = r.face(id, 400).expect("system face is shared");
        assert_eq!(face.font.data.len() as u64, std::fs::metadata(path).expect("stat").len());
    }

    #[test]
    fn aliases_and_case_map_to_canonical() {
        let r = registry();
        assert_eq!(r.canonical.get("inter variable").map(String::as_str), Some("Inter"));
        assert_eq!(r.canonical.get("jetbrains mono").map(String::as_str), Some("JetBrains Mono"));
        assert!(r.protected.contains(&"space grotesk light".to_string()));
        assert_eq!(r.generation(), 1);
    }
}
