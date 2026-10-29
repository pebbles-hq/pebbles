//! Explorer theme tokens: [`ExplorerStyle`] — every colour and size the tree
//! uses, derived from the Pebbles [`Theme`](crate::Theme) by default and
//! overridable by the app (live: it re-derives on every theme / light-dark
//! change).

#[allow(clippy::wildcard_imports)]
use super::*;

/// Every colour and size the explorer paints with. Build one from a theme
/// with [`from_theme`](Self::from_theme), tweak fields, and install it with
/// [`FileExplorer::set_style`] (a function of the theme, so it follows
/// light/dark switches).
///
/// ```ignore
/// explorer.set_style(|th| ExplorerStyle {
///     indent: 10.0,
///     selection_active_bg: th.colors.primary,
///     ..ExplorerStyle::from_theme(th)
/// });
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ExplorerStyle {
    /// Row background (unselected, not hovered).
    pub row_bg: Color,
    /// Hover overlay mixed onto the row.
    pub hover_bg: Color,
    /// How strongly the hover overlay shows (0–1).
    pub hover_strength: f32,
    /// Selection while the explorer is the active surface.
    pub selection_active_bg: Color,
    pub selection_active_fg: Color,
    /// Selection while another input (an editor, a field) has the keyboard.
    pub selection_inactive_bg: Color,
    pub selection_inactive_fg: Color,
    /// The focus row's outline.
    pub focus_outline: Color,
    pub focus_outline_width: f64,
    /// The drop-target highlight (mixed onto the row).
    pub drop_target_bg: Color,
    pub drop_target_strength: f32,
    /// Indent guides (`None` = don't draw them).
    pub indent_guide: Option<Color>,
    /// Label colours.
    pub text: Color,
    pub muted_text: Color,
    /// The default icon / twistie tint.
    pub icon_tint: Color,
    /// Decoration colours (see [`FileExplorer::set_decorations`]).
    pub git_modified: Color,
    pub git_added: Color,
    pub git_deleted: Color,
    pub git_untracked: Color,
    pub git_ignored: Color,
    pub git_conflict: Color,
    pub git_renamed: Color,
    pub error: Color,
    pub warning: Color,
    /// Fixed row height (`None` = size to content + padding).
    pub row_height: Option<f64>,
    /// Row padding (also sets the height when `row_height` is `None`).
    pub row_padding: EdgeInsets,
    /// Indent per depth level.
    pub indent: f64,
    pub icon_size: f64,
    pub twistie_size: f64,
    pub font_size: f64,
    /// Label font family (`None` = the theme's).
    pub font_family: Option<String>,
}

impl ExplorerStyle {
    /// The tokens derived from a Pebbles theme (the default look).
    pub fn from_theme(th: &crate::Theme) -> Self {
        let c = th.colors;
        let (dark, rgb) = (th.dark, |r, g, b| Color::from_rgb8(r, g, b));
        ExplorerStyle {
            row_bg: c.background,
            hover_bg: c.foreground,
            hover_strength: 0.05,
            selection_active_bg: c.accent,
            selection_active_fg: c.accent_foreground,
            selection_inactive_bg: mix(c.background, c.accent, 0.6),
            selection_inactive_fg: c.foreground,
            focus_outline: c.ring,
            focus_outline_width: 1.0,
            drop_target_bg: c.primary,
            drop_target_strength: 0.18,
            indent_guide: Some(mix(c.background, c.foreground, 0.12)),
            text: c.foreground,
            muted_text: c.muted_foreground,
            icon_tint: c.muted_foreground,
            // VS Code's gitDecoration defaults (dark / light).
            git_modified: if dark { rgb(226, 192, 141) } else { rgb(137, 81, 0) },
            git_added: if dark { rgb(129, 184, 139) } else { rgb(88, 126, 10) },
            git_deleted: if dark { rgb(199, 78, 57) } else { rgb(173, 6, 6) },
            git_untracked: if dark { rgb(115, 201, 145) } else { rgb(0, 124, 0) },
            git_ignored: if dark { rgb(140, 140, 140) } else { rgb(142, 142, 144) },
            git_conflict: if dark { rgb(228, 103, 107) } else { rgb(173, 6, 6) },
            git_renamed: if dark { rgb(115, 201, 145) } else { rgb(0, 124, 0) },
            error: c.destructive,
            warning: c.warning,
            row_height: None,
            row_padding: th.pad(6.0, 3.0),
            indent: 14.0,
            icon_size: 16.0,
            twistie_size: 14.0,
            font_size: 13.5,
            font_family: None,
        }
    }
}

/// The installed style override (a function of the theme).
pub(super) type StyleFn = Rc<dyn Fn(&crate::Theme) -> ExplorerStyle>;

impl FileExplorer {
    /// Override the explorer's tokens. `f` gets the current Pebbles theme and
    /// runs on every render, so light/dark and theme switches apply live.
    pub fn set_style(&self, f: impl Fn(&crate::Theme) -> ExplorerStyle + 'static) {
        self.style_fn.set(Some(Rc::new(f)));
    }

    /// Back to the theme-derived tokens.
    pub fn clear_style(&self) {
        self.style_fn.set(None);
    }

    /// The tokens in effect now (reactive: tracks the theme and the override).
    pub fn style(&self) -> ExplorerStyle {
        let th = theme();
        match self.style_fn.get() {
            Some(f) => f(&th),
            None => ExplorerStyle::from_theme(&th),
        }
    }
}
