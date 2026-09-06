//! Formatting + color helpers.

use pebbles::prelude::*;

use crate::model;
use crate::store;

/// A money label in the current currency, e.g. `$1,234.56`.
pub fn price(cents: i64) -> String {
    model::money_sym(cents, &store::symbol())
}

/// Blend `t` of `over` onto `base` (a light tint for icon chips / washes).
pub fn mix(base: Color, over: Color, t: f64) -> Color {
    let t = t as f32;
    let [br, bg, bb, _] = base.components;
    let [or, og, ob, _] = over.components;
    Color::new([br + (or - br) * t, bg + (og - bg) * t, bb + (ob - bb) * t, 1.0])
}
