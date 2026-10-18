//! Nine-slice split (02 R36 step 2, V51 step 2): source rect → destination rect per slice.

use kurbo::Rect;

/// The (source, destination) rects of a `w × h` image with stretchable `center` (image px,
/// clamped to the image) drawn into `dst`. Corners keep their size unless they do not fit, in
/// which case both corners scale by `dst / (corners)` and the center column/row is dropped
/// (Flutter's rule). Empty slices are skipped.
pub fn nine_slice_rects(w: f64, h: f64, center: Rect, dst: Rect) -> Vec<(Rect, Rect)> {
    let c = Rect::new(
        center.x0.clamp(0.0, w),
        center.y0.clamp(0.0, h),
        center.x1.clamp(0.0, w),
        center.y1.clamp(0.0, h),
    );
    let axis = |n: f64, c0: f64, c1: f64, d0: f64, d1: f64| -> ([f64; 4], [f64; 4]) {
        let src = [0.0, c0, c1, n];
        let corners = c0 + (n - c1);
        let len = d1 - d0;
        if corners > len && corners > 0.0 {
            let k = len / corners;
            // The center collapses: the two corners meet at d0 + c0·k.
            (src, [d0, d0 + c0 * k, d0 + c0 * k, d1])
        } else {
            (src, [d0, d0 + c0, d1 - (n - c1), d1])
        }
    };
    let (sx, dx) = axis(w, c.x0, c.x1, dst.x0, dst.x1);
    let (sy, dy) = axis(h, c.y0, c.y1, dst.y0, dst.y1);
    let mut out = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            let s = Rect::new(sx[i], sy[j], sx[i + 1], sy[j + 1]);
            let d = Rect::new(dx[i], dy[j], dx[i + 1], dy[j + 1]);
            if s.width() > 0.0 && s.height() > 0.0 && d.width() > 0.0 && d.height() > 0.0 {
                out.push((s, d));
            }
        }
    }
    out
}
