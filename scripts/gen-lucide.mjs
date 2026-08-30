#!/usr/bin/env node
// Generates crates/pebbles-icons/src/lucide.rs from Lucide's machine-readable
// icon-nodes.json. Each icon becomes a `pub const NAME: IconData` plus an entry
// in the sorted `ALL` table used by `by_name`.
//
//   node scripts/gen-lucide.mjs [path/to/icon-nodes.json]
//
// Source data: https://unpkg.com/lucide-static@latest/icon-nodes.json
// (Lucide is ISC-licensed; see NOTICE.)

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const srcPath = process.argv[2] ?? "/tmp/lucide-nodes.json";
const outPath = resolve(here, "../crates/pebbles-icons/src/lucide.rs");

const nodes = JSON.parse(readFileSync(srcPath, "utf8"));

/** Format a number as a Rust f64 literal (always with a decimal point). */
function f(v) {
  const n = typeof v === "number" ? v : parseFloat(v);
  if (!Number.isFinite(n)) return "0.0";
  return Number.isInteger(n) ? `${n}.0` : `${n}`;
}

/** Parse an SVG points string ("1 2 3,4") into [[1,2],[3,4]]. */
function points(s) {
  const nums = String(s).trim().split(/[\s,]+/).map(Number);
  const out = [];
  for (let i = 0; i + 1 < nums.length; i += 2) out.push([nums[i], nums[i + 1]]);
  return out;
}

function ptsLit(pairs) {
  return "&[" + pairs.map(([x, y]) => `(${f(x)},${f(y)})`).join(",") + "]";
}

/** Convert one Lucide [tag, attrs] node into an `IconPrim::…` literal. */
function prim(tag, a) {
  switch (tag) {
    case "path":
      return `IconPrim::Path(${JSON.stringify(a.d)})`;
    case "line":
      return `IconPrim::Line(${f(a.x1)},${f(a.y1)},${f(a.x2)},${f(a.y2)})`;
    case "polyline":
      return `IconPrim::Polyline(${ptsLit(points(a.points))})`;
    case "polygon":
      return `IconPrim::Polygon(${ptsLit(points(a.points))})`;
    case "circle":
      return `IconPrim::Circle(${f(a.cx)},${f(a.cy)},${f(a.r)})`;
    case "ellipse":
      return `IconPrim::Ellipse(${f(a.cx)},${f(a.cy)},${f(a.rx)},${f(a.ry)})`;
    case "rect": {
      const rx = a.rx ?? a.ry ?? 0;
      const ry = a.ry ?? a.rx ?? 0;
      return `IconPrim::Rect(${f(a.x ?? 0)},${f(a.y ?? 0)},${f(a.width)},${f(a.height)},${f(rx)},${f(ry)})`;
    }
    default:
      throw new Error(`unhandled svg tag: ${tag}`);
  }
}

/** kebab-case icon name → Rust SCREAMING_SNAKE const identifier. */
function constName(name) {
  let id = name.toUpperCase().replace(/[^A-Z0-9]+/g, "_");
  if (/^[0-9]/.test(id)) id = "_" + id;
  return id;
}

const names = Object.keys(nodes).sort();
const seen = new Map();
const consts = [];
const table = [];

for (const name of names) {
  let id = constName(name);
  // De-collide any names that map to the same identifier.
  if (seen.has(id)) {
    const n = seen.get(id) + 1;
    seen.set(id, n);
    id = `${id}_${n}`;
  } else {
    seen.set(id, 0);
  }
  const prims = nodes[name].map(([tag, attrs]) => prim(tag, attrs)).join(", ");
  consts.push(
    `/// Lucide \`${name}\`.\npub const ${id}: IconData = IconData::stroked(24.0, &[${prims}]);`,
  );
  table.push(`    (${JSON.stringify(name)}, ${id}),`);
}

const out = `//! The bundled Lucide icon set — GENERATED, do not edit by hand.
//!
//! Regenerate with \`node scripts/gen-lucide.mjs\`. Source: Lucide's
//! \`icon-nodes.json\` (ISC-licensed; see NOTICE). ${names.length} icons.

use super::{IconData, IconPrim};

${consts.join("\n\n")}

/// Every bundled icon, keyed by its kebab-case Lucide name, sorted for the
/// binary search in [\`by_name\`].
pub static ALL: &[(&str, IconData)] = &[
${table.join("\n")}
];

/// Look up a bundled Lucide icon by its kebab-case name, e.g. \`"circle-check"\`.
pub fn by_name(name: &str) -> Option<IconData> {
    ALL.binary_search_by(|(k, _)| k.cmp(&name)).ok().map(|i| ALL[i].1)
}
`;

writeFileSync(outPath, out);
console.log(`wrote ${outPath}: ${names.length} icons, ${out.length} bytes`);
