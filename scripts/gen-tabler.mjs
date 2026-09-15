#!/usr/bin/env node
// Generates crates/pebbles-icons/src/tabler.rs from Tabler's machine-readable
// node JSON (outline + filled). Each icon becomes a `pub const NAME: IconData`;
// outline icons live at the module root and filled ones under `tabler::filled`.
// Every Tabler node is an SVG `<path>`, so each icon is one or more
// `IconPrim::Path` — no other primitive kinds appear.
//
//   node scripts/gen-tabler.mjs [outline-nodes.json] [filled-nodes.json]
//
// Source data: the `@tabler/icons` npm package (`tabler-nodes-outline.json` and
// `tabler-nodes-filled.json`). Tabler is MIT-licensed; see NOTICE.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const outlineSrc = process.argv[2] ?? "/tmp/tabler-nodes-outline.json";
const filledSrc = process.argv[3] ?? "/tmp/tabler-nodes-filled.json";
const outPath = resolve(here, "../crates/pebbles-icons/src/tabler.rs");

const outline = JSON.parse(readFileSync(outlineSrc, "utf8"));
const filled = JSON.parse(readFileSync(filledSrc, "utf8"));

/** kebab-case icon name → Rust SCREAMING_SNAKE const identifier. */
function constName(name) {
  let id = name.toUpperCase().replace(/[^A-Z0-9]+/g, "_");
  if (/^[0-9]/.test(id)) id = "_" + id;
  return id;
}

/** Build the `pub const …` block + `ALL` table rows for one variant. */
function buildVariant(nodes, ctor, label) {
  const names = Object.keys(nodes).sort();
  const seen = new Map();
  const consts = [];
  const table = [];
  for (const name of names) {
    let id = constName(name);
    if (seen.has(id)) {
      const n = seen.get(id) + 1;
      seen.set(id, n);
      id = `${id}_${n}`;
    } else {
      seen.set(id, 0);
    }
    // Every Tabler node is a path; the transparent bounding-box frame is already
    // stripped from the node JSON.
    const prims = nodes[name].map(([, a]) => `IconPrim::Path(${JSON.stringify(a.d)})`).join(", ");
    consts.push(`/// Tabler \`${name}\` (${label}).\npub const ${id}: IconData = IconData::${ctor}(24.0, &[${prims}]);`);
    table.push(`    (${JSON.stringify(name)}, ${id}),`);
  }
  return { consts, table, count: names.length };
}

const out = buildVariant(outline, "stroked", "outline");
const fill = buildVariant(filled, "filled", "filled");

const src = `//! The bundled Tabler icon set — GENERATED, do not edit by hand.
//!
//! Regenerate with \`node scripts/gen-tabler.mjs\`. Source: the \`@tabler/icons\`
//! package's \`tabler-nodes-{outline,filled}.json\` (MIT-licensed; see NOTICE).
//! ${out.count} outline icons + ${fill.count} filled ([\`filled\`]).
//!
//! Outline icons (the default look, stroke width 2) live at the module root
//! (\`tabler::HOME\`); the solid variants live under [\`filled\`] (\`tabler::filled::HOME\`).

use super::{IconData, IconPrim};

${out.consts.join("\n\n")}

/// Every bundled **outline** icon, keyed by its kebab-case Tabler name, sorted for
/// the binary search in [\`by_name\`].
pub static ALL: &[(&str, IconData)] = &[
${out.table.join("\n")}
];

/// Look up a bundled outline Tabler icon by its kebab-case name, e.g. \`"circle-check"\`.
pub fn by_name(name: &str) -> Option<IconData> {
    ALL.binary_search_by(|(k, _)| k.cmp(&name)).ok().map(|i| ALL[i].1)
}

/// The **filled** (solid) Tabler variants — the second half of what makes Tabler more
/// complete than a stroke-only set. Same names as the outline icons where a filled
/// version exists (\`tabler::filled::HOME\`), plus filled-only glyphs.
pub mod filled {
    use super::super::{IconData, IconPrim};

${fill.consts.map((c) => "    " + c.replace(/\n/g, "\n    ")).join("\n\n")}

    /// Every bundled **filled** icon, keyed by its kebab-case Tabler name.
    pub static ALL: &[(&str, IconData)] = &[
${fill.table.map((r) => "    " + r).join("\n")}
    ];

    /// Look up a bundled filled Tabler icon by its kebab-case name.
    pub fn by_name(name: &str) -> Option<IconData> {
        ALL.binary_search_by(|(k, _)| k.cmp(&name)).ok().map(|i| ALL[i].1)
    }
}
`;

writeFileSync(outPath, src);
console.log(`wrote ${outPath}: ${out.count} outline + ${fill.count} filled, ${src.length} bytes`);
