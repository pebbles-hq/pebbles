//! The shader table (06 §9): every pipeline's full WGSL source — `prelude.wgsl` concatenated in
//! front — built at compile time, so the source naga validates is exactly the one wgpu compiles.

macro_rules! shader {
    ($file:literal) => {
        concat!(include_str!("prelude.wgsl"), "\n", include_str!($file))
    };
    // A brush paint (09 §6): prelude + the paint + the shared coverage variants.
    ($file:literal, brush) => {
        concat!(
            include_str!("prelude.wgsl"),
            "\n",
            include_str!($file),
            "\n",
            include_str!("brush_cover.wgsl")
        )
    };
}

pub(crate) const SOLID: &str = shader!("solid.wgsl");
pub(crate) const GLYPH: &str = shader!("glyph.wgsl");
/// The LCD glyph module (`glyph_lcd.wgsl`): needs `Features::DUAL_SOURCE_BLENDING`.
pub(crate) const GLYPH_LCD: &str = concat!(
    "enable dual_source_blending;\n",
    include_str!("prelude.wgsl"),
    "\n",
    include_str!("glyph.wgsl"),
    "\n",
    include_str!("glyph_lcd.wgsl")
);
pub(crate) const IMAGE: &str = shader!("image.wgsl");
pub(crate) const SHADOW: &str = shader!("shadow.wgsl");
pub(crate) const SDF: &str = shader!("sdf.wgsl");
pub(crate) const COMPOSITE: &str = shader!("composite.wgsl");
pub(crate) const BLIT: &str = shader!("blit.wgsl");
pub(crate) const EFFECTS: &str = shader!("effects.wgsl");
pub(crate) const PROJECTED: &str = shader!("projected.wgsl");
pub(crate) const VERTICES: &str = shader!("vertices.wgsl");
pub(crate) const GRADIENT: &str = shader!("gradient.wgsl", brush);
pub(crate) const IMAGE_BRUSH: &str = shader!("image_brush.wgsl", brush);

/// Everything before the app source of a shader brush (09 §6.1, L43).
pub(crate) const SHADER_BRUSH_HEAD: &str =
    concat!(include_str!("prelude.wgsl"), "\n", include_str!("shader_brush_prelude.wgsl"), "\n");

/// The full module of a shader brush: head + the app's `pebbles_paint` + the coverage variants.
pub(crate) fn shader_brush_source(app: &str) -> String {
    format!("{SHADER_BRUSH_HEAD}{app}\n{}", include_str!("brush_cover.wgsl"))
}

/// Lines before the app source in [`shader_brush_source`] (app line = module line − this).
pub(crate) fn shader_brush_head_lines() -> usize {
    SHADER_BRUSH_HEAD.lines().count()
}

/// `(name, full source)` for every pipeline shader.
#[cfg_attr(not(test), allow(dead_code))] // read by the V02 validation tests
pub(crate) const ALL: &[(&str, &str)] = &[
    ("solid", SOLID),
    ("glyph", GLYPH),
    ("glyph_lcd", GLYPH_LCD),
    ("image", IMAGE),
    ("shadow", SHADOW),
    ("sdf", SDF),
    ("composite", COMPOSITE),
    ("blit", BLIT),
    ("effects", EFFECTS),
    ("projected", PROJECTED),
    ("vertices", VERTICES),
    ("gradient", GRADIENT),
    ("image_brush", IMAGE_BRUSH),
];

#[cfg(test)]
mod tests;
