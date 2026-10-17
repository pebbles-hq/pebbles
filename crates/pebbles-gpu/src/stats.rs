//! Per-frame renderer counters (reset every render). C12 lands the creation counters;
//! C17 adds `Unsupported`/`record`/`unsupported`; M04 completes the 10 §9.1 fields.

/// Per-frame counters. Reset at the top of every render; read with `Renderer::last_stats()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FrameStats {
    pub ops: u32,
    pub cmds: u32,
    pub draw_calls: u32,
    pub pipeline_switches: u32,
    pub render_passes: u32,
    pub culled: u32,
    pub glyphs_rasterized: u32,
    pub bytes_uploaded: u64,
    pub prepare_us: u32,
    pub execute_us: u32,
    // creations — all 0 in steady state (P1)
    pub buffers_created: u32,
    pub bind_groups_created: u32,
    pub textures_created: u32,
    pub texture_views_created: u32,
    pub pipelines_created: u32,
}

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
