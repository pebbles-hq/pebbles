//! Generation-keyed text caches (03 §3.8): content-hashed shape and layout maps, swept only
//! under pressure by the existing rule and cleared on every font-set change.

use std::collections::HashMap;
use std::rc::Rc;

use super::layout::TextLayout;
use super::shape::ShapedText;
use super::{SHAPE_CACHE_PRESSURE, SHAPE_CACHE_STALE};

/// A `u64`-keyed map whose entries remember the generation they were last used in.
pub(crate) struct GenMap<V> {
    map: HashMap<u64, (V, u32)>,
}

impl<V> Default for GenMap<V> {
    fn default() -> Self {
        GenMap { map: HashMap::new() }
    }
}

impl<V: Clone> GenMap<V> {
    /// The entry for `key`, marked used in `generation`.
    #[allow(dead_code)] // RenderParagraph looks layouts up from T22.
    pub(crate) fn get(&mut self, key: u64, generation: u32) -> Option<V> {
        self.map.get_mut(&key).map(|(v, used)| {
            *used = generation;
            v.clone()
        })
    }

    #[allow(dead_code)]
    pub(crate) fn insert(&mut self, key: u64, value: V, generation: u32) {
        self.map.insert(key, (value, generation));
    }

    /// Under pressure, on every 256th generation, drop entries unused for more than
    /// `SHAPE_CACHE_STALE` generations. Never every frame.
    pub(crate) fn sweep(&mut self, generation: u32) {
        if self.map.len() > SHAPE_CACHE_PRESSURE && generation.is_multiple_of(256) {
            self.map.retain(|_, (_, used)| generation.wrapping_sub(*used) <= SHAPE_CACHE_STALE);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }
}

/// The shape and layout caches of one `TextEnv`.
#[derive(Default)]
pub(crate) struct TextCaches {
    #[allow(dead_code)] // filled by RenderParagraph from T22.
    pub(crate) shapes: GenMap<Rc<ShapedText>>,
    #[allow(dead_code)]
    pub(crate) layouts: GenMap<Rc<TextLayout>>,
    pub(crate) generation: u32,
}

impl TextCaches {
    /// End of frame: advance the generation and sweep both maps independently.
    pub(crate) fn finish_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.shapes.sweep(self.generation);
        self.layouts.sweep(self.generation);
    }

    /// The font set changed: every shape may differ.
    pub(crate) fn clear(&mut self) {
        self.shapes.clear();
        self.layouts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::ParagraphStyle;
    use crate::text::TextEnv;

    #[test]
    fn sweep_only_under_pressure() {
        let mut m: GenMap<u32> = GenMap::default();
        for k in 0..40_000 {
            m.insert(k, 0, 0);
        }
        m.sweep(256);
        assert_eq!(m.len(), 40_000, "not stale yet");
        m.sweep(4096 + 256);
        assert_eq!(m.len(), 0, "stale under pressure");
    }

    #[test]
    fn font_generation_clears() {
        let mut env = TextEnv::new();
        let shaped = crate::text::layout_for_test(&mut env, "abc", &ParagraphStyle::default(), &[], None);
        env.caches.layouts.insert(1, shaped.clone(), 0);
        env.caches.shapes.insert(1, shaped.shaped.clone(), 0);
        assert_eq!(env.register_font(crate::fonts::builtin_fonts()[5].1.to_vec()), 1);
        assert_eq!((env.caches.shapes.len(), env.layout_cache_len()), (0, 0));
    }
}
