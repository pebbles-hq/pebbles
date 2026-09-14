//! Animation widgets — implicit (`Animated*`) and explicit (`*Transition`).

mod animated;
mod animated_list;
mod motion;
mod transform;

pub use animated::{AnimatedContainer, animated_container};
pub use animated_list::{AnimatedGrid, AnimatedList, animated_grid, animated_list};
pub use motion::{
    AnimatedAlign, AnimatedCrossFade, AnimatedOpacity, AnimatedPadding, AnimatedPositioned, AnimatedRotation,
    AnimatedScale, AnimatedSlide, AnimatedSwitcher, DecoratedBoxTransition, Dismissible, FadeTransition,
    PositionedTransition, RotationTransition, ScaleTransition, SizeTransition, SlideTransition,
    animated_align, animated_cross_fade, animated_opacity, animated_padding, animated_positioned,
    animated_rotation, animated_scale, animated_slide, animated_switcher, decorated_box_transition,
    dismissible, fade_transition, positioned_transition, rotation_transition, scale_transition,
    size_transition, slide_transition,
};
pub use transform::{Transform, transform};
