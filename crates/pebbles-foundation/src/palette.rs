//! The built-in color palette — the full Tailwind/shadcn color scale.
//!
//! Every family (`slate`, `gray`, `zinc`, `neutral`, `stone`, `red`, `orange`, `amber`,
//! `yellow`, `lime`, `green`, `emerald`, `teal`, `cyan`, `sky`, `blue`, `indigo`,
//! `violet`, `purple`, `fuchsia`, `pink`, `rose`) exposes shades `S50`..`S950`:
//! `palette::blue::S500`, `palette::zinc::S900`, etc.
//!
//! Developers can also make their own: any [`Color::from_rgba8`] value, or a custom
//! module of consts. Themes are customized by copying a base and overriding fields
//! (both `Theme` and `Colors` are `Copy`).

use crate::color::Color;

pub const TRANSPARENT: Color = Color::from_rgba8(0, 0, 0, 0);
pub const BLACK: Color = Color::from_rgba8(0, 0, 0, 255);
pub const WHITE: Color = Color::from_rgba8(255, 255, 255, 255);

/// The Tailwind `slate` scale.
pub mod slate {
    use super::Color;
    /// `#f8fafc`
    pub const S50: Color = Color::from_rgba8(248, 250, 252, 255);
    /// `#f1f5f9`
    pub const S100: Color = Color::from_rgba8(241, 245, 249, 255);
    /// `#e2e8f0`
    pub const S200: Color = Color::from_rgba8(226, 232, 240, 255);
    /// `#cbd5e1`
    pub const S300: Color = Color::from_rgba8(203, 213, 225, 255);
    /// `#94a3b8`
    pub const S400: Color = Color::from_rgba8(148, 163, 184, 255);
    /// `#64748b`
    pub const S500: Color = Color::from_rgba8(100, 116, 139, 255);
    /// `#475569`
    pub const S600: Color = Color::from_rgba8(71, 85, 105, 255);
    /// `#334155`
    pub const S700: Color = Color::from_rgba8(51, 65, 85, 255);
    /// `#1e293b`
    pub const S800: Color = Color::from_rgba8(30, 41, 59, 255);
    /// `#0f172a`
    pub const S900: Color = Color::from_rgba8(15, 23, 42, 255);
    /// `#020617`
    pub const S950: Color = Color::from_rgba8(2, 6, 23, 255);
}

/// The Tailwind `gray` scale.
pub mod gray {
    use super::Color;
    /// `#f9fafb`
    pub const S50: Color = Color::from_rgba8(249, 250, 251, 255);
    /// `#f3f4f6`
    pub const S100: Color = Color::from_rgba8(243, 244, 246, 255);
    /// `#e5e7eb`
    pub const S200: Color = Color::from_rgba8(229, 231, 235, 255);
    /// `#d1d5db`
    pub const S300: Color = Color::from_rgba8(209, 213, 219, 255);
    /// `#9ca3af`
    pub const S400: Color = Color::from_rgba8(156, 163, 175, 255);
    /// `#6b7280`
    pub const S500: Color = Color::from_rgba8(107, 114, 128, 255);
    /// `#4b5563`
    pub const S600: Color = Color::from_rgba8(75, 85, 99, 255);
    /// `#374151`
    pub const S700: Color = Color::from_rgba8(55, 65, 81, 255);
    /// `#1f2937`
    pub const S800: Color = Color::from_rgba8(31, 41, 55, 255);
    /// `#111827`
    pub const S900: Color = Color::from_rgba8(17, 24, 39, 255);
    /// `#030712`
    pub const S950: Color = Color::from_rgba8(3, 7, 18, 255);
}

/// The Tailwind `zinc` scale.
pub mod zinc {
    use super::Color;
    /// `#fafafa`
    pub const S50: Color = Color::from_rgba8(250, 250, 250, 255);
    /// `#f4f4f5`
    pub const S100: Color = Color::from_rgba8(244, 244, 245, 255);
    /// `#e4e4e7`
    pub const S200: Color = Color::from_rgba8(228, 228, 231, 255);
    /// `#d4d4d8`
    pub const S300: Color = Color::from_rgba8(212, 212, 216, 255);
    /// `#a1a1aa`
    pub const S400: Color = Color::from_rgba8(161, 161, 170, 255);
    /// `#71717a`
    pub const S500: Color = Color::from_rgba8(113, 113, 122, 255);
    /// `#52525b`
    pub const S600: Color = Color::from_rgba8(82, 82, 91, 255);
    /// `#3f3f46`
    pub const S700: Color = Color::from_rgba8(63, 63, 70, 255);
    /// `#27272a`
    pub const S800: Color = Color::from_rgba8(39, 39, 42, 255);
    /// `#18181b`
    pub const S900: Color = Color::from_rgba8(24, 24, 27, 255);
    /// `#09090b`
    pub const S950: Color = Color::from_rgba8(9, 9, 11, 255);
}

/// The Tailwind `neutral` scale.
pub mod neutral {
    use super::Color;
    /// `#fafafa`
    pub const S50: Color = Color::from_rgba8(250, 250, 250, 255);
    /// `#f5f5f5`
    pub const S100: Color = Color::from_rgba8(245, 245, 245, 255);
    /// `#e5e5e5`
    pub const S200: Color = Color::from_rgba8(229, 229, 229, 255);
    /// `#d4d4d4`
    pub const S300: Color = Color::from_rgba8(212, 212, 212, 255);
    /// `#a3a3a3`
    pub const S400: Color = Color::from_rgba8(163, 163, 163, 255);
    /// `#737373`
    pub const S500: Color = Color::from_rgba8(115, 115, 115, 255);
    /// `#525252`
    pub const S600: Color = Color::from_rgba8(82, 82, 82, 255);
    /// `#404040`
    pub const S700: Color = Color::from_rgba8(64, 64, 64, 255);
    /// `#262626`
    pub const S800: Color = Color::from_rgba8(38, 38, 38, 255);
    /// `#171717`
    pub const S900: Color = Color::from_rgba8(23, 23, 23, 255);
    /// `#0a0a0a`
    pub const S950: Color = Color::from_rgba8(10, 10, 10, 255);
}

/// The Tailwind `stone` scale.
pub mod stone {
    use super::Color;
    /// `#fafaf9`
    pub const S50: Color = Color::from_rgba8(250, 250, 249, 255);
    /// `#f5f5f4`
    pub const S100: Color = Color::from_rgba8(245, 245, 244, 255);
    /// `#e7e5e4`
    pub const S200: Color = Color::from_rgba8(231, 229, 228, 255);
    /// `#d6d3d1`
    pub const S300: Color = Color::from_rgba8(214, 211, 209, 255);
    /// `#a8a29e`
    pub const S400: Color = Color::from_rgba8(168, 162, 158, 255);
    /// `#78716c`
    pub const S500: Color = Color::from_rgba8(120, 113, 108, 255);
    /// `#57534e`
    pub const S600: Color = Color::from_rgba8(87, 83, 78, 255);
    /// `#44403c`
    pub const S700: Color = Color::from_rgba8(68, 64, 60, 255);
    /// `#292524`
    pub const S800: Color = Color::from_rgba8(41, 37, 36, 255);
    /// `#1c1917`
    pub const S900: Color = Color::from_rgba8(28, 25, 23, 255);
    /// `#0c0a09`
    pub const S950: Color = Color::from_rgba8(12, 10, 9, 255);
}

/// The Tailwind `red` scale.
pub mod red {
    use super::Color;
    /// `#fef2f2`
    pub const S50: Color = Color::from_rgba8(254, 242, 242, 255);
    /// `#fee2e2`
    pub const S100: Color = Color::from_rgba8(254, 226, 226, 255);
    /// `#fecaca`
    pub const S200: Color = Color::from_rgba8(254, 202, 202, 255);
    /// `#fca5a5`
    pub const S300: Color = Color::from_rgba8(252, 165, 165, 255);
    /// `#f87171`
    pub const S400: Color = Color::from_rgba8(248, 113, 113, 255);
    /// `#ef4444`
    pub const S500: Color = Color::from_rgba8(239, 68, 68, 255);
    /// `#dc2626`
    pub const S600: Color = Color::from_rgba8(220, 38, 38, 255);
    /// `#b91c1c`
    pub const S700: Color = Color::from_rgba8(185, 28, 28, 255);
    /// `#991b1b`
    pub const S800: Color = Color::from_rgba8(153, 27, 27, 255);
    /// `#7f1d1d`
    pub const S900: Color = Color::from_rgba8(127, 29, 29, 255);
    /// `#450a0a`
    pub const S950: Color = Color::from_rgba8(69, 10, 10, 255);
}

/// The Tailwind `orange` scale.
pub mod orange {
    use super::Color;
    /// `#fff7ed`
    pub const S50: Color = Color::from_rgba8(255, 247, 237, 255);
    /// `#ffedd5`
    pub const S100: Color = Color::from_rgba8(255, 237, 213, 255);
    /// `#fed7aa`
    pub const S200: Color = Color::from_rgba8(254, 215, 170, 255);
    /// `#fdba74`
    pub const S300: Color = Color::from_rgba8(253, 186, 116, 255);
    /// `#fb923c`
    pub const S400: Color = Color::from_rgba8(251, 146, 60, 255);
    /// `#f97316`
    pub const S500: Color = Color::from_rgba8(249, 115, 22, 255);
    /// `#ea580c`
    pub const S600: Color = Color::from_rgba8(234, 88, 12, 255);
    /// `#c2410c`
    pub const S700: Color = Color::from_rgba8(194, 65, 12, 255);
    /// `#9a3412`
    pub const S800: Color = Color::from_rgba8(154, 52, 18, 255);
    /// `#7c2d12`
    pub const S900: Color = Color::from_rgba8(124, 45, 18, 255);
    /// `#431407`
    pub const S950: Color = Color::from_rgba8(67, 20, 7, 255);
}

/// The Tailwind `amber` scale.
pub mod amber {
    use super::Color;
    /// `#fffbeb`
    pub const S50: Color = Color::from_rgba8(255, 251, 235, 255);
    /// `#fef3c7`
    pub const S100: Color = Color::from_rgba8(254, 243, 199, 255);
    /// `#fde68a`
    pub const S200: Color = Color::from_rgba8(253, 230, 138, 255);
    /// `#fcd34d`
    pub const S300: Color = Color::from_rgba8(252, 211, 77, 255);
    /// `#fbbf24`
    pub const S400: Color = Color::from_rgba8(251, 191, 36, 255);
    /// `#f59e0b`
    pub const S500: Color = Color::from_rgba8(245, 158, 11, 255);
    /// `#d97706`
    pub const S600: Color = Color::from_rgba8(217, 119, 6, 255);
    /// `#b45309`
    pub const S700: Color = Color::from_rgba8(180, 83, 9, 255);
    /// `#92400e`
    pub const S800: Color = Color::from_rgba8(146, 64, 14, 255);
    /// `#78350f`
    pub const S900: Color = Color::from_rgba8(120, 53, 15, 255);
    /// `#451a03`
    pub const S950: Color = Color::from_rgba8(69, 26, 3, 255);
}

/// The Tailwind `yellow` scale.
pub mod yellow {
    use super::Color;
    /// `#fefce8`
    pub const S50: Color = Color::from_rgba8(254, 252, 232, 255);
    /// `#fef9c3`
    pub const S100: Color = Color::from_rgba8(254, 249, 195, 255);
    /// `#fef08a`
    pub const S200: Color = Color::from_rgba8(254, 240, 138, 255);
    /// `#fde047`
    pub const S300: Color = Color::from_rgba8(253, 224, 71, 255);
    /// `#facc15`
    pub const S400: Color = Color::from_rgba8(250, 204, 21, 255);
    /// `#eab308`
    pub const S500: Color = Color::from_rgba8(234, 179, 8, 255);
    /// `#ca8a04`
    pub const S600: Color = Color::from_rgba8(202, 138, 4, 255);
    /// `#a16207`
    pub const S700: Color = Color::from_rgba8(161, 98, 7, 255);
    /// `#854d0e`
    pub const S800: Color = Color::from_rgba8(133, 77, 14, 255);
    /// `#713f12`
    pub const S900: Color = Color::from_rgba8(113, 63, 18, 255);
    /// `#422006`
    pub const S950: Color = Color::from_rgba8(66, 32, 6, 255);
}

/// The Tailwind `lime` scale.
pub mod lime {
    use super::Color;
    /// `#f7fee7`
    pub const S50: Color = Color::from_rgba8(247, 254, 231, 255);
    /// `#ecfccb`
    pub const S100: Color = Color::from_rgba8(236, 252, 203, 255);
    /// `#d9f99d`
    pub const S200: Color = Color::from_rgba8(217, 249, 157, 255);
    /// `#bef264`
    pub const S300: Color = Color::from_rgba8(190, 242, 100, 255);
    /// `#a3e635`
    pub const S400: Color = Color::from_rgba8(163, 230, 53, 255);
    /// `#84cc16`
    pub const S500: Color = Color::from_rgba8(132, 204, 22, 255);
    /// `#65a30d`
    pub const S600: Color = Color::from_rgba8(101, 163, 13, 255);
    /// `#4d7c0f`
    pub const S700: Color = Color::from_rgba8(77, 124, 15, 255);
    /// `#3f6212`
    pub const S800: Color = Color::from_rgba8(63, 98, 18, 255);
    /// `#365314`
    pub const S900: Color = Color::from_rgba8(54, 83, 20, 255);
    /// `#1a2e05`
    pub const S950: Color = Color::from_rgba8(26, 46, 5, 255);
}

/// The Tailwind `green` scale.
pub mod green {
    use super::Color;
    /// `#f0fdf4`
    pub const S50: Color = Color::from_rgba8(240, 253, 244, 255);
    /// `#dcfce7`
    pub const S100: Color = Color::from_rgba8(220, 252, 231, 255);
    /// `#bbf7d0`
    pub const S200: Color = Color::from_rgba8(187, 247, 208, 255);
    /// `#86efac`
    pub const S300: Color = Color::from_rgba8(134, 239, 172, 255);
    /// `#4ade80`
    pub const S400: Color = Color::from_rgba8(74, 222, 128, 255);
    /// `#22c55e`
    pub const S500: Color = Color::from_rgba8(34, 197, 94, 255);
    /// `#16a34a`
    pub const S600: Color = Color::from_rgba8(22, 163, 74, 255);
    /// `#15803d`
    pub const S700: Color = Color::from_rgba8(21, 128, 61, 255);
    /// `#166534`
    pub const S800: Color = Color::from_rgba8(22, 101, 52, 255);
    /// `#14532d`
    pub const S900: Color = Color::from_rgba8(20, 83, 45, 255);
    /// `#052e16`
    pub const S950: Color = Color::from_rgba8(5, 46, 22, 255);
}

/// The Tailwind `emerald` scale.
pub mod emerald {
    use super::Color;
    /// `#ecfdf5`
    pub const S50: Color = Color::from_rgba8(236, 253, 245, 255);
    /// `#d1fae5`
    pub const S100: Color = Color::from_rgba8(209, 250, 229, 255);
    /// `#a7f3d0`
    pub const S200: Color = Color::from_rgba8(167, 243, 208, 255);
    /// `#6ee7b7`
    pub const S300: Color = Color::from_rgba8(110, 231, 183, 255);
    /// `#34d399`
    pub const S400: Color = Color::from_rgba8(52, 211, 153, 255);
    /// `#10b981`
    pub const S500: Color = Color::from_rgba8(16, 185, 129, 255);
    /// `#059669`
    pub const S600: Color = Color::from_rgba8(5, 150, 105, 255);
    /// `#047857`
    pub const S700: Color = Color::from_rgba8(4, 120, 87, 255);
    /// `#065f46`
    pub const S800: Color = Color::from_rgba8(6, 95, 70, 255);
    /// `#064e3b`
    pub const S900: Color = Color::from_rgba8(6, 78, 59, 255);
    /// `#022c22`
    pub const S950: Color = Color::from_rgba8(2, 44, 34, 255);
}

/// The Tailwind `teal` scale.
pub mod teal {
    use super::Color;
    /// `#f0fdfa`
    pub const S50: Color = Color::from_rgba8(240, 253, 250, 255);
    /// `#ccfbf1`
    pub const S100: Color = Color::from_rgba8(204, 251, 241, 255);
    /// `#99f6e4`
    pub const S200: Color = Color::from_rgba8(153, 246, 228, 255);
    /// `#5eead4`
    pub const S300: Color = Color::from_rgba8(94, 234, 212, 255);
    /// `#2dd4bf`
    pub const S400: Color = Color::from_rgba8(45, 212, 191, 255);
    /// `#14b8a6`
    pub const S500: Color = Color::from_rgba8(20, 184, 166, 255);
    /// `#0d9488`
    pub const S600: Color = Color::from_rgba8(13, 148, 136, 255);
    /// `#0f766e`
    pub const S700: Color = Color::from_rgba8(15, 118, 110, 255);
    /// `#115e59`
    pub const S800: Color = Color::from_rgba8(17, 94, 89, 255);
    /// `#134e4a`
    pub const S900: Color = Color::from_rgba8(19, 78, 74, 255);
    /// `#042f2e`
    pub const S950: Color = Color::from_rgba8(4, 47, 46, 255);
}

/// The Tailwind `cyan` scale.
pub mod cyan {
    use super::Color;
    /// `#ecfeff`
    pub const S50: Color = Color::from_rgba8(236, 254, 255, 255);
    /// `#cffafe`
    pub const S100: Color = Color::from_rgba8(207, 250, 254, 255);
    /// `#a5f3fc`
    pub const S200: Color = Color::from_rgba8(165, 243, 252, 255);
    /// `#67e8f9`
    pub const S300: Color = Color::from_rgba8(103, 232, 249, 255);
    /// `#22d3ee`
    pub const S400: Color = Color::from_rgba8(34, 211, 238, 255);
    /// `#06b6d4`
    pub const S500: Color = Color::from_rgba8(6, 182, 212, 255);
    /// `#0891b2`
    pub const S600: Color = Color::from_rgba8(8, 145, 178, 255);
    /// `#0e7490`
    pub const S700: Color = Color::from_rgba8(14, 116, 144, 255);
    /// `#155e75`
    pub const S800: Color = Color::from_rgba8(21, 94, 117, 255);
    /// `#164e63`
    pub const S900: Color = Color::from_rgba8(22, 78, 99, 255);
    /// `#083344`
    pub const S950: Color = Color::from_rgba8(8, 51, 68, 255);
}

/// The Tailwind `sky` scale.
pub mod sky {
    use super::Color;
    /// `#f0f9ff`
    pub const S50: Color = Color::from_rgba8(240, 249, 255, 255);
    /// `#e0f2fe`
    pub const S100: Color = Color::from_rgba8(224, 242, 254, 255);
    /// `#bae6fd`
    pub const S200: Color = Color::from_rgba8(186, 230, 253, 255);
    /// `#7dd3fc`
    pub const S300: Color = Color::from_rgba8(125, 211, 252, 255);
    /// `#38bdf8`
    pub const S400: Color = Color::from_rgba8(56, 189, 248, 255);
    /// `#0ea5e9`
    pub const S500: Color = Color::from_rgba8(14, 165, 233, 255);
    /// `#0284c7`
    pub const S600: Color = Color::from_rgba8(2, 132, 199, 255);
    /// `#0369a1`
    pub const S700: Color = Color::from_rgba8(3, 105, 161, 255);
    /// `#075985`
    pub const S800: Color = Color::from_rgba8(7, 89, 133, 255);
    /// `#0c4a6e`
    pub const S900: Color = Color::from_rgba8(12, 74, 110, 255);
    /// `#082f49`
    pub const S950: Color = Color::from_rgba8(8, 47, 73, 255);
}

/// The Tailwind `blue` scale.
pub mod blue {
    use super::Color;
    /// `#eff6ff`
    pub const S50: Color = Color::from_rgba8(239, 246, 255, 255);
    /// `#dbeafe`
    pub const S100: Color = Color::from_rgba8(219, 234, 254, 255);
    /// `#bfdbfe`
    pub const S200: Color = Color::from_rgba8(191, 219, 254, 255);
    /// `#93c5fd`
    pub const S300: Color = Color::from_rgba8(147, 197, 253, 255);
    /// `#60a5fa`
    pub const S400: Color = Color::from_rgba8(96, 165, 250, 255);
    /// `#3b82f6`
    pub const S500: Color = Color::from_rgba8(59, 130, 246, 255);
    /// `#2563eb`
    pub const S600: Color = Color::from_rgba8(37, 99, 235, 255);
    /// `#1d4ed8`
    pub const S700: Color = Color::from_rgba8(29, 78, 216, 255);
    /// `#1e40af`
    pub const S800: Color = Color::from_rgba8(30, 64, 175, 255);
    /// `#1e3a8a`
    pub const S900: Color = Color::from_rgba8(30, 58, 138, 255);
    /// `#172554`
    pub const S950: Color = Color::from_rgba8(23, 37, 84, 255);
}

/// The Tailwind `indigo` scale.
pub mod indigo {
    use super::Color;
    /// `#eef2ff`
    pub const S50: Color = Color::from_rgba8(238, 242, 255, 255);
    /// `#e0e7ff`
    pub const S100: Color = Color::from_rgba8(224, 231, 255, 255);
    /// `#c7d2fe`
    pub const S200: Color = Color::from_rgba8(199, 210, 254, 255);
    /// `#a5b4fc`
    pub const S300: Color = Color::from_rgba8(165, 180, 252, 255);
    /// `#818cf8`
    pub const S400: Color = Color::from_rgba8(129, 140, 248, 255);
    /// `#6366f1`
    pub const S500: Color = Color::from_rgba8(99, 102, 241, 255);
    /// `#4f46e5`
    pub const S600: Color = Color::from_rgba8(79, 70, 229, 255);
    /// `#4338ca`
    pub const S700: Color = Color::from_rgba8(67, 56, 202, 255);
    /// `#3730a3`
    pub const S800: Color = Color::from_rgba8(55, 48, 163, 255);
    /// `#312e81`
    pub const S900: Color = Color::from_rgba8(49, 46, 129, 255);
    /// `#1e1b4b`
    pub const S950: Color = Color::from_rgba8(30, 27, 75, 255);
}

/// The Tailwind `violet` scale.
pub mod violet {
    use super::Color;
    /// `#f5f3ff`
    pub const S50: Color = Color::from_rgba8(245, 243, 255, 255);
    /// `#ede9fe`
    pub const S100: Color = Color::from_rgba8(237, 233, 254, 255);
    /// `#ddd6fe`
    pub const S200: Color = Color::from_rgba8(221, 214, 254, 255);
    /// `#c4b5fd`
    pub const S300: Color = Color::from_rgba8(196, 181, 253, 255);
    /// `#a78bfa`
    pub const S400: Color = Color::from_rgba8(167, 139, 250, 255);
    /// `#8b5cf6`
    pub const S500: Color = Color::from_rgba8(139, 92, 246, 255);
    /// `#7c3aed`
    pub const S600: Color = Color::from_rgba8(124, 58, 237, 255);
    /// `#6d28d9`
    pub const S700: Color = Color::from_rgba8(109, 40, 217, 255);
    /// `#5b21b6`
    pub const S800: Color = Color::from_rgba8(91, 33, 182, 255);
    /// `#4c1d95`
    pub const S900: Color = Color::from_rgba8(76, 29, 149, 255);
    /// `#2e1065`
    pub const S950: Color = Color::from_rgba8(46, 16, 101, 255);
}

/// The Tailwind `purple` scale.
pub mod purple {
    use super::Color;
    /// `#faf5ff`
    pub const S50: Color = Color::from_rgba8(250, 245, 255, 255);
    /// `#f3e8ff`
    pub const S100: Color = Color::from_rgba8(243, 232, 255, 255);
    /// `#e9d5ff`
    pub const S200: Color = Color::from_rgba8(233, 213, 255, 255);
    /// `#d8b4fe`
    pub const S300: Color = Color::from_rgba8(216, 180, 254, 255);
    /// `#c084fc`
    pub const S400: Color = Color::from_rgba8(192, 132, 252, 255);
    /// `#a855f7`
    pub const S500: Color = Color::from_rgba8(168, 85, 247, 255);
    /// `#9333ea`
    pub const S600: Color = Color::from_rgba8(147, 51, 234, 255);
    /// `#7e22ce`
    pub const S700: Color = Color::from_rgba8(126, 34, 206, 255);
    /// `#6b21a8`
    pub const S800: Color = Color::from_rgba8(107, 33, 168, 255);
    /// `#581c87`
    pub const S900: Color = Color::from_rgba8(88, 28, 135, 255);
    /// `#3b0764`
    pub const S950: Color = Color::from_rgba8(59, 7, 100, 255);
}

/// The Tailwind `fuchsia` scale.
pub mod fuchsia {
    use super::Color;
    /// `#fdf4ff`
    pub const S50: Color = Color::from_rgba8(253, 244, 255, 255);
    /// `#fae8ff`
    pub const S100: Color = Color::from_rgba8(250, 232, 255, 255);
    /// `#f5d0fe`
    pub const S200: Color = Color::from_rgba8(245, 208, 254, 255);
    /// `#f0abfc`
    pub const S300: Color = Color::from_rgba8(240, 171, 252, 255);
    /// `#e879f9`
    pub const S400: Color = Color::from_rgba8(232, 121, 249, 255);
    /// `#d946ef`
    pub const S500: Color = Color::from_rgba8(217, 70, 239, 255);
    /// `#c026d3`
    pub const S600: Color = Color::from_rgba8(192, 38, 211, 255);
    /// `#a21caf`
    pub const S700: Color = Color::from_rgba8(162, 28, 175, 255);
    /// `#86198f`
    pub const S800: Color = Color::from_rgba8(134, 25, 143, 255);
    /// `#701a75`
    pub const S900: Color = Color::from_rgba8(112, 26, 117, 255);
    /// `#4a044e`
    pub const S950: Color = Color::from_rgba8(74, 4, 78, 255);
}

/// The Tailwind `pink` scale.
pub mod pink {
    use super::Color;
    /// `#fdf2f8`
    pub const S50: Color = Color::from_rgba8(253, 242, 248, 255);
    /// `#fce7f3`
    pub const S100: Color = Color::from_rgba8(252, 231, 243, 255);
    /// `#fbcfe8`
    pub const S200: Color = Color::from_rgba8(251, 207, 232, 255);
    /// `#f9a8d4`
    pub const S300: Color = Color::from_rgba8(249, 168, 212, 255);
    /// `#f472b6`
    pub const S400: Color = Color::from_rgba8(244, 114, 182, 255);
    /// `#ec4899`
    pub const S500: Color = Color::from_rgba8(236, 72, 153, 255);
    /// `#db2777`
    pub const S600: Color = Color::from_rgba8(219, 39, 119, 255);
    /// `#be185d`
    pub const S700: Color = Color::from_rgba8(190, 24, 93, 255);
    /// `#9d174d`
    pub const S800: Color = Color::from_rgba8(157, 23, 77, 255);
    /// `#831843`
    pub const S900: Color = Color::from_rgba8(131, 24, 67, 255);
    /// `#500724`
    pub const S950: Color = Color::from_rgba8(80, 7, 36, 255);
}

/// The Tailwind `rose` scale.
pub mod rose {
    use super::Color;
    /// `#fff1f2`
    pub const S50: Color = Color::from_rgba8(255, 241, 242, 255);
    /// `#ffe4e6`
    pub const S100: Color = Color::from_rgba8(255, 228, 230, 255);
    /// `#fecdd3`
    pub const S200: Color = Color::from_rgba8(254, 205, 211, 255);
    /// `#fda4af`
    pub const S300: Color = Color::from_rgba8(253, 164, 175, 255);
    /// `#fb7185`
    pub const S400: Color = Color::from_rgba8(251, 113, 133, 255);
    /// `#f43f5e`
    pub const S500: Color = Color::from_rgba8(244, 63, 94, 255);
    /// `#e11d48`
    pub const S600: Color = Color::from_rgba8(225, 29, 72, 255);
    /// `#be123c`
    pub const S700: Color = Color::from_rgba8(190, 18, 60, 255);
    /// `#9f1239`
    pub const S800: Color = Color::from_rgba8(159, 18, 57, 255);
    /// `#881337`
    pub const S900: Color = Color::from_rgba8(136, 19, 55, 255);
    /// `#4c0519`
    pub const S950: Color = Color::from_rgba8(76, 5, 25, 255);
}

// ---- convenience aliases (the 500 shade) ----
pub const RED: Color = red::S500;
pub const ORANGE: Color = orange::S500;
pub const AMBER: Color = amber::S500;
pub const YELLOW: Color = yellow::S500;
pub const LIME: Color = lime::S500;
pub const GREEN: Color = green::S500;
pub const EMERALD: Color = emerald::S500;
pub const TEAL: Color = teal::S500;
pub const CYAN: Color = cyan::S500;
pub const SKY: Color = sky::S500;
pub const BLUE: Color = blue::S500;
pub const INDIGO: Color = indigo::S500;
pub const VIOLET: Color = violet::S500;
pub const PURPLE: Color = purple::S500;
pub const FUCHSIA: Color = fuchsia::S500;
pub const PINK: Color = pink::S500;
pub const ROSE: Color = rose::S500;

