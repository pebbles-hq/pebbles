//! The group-alpha stack (C17 interim layers): opacity folds into every draw inside,
//! zero-alpha and blend-mode layers skip their content (09 §1 "interim").

/// One pushed layer's interim state.
#[derive(Clone, Copy, Debug)]
struct Entry {
    alpha: f32,
    skip: bool,
}

/// The stack of open layers; `group_alpha()` is the product of their alphas.
#[derive(Default)]
pub(crate) struct LayerStack {
    entries: Vec<Entry>,
    skipped: u32,
    alpha: f32,
}

impl LayerStack {
    pub(crate) fn reset(&mut self) {
        self.entries.clear();
        self.skipped = 0;
        self.alpha = 1.0;
    }

    /// Push a layer; `skip` hides its whole subtree.
    pub(crate) fn push(&mut self, alpha: f32, skip: bool) {
        self.entries.push(Entry { alpha, skip });
        self.skipped += u32::from(skip);
        self.recompute();
    }

    /// Pop the innermost layer. Returns `false` (and does nothing) when the stack is empty.
    pub(crate) fn pop(&mut self) -> bool {
        let Some(e) = self.entries.pop() else { return false };
        self.skipped -= u32::from(e.skip);
        self.recompute();
        true
    }

    /// Whether the current position is inside a skipped layer.
    pub(crate) fn is_skipped(&self) -> bool {
        self.skipped > 0
    }

    /// Product of every open layer's alpha.
    pub(crate) fn group_alpha(&self) -> f32 {
        self.alpha
    }

    fn recompute(&mut self) {
        self.alpha = self.entries.iter().map(|e| e.alpha).product();
    }
}
