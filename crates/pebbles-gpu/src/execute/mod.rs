//! Execute: the wgpu-side draw/encode helpers.

pub(crate) mod encode;
pub(crate) mod pipelines;
pub(crate) mod resolve;
pub(crate) mod resources;

pub(crate) use encode::encode;
