//! Per-frame renderer counters (reset every render). C12 lands the creation counters;
//! C17 adds `Unsupported`/`record`/`unsupported`; M04 completes the 10 §9.1 fields.

/// Per-frame counters. Reset at the top of every render; read with `Renderer::last_stats()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FrameStats {
    /// Display-list ops walked.
    pub ops: u32,
    /// Draw commands in the frame plan.
    pub cmds: u32,
    /// Draw calls issued.
    pub draw_calls: u32,
    /// Pipeline changes in the pass.
    pub pipeline_switches: u32,
    /// Render passes begun.
    pub render_passes: u32,
    /// Ops culled as offscreen.
    pub culled: u32,
    /// Glyphs rasterized this frame (atlas misses).
    pub glyphs_rasterized: u32,
    /// Bytes written to GPU buffers.
    pub bytes_uploaded: u64,
    /// CPU prepare time, microseconds.
    pub prepare_us: u32,
    /// Encode + submit time, microseconds.
    pub execute_us: u32,
    // creations — all 0 in steady state (P1)
    /// GPU buffers created.
    pub buffers_created: u32,
    /// Bind groups created.
    pub bind_groups_created: u32,
    /// Textures created.
    pub textures_created: u32,
    /// Texture views created.
    pub texture_views_created: u32,
    /// Render pipelines created.
    pub pipelines_created: u32,
}

#[allow(clippy::derivable_impls)] // hand-written per 10 §9.1
impl Default for FrameStats {
    fn default() -> Self {
        FrameStats {
            ops: 0,
            cmds: 0,
            draw_calls: 0,
            pipeline_switches: 0,
            render_passes: 0,
            culled: 0,
            glyphs_rasterized: 0,
            bytes_uploaded: 0,
            prepare_us: 0,
            execute_us: 0,
            buffers_created: 0,
            bind_groups_created: 0,
            textures_created: 0,
            texture_views_created: 0,
            pipelines_created: 0,
        }
    }
}
