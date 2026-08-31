//! [`Field`] — a generic labeled form control: a label above any control, with a
//! muted description or a destructive error message below. Generalizes the
//! label/helper/error block that `TextField` grows internally, so *any* widget
//! (Select, Switch, a custom control) gets the same form treatment.
//!
//! ```ignore
//! field(select(opts).value(v))
//!     .label("Country")
//!     .description("Where you'll be billed.")
//!     .error_opt(validation_error) // Some(..) replaces the description in red
//! ```

use pebbles_foundation::CrossAxisAlignment;
use pebbles_foundation::{MainAxisSize};

use crate::theme::theme;
use crate::widgets::{SizedBox, column, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A labeled control with an optional description / error. Build with [`field`].
pub struct Field {
    control: Option<AnyWidget>,
    label: Option<String>,
    description: Option<String>,
    error: Option<String>,
}

/// Wrap `control` in a form field. Add `.label(..)` / `.description(..)` / `.error_opt(..)`.
pub fn field(control: impl IntoWidget) -> Field {
    Field { control: Some(control.into_widget()), label: None, description: None, error: None }
}

impl Field {
    /// The label rendered above the control.
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    /// Muted helper text below the control (hidden when an error is set).
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    /// A destructive error message below the control; replaces the description.
    pub fn error(mut self, s: impl Into<String>) -> Self {
        self.error = Some(s.into());
        self
    }
    /// Set the error from an `Option` (the validation-friendly form): `Some` shows the
    /// red message + replaces the description, `None` shows the description.
    pub fn error_opt(mut self, s: Option<impl Into<String>>) -> Self {
        self.error = s.map(Into::into);
        self
    }
}

impl IntoWidget for Field {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let mut col: Vec<AnyWidget> = Vec::new();
        if let Some(lbl) = self.label.take() {
            col.push(text(lbl).size(13.5).weight(500.0).color(c.foreground).into_widget());
            col.push(SizedBox::spacer(0.0, 7.0).into_widget());
        }
        col.push(self.control.take().unwrap_or_else(|| SizedBox::spacer(0.0, 0.0).into_widget()));
        if let Some(err) = self.error.take() {
            col.push(SizedBox::spacer(0.0, 6.0).into_widget());
            col.push(text(err).size(12.5).color(c.destructive).into_widget());
        } else if let Some(help) = self.description.take() {
            col.push(SizedBox::spacer(0.0, 6.0).into_widget());
            col.push(text(help).size(12.5).color(c.muted_foreground).into_widget());
        }
        column(col).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).into_widget()
    }
}
