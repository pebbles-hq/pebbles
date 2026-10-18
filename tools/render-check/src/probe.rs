//! Analytic probes (§6.1 reference #2, V16): closed-form expectations at named pixels or
//! regions. Each probe names the case it judges; its geometry is that case's §7.2
//! construction, so the case and the probe must change together. Probes sample named
//! regions; none counts non-background pixels over a whole frame (00 N14).

use crate::case::Case;
use crate::metric::Rgba8;
use crate::rng::XorShift64;

const WHITE: [u8; 4] = [255, 255, 255, 255];

/// The pixel at device `(x, y)` (clamped into the image).
pub fn px(img: &Rgba8, x: u32, y: u32) -> [u8; 4] {
    img.px(x.min(img.width.saturating_sub(1)), y.min(img.height.saturating_sub(1)))
}

/// Every channel within `tol`.
pub fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    (0..4).all(|c| a[c].abs_diff(b[c]) <= tol)
}

/// `(2126 r + 7152 g + 722 b) / 10000` (same as the metric).
pub fn luma(p: [u8; 4]) -> u32 {
    crate::metric::luma(p)
}

/// Logical → device px: `floor(v * scale)`.
pub fn dev(v: f64, scale: f64) -> u32 {
    (v * scale).floor().max(0.0) as u32
}

/// Every pixel outside the device rects `(x0, y0, x1, y1)` (half-open) equals `bg` exactly.
pub fn all_bg_outside(img: &Rgba8, rects_device: &[(u32, u32, u32, u32)], bg: [u8; 4]) -> Result<(), String> {
    for y in 0..img.height {
        for x in 0..img.width {
            let inside = rects_device.iter().any(|&(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
            let p = img.px(x, y);
            if !inside && p != bg {
                return Err(format!("pixel ({x},{y}) = {p:?}, want background {bg:?}"));
            }
        }
    }
    Ok(())
}

/// The procedural noise image of the corpus (§7.2 `img/`): `w × h` opaque RGBA8, each
/// pixel the low three bytes of `XorShift64(seed).next_u64()`.
pub fn noise_rgba8(w: u32, h: u32, seed: u64) -> Vec<u8> {
    let mut rng = XorShift64::new(seed);
    (0..w * h)
        .flat_map(|_| {
            let v = rng.next_u64().to_le_bytes();
            [v[0], v[1], v[2], 255]
        })
        .collect()
}

fn fraction_near(img: &Rgba8, (x0, y0, x1, y1): (u32, u32, u32, u32), want: [u8; 4], tol: u8) -> (u64, u64) {
    let (mut hit, mut all) = (0, 0);
    for y in y0..y1.min(img.height) {
        for x in x0..x1.min(img.width) {
            all += 1;
            hit += u64::from(near(img.px(x, y), want, tol));
        }
    }
    (hit, all)
}

/// `shadow/elevation_card`: the V06 card occupies logical (40,40)–(120,120) on `#F0F0F0`;
/// its center is white — the shadow does not paint over its own card (C07).
pub fn card_interior_white(img: &Rgba8, scale: f64) -> Result<(), String> {
    let p = px(img, dev(80.0, scale), dev(80.0, scale));
    if near(p, WHITE, 1) { Ok(()) } else { Err(format!("card center {p:?}, want white ±1")) }
}

/// `shadow/elevation_card`: 6 px below the card (logical (80,126)) is darker than the
/// `#F0F0F0` background by ≥ 10 luma — the shadow is drawn.
pub fn shadow_below_card(img: &Rgba8, scale: f64) -> Result<(), String> {
    let p = px(img, dev(80.0, scale), dev(126.0, scale));
    if luma(p) + 10 <= 0xF0 {
        Ok(())
    } else {
        Err(format!("below card {p:?} (luma {}), want ≤ 230", luma(p)))
    }
}

/// `layer/opacity_zero`, `clip/rect_offscreen`, `clip/empty_intersection`: nothing is
/// visible, so the whole (white) canvas — the named region — is background.
pub fn all_background(img: &Rgba8, _scale: f64) -> Result<(), String> {
    all_bg_outside(img, &[], WHITE)
}

/// The child color `layer/shader_mask_destin` masks (a full-size box under the mask).
pub const MASK_CHILD: [u8; 4] = [0x33, 0x66, 0xFF, 255];

/// `layer/shader_mask_destin`: the DestIn gradient fades the child over logical x 0..200;
/// on the middle row the leftmost column is the child color ±4, the rightmost bg ±4.
pub fn mask_fades_left_to_right(img: &Rgba8, scale: f64) -> Result<(), String> {
    let y = img.height / 2;
    let (l, r) = (px(img, 0, y), px(img, dev(200.0, scale).saturating_sub(1), y));
    if !near(l, MASK_CHILD, 4) {
        return Err(format!("leftmost {l:?}, want child {MASK_CHILD:?} ±4"));
    }
    if !near(r, WHITE, 4) {
        return Err(format!("rightmost {r:?}, want background ±4"));
    }
    Ok(())
}

/// `img/huge_10000x300`: the solid red image is drawn into logical (0,0)–(1000,30);
/// ≥ 95 % of that rect is `#FF0000` ±2 (downscaled, never dropped).
pub fn huge_image_downscaled_red(img: &Rgba8, scale: f64) -> Result<(), String> {
    let (hit, all) = fraction_near(img, (0, 0, dev(1000.0, scale), dev(30.0, scale)), [255, 0, 0, 255], 2);
    if all > 0 && hit * 100 >= all * 95 {
        Ok(())
    } else {
        Err(format!("{hit}/{all} red pixels, want ≥ 95 %"))
    }
}

/// `text/emoji_color`: in the first text line (logical y 0..64) ≥ 20 pixels differ from
/// the white background and ≥ 1 is chromatic (`|r − g| > 20`).
pub fn emoji_has_color(img: &Rgba8, scale: f64) -> Result<(), String> {
    let (mut ink, mut chroma) = (0u32, 0u32);
    for y in 0..dev(64.0, scale).min(img.height) {
        for x in 0..img.width {
            let p = img.px(x, y);
            ink += u32::from(!near(p, WHITE, 8));
            chroma += u32::from(p[0].abs_diff(p[1]) > 20);
        }
    }
    if ink >= 20 && chroma >= 1 { Ok(()) } else { Err(format!("{ink} ink / {chroma} chromatic pixels")) }
}

/// Every pixel is opaque (the canvas is opaque: anything else is NaN/undefined blending).
pub fn all_opaque(img: &Rgba8, _scale: f64) -> Result<(), String> {
    match img.data.as_chunks::<4>().0.iter().position(|p| p[3] != 255) {
        Some(i) => {
            let i = i as u32;
            Err(format!("pixel ({},{}) not opaque", i % img.width, i / img.width))
        }
        None => Ok(()),
    }
}

/// `adversarial/*`: invalid input must not leak garbage. Every pixel is opaque (the canvas
/// is opaque, so non-opaque output means NaN/undefined blending) and the 4 × 4 corner
/// patches — outside every valid shape the cases draw — stay white.
pub fn adversarial_bg_outside_valid(img: &Rgba8, _scale: f64) -> Result<(), String> {
    if let Some(i) = img.data.as_chunks::<4>().0.iter().position(|p| p[3] != 255) {
        let i = i as u32;
        return Err(format!("pixel ({},{}) not opaque", i % img.width, i / img.width));
    }
    let (w, h) = (img.width, img.height);
    let k = 4.min(w).min(h);
    for (x0, y0) in [(0, 0), (w - k, 0), (0, h - k), (w - k, h - k)] {
        let (hit, all) = fraction_near(img, (x0, y0, x0 + k, y0 + k), WHITE, 0);
        if hit != all {
            return Err(format!("corner patch at ({x0},{y0}) is not background"));
        }
    }
    Ok(())
}

/// `brush/image_fill_*`: a 16×16 `noise_rgba8(.., 7)` image under `Affine::scale(2.)` fills
/// logical (8,8)–(120,120); each texel's center inside the rect matches the image ±4.
pub fn image_fill_texels(img: &Rgba8, scale: f64) -> Result<(), String> {
    let tex = noise_rgba8(16, 16, 7);
    let mut checked = 0;
    for ty in 4..60 {
        for tx in 4..60 {
            let (lx, ly) = (2.0 * f64::from(tx) + 1.0, 2.0 * f64::from(ty) + 1.0);
            let i = (((ty % 16) * 16 + tx % 16) * 4) as usize;
            let want = [tex[i], tex[i + 1], tex[i + 2], 255];
            let got = px(img, dev(lx, scale), dev(ly, scale));
            if !near(got, want, 4) {
                return Err(format!("texel ({tx},{ty}) at logical ({lx},{ly}) = {got:?}, want {want:?} ±4"));
            }
            checked += 1;
        }
    }
    if checked > 0 { Ok(()) } else { Err("no texel inside the rect".into()) }
}

/// Run every probe of `case` on `img`, appending `(name, None | Some(message))` to `out`.
/// Returns the number of failing probes. A panicking probe counts as failing.
pub fn run_probes(case: &Case, img: &Rgba8, out: &mut Vec<(&'static str, Option<String>)>) -> usize {
    let mut fails = 0;
    for (name, p) in &case.probes {
        let res = std::panic::catch_unwind(|| p(img, case.scale))
            .unwrap_or_else(|_| Err("probe panicked".to_string()));
        if res.is_err() {
            fails += 1;
        }
        out.push((name, res.err()));
    }
    fails
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn px_reads_rgba() {
        let mut img = Rgba8::filled(3, 2, WHITE);
        img.set(2, 1, [1, 2, 3, 4]);
        assert_eq!(px(&img, 2, 1), [1, 2, 3, 4]);
        assert_eq!(px(&img, 9, 9), [1, 2, 3, 4], "clamped");
    }

    #[test]
    fn near_tolerates_delta() {
        assert!(near([10, 10, 10, 255], [12, 8, 10, 255], 2));
        assert!(!near([10, 10, 10, 255], [13, 10, 10, 255], 2));
    }

    #[test]
    fn all_bg_outside_reports_offender() {
        let mut img = Rgba8::filled(8, 8, WHITE);
        img.set(2, 2, [0, 0, 0, 255]);
        assert!(all_bg_outside(&img, &[(1, 1, 4, 4)], WHITE).is_ok());
        img.set(6, 5, [9, 9, 9, 255]);
        let e = all_bg_outside(&img, &[(1, 1, 4, 4)], WHITE).unwrap_err();
        assert!(e.contains("(6,5)"), "{e}");
    }

    #[test]
    fn luma_formula() {
        assert_eq!(luma(WHITE), 255);
        assert_eq!(luma([255, 0, 0, 255]), 54);
    }

    #[test]
    fn dev_floors() {
        assert_eq!((dev(80.0, 1.0), dev(80.0, 1.25), dev(10.9, 1.0)), (80, 100, 10));
    }

    #[test]
    fn probes_accept_their_ideal_images() {
        assert!(all_background(&Rgba8::filled(16, 16, WHITE), 1.0).is_ok());
        assert!(all_background(&Rgba8::filled(16, 16, [0, 0, 0, 255]), 1.0).is_err());
        assert!(adversarial_bg_outside_valid(&Rgba8::filled(16, 16, WHITE), 1.0).is_ok());
        assert!(adversarial_bg_outside_valid(&Rgba8::filled(16, 16, [255, 255, 255, 0]), 1.0).is_err());
        let mut fill = Rgba8::filled(128, 128, WHITE);
        let tex = noise_rgba8(16, 16, 7);
        for y in 8..120u32 {
            for x in 8..120u32 {
                let i = ((((y / 2) % 16) * 16 + (x / 2) % 16) * 4) as usize;
                fill.set(x, y, [tex[i], tex[i + 1], tex[i + 2], 255]);
            }
        }
        assert!(image_fill_texels(&fill, 1.0).is_ok());
        assert!(image_fill_texels(&Rgba8::filled(128, 128, WHITE), 1.0).is_err());
    }
}
