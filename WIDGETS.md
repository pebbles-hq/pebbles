# Pebbles widget status

Legend: **✅ Refined** = Flutter-quality, interactive/polished · **🟡 Functional** =
works and themed, but pending interaction/polish (hover/press, drag, etc.) ·
**⬜ Missing** = not built yet.

Run `cargo run -p gallery` to see everything currently built, in labeled sections.

## Layout & primitives

These are structural (no per-widget interaction needed); all are done.

| Widget | Backs | Status |
|--------|-------|--------|
| `Container` | composite | ✅ |
| `Row` / `Column` | `RenderFlex` | ✅ |
| `Expanded` / `Flexible` / `spacer` | flex parent-data | ✅ |
| `Stack` / `Positioned` | `RenderStack` | ✅ |
| `Wrap` | `RenderWrap` | ✅ |
| `AspectRatio` | `RenderAspectRatio` | ✅ |
| `Padding` | `RenderPadding` | ✅ |
| `Align` / `center` | `RenderAlign` | ✅ |
| `SizedBox` / `ConstrainedBox` | `RenderConstrainedBox` | ✅ |
| `ColoredBox` / `DecoratedBox` | `RenderColoredBox` / `RenderDecoratedBox` | ✅ |
| `Opacity` / `ClipRRect` | `RenderOpacity` / `RenderClipRRect` | ✅ |
| `SingleChildScrollView` / `list_view` | `RenderScroll` | ✅ |
| `Text` | `RenderParagraph` (parley) | ✅ |
| `Icon` | `RenderIcon` | ✅ |
| `GestureDetector` | `RenderPointerListener` | ✅ tap/double/secondary/down/up/hover/cursor |

## Components (shadcn-style)

| Widget | Status | Interactivity | Notes |
|--------|--------|---------------|-------|
| `Button` | ✅ Refined | hover · press · disabled · pointer cursor | overrides: color/text/radius/padding/full_width |
| `IconButton` | ✅ Refined | hover · press · cursor | |
| typography (`heading`/`title`/`body`/`label`/`muted`) | ✅ | — (static) | |
| `Card` | ✅ | — | border + shadow |
| `Badge` | ✅ | — | 5 variants |
| `Alert` | ✅ | — | 4 variants |
| `Avatar` | ✅ | — | initials |
| `Separator` | ✅ | — | |
| `Skeleton` | ✅ | — | |
| `Progress` | ✅ | — | determinate |
| `Panel` | ✅ | — | docking surface |
| `Checkbox` | 🟡 | controlled | needs hover/press |
| `Switch` | 🟡 | controlled | needs hover/press |
| `Radio` | 🟡 | controlled | needs hover/press |
| `Toggle` | 🟡 | controlled | needs hover/press |
| `Slider` | 🟡 | display only | needs drag (pointer-position callbacks) |
| `Tabs` | 🟡 | controlled | needs tab hover |
| `Accordion` / `Collapsible` | 🟡 | controlled | needs header hover |
| `Breadcrumb` | 🟡 | — | needs link hover |
| `Toolbar` / `StatusBar` | 🟡 | — | |
| `Pagination` | 🟡 | controlled | |
| `ListTile` | 🟡 | — | needs row hover |
| `Table` | 🟡 | — | needs row hover/sort |
| `TreeView` / `TreeNode` | 🟡 | controlled | needs row hover |
| `SplitView` | 🟡 | controlled ratio | needs live drag-resize |

## Missing — planned

| Category | Widgets | Blocker |
|----------|---------|---------|
| **Forms** | `TextField`, `Input`, `Textarea`, `NumberInput` | focus system + keyboard/IME routing + caret |
| **Overlays** | `Dialog`, `DropdownMenu`, `Select`, `Combobox`, `Popover`, `Tooltip`, `ContextMenu`, `Toast`, `CommandPalette`, `Sheet`/`Drawer` | overlay layer + anchor positioning + dismiss |
| **Menus** | `MenuBar`, `Menu`, `MenuItem` | overlay layer |
| **Layout** | `GridView`, `FractionallySizedBox`, `FittedBox`, `IntrinsicWidth`/`Height`, `Baseline`, `Table` (grid) | render objects |
| **Data** | `DataTable` (sort/select), `Calendar`, `DatePicker` | — |
| **Feedback** | `CircularProgress`/`Spinner`, `Chip` (deletable) | arc render + animation |
| **Motion** | `AnimatedContainer`, transitions, ripple | animation ticker subsystem |
| **Infra** | `InheritedWidget` → `ThemeProvider`, `FocusScope` | engine additions |

## Refinement pass order (interactivity)

Applying the Button hover/press pattern, widget by widget:
`Checkbox` → `Switch` → `Radio` → `Toggle` → `Tabs` → `ListTile`/`TreeView` rows →
`Accordion` headers → `Breadcrumb` links. Then forms, then overlays, then motion.
