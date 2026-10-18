//! The shader table (06 §9): every pipeline's full WGSL source — `prelude.wgsl` concatenated in
//! front — built at compile time, so the source naga validates is exactly the one wgpu compiles.

macro_rules! shader {
    ($file:literal) => {
        concat!(include_str!("prelude.wgsl"), "\n", include_str!($file))
    };
}

pub(crate) const SOLID: &str = shader!("solid.wgsl");
pub(crate) const GLYPH: &str = shader!("glyph.wgsl");
pub(crate) const IMAGE: &str = shader!("image.wgsl");
pub(crate) const SHADOW: &str = shader!("shadow.wgsl");
pub(crate) const SDF: &str = shader!("sdf.wgsl");
pub(crate) const COMPOSITE: &str = shader!("composite.wgsl");

/// `(name, full source)` for every pipeline shader.
#[cfg_attr(not(test), allow(dead_code))] // read by the V02 validation tests
pub(crate) const ALL: &[(&str, &str)] = &[
    ("solid", SOLID),
    ("glyph", GLYPH),
    ("image", IMAGE),
    ("shadow", SHADOW),
    ("sdf", SDF),
    ("composite", COMPOSITE),
];

#[cfg(test)]
mod tests;
