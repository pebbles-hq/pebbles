# Pebbles widget status

> The **live widget + styling inventory**. Rebuilt 2026-09-01 — every row verified against
> the code (including today's uncommitted timer-wake work: `animation::next_deadline` +
> shell `WaitUntil`, so tooltip/hover-card delays fire even with a still mouse).
> Companions (sibling `../documentations/` folder, not git-tracked):
> `widget-catalog-plan.md` — the shadcn-parity build record · `p2-roadmap.md` — the
> execution tracker; the IDs below (A1, C2, G5, …) point at its items ·
> `performance-standards.md` — BINDING memory/lifecycle standards (+ the 2026-09-01
> leak audit → p2 **E6**) · `desktop-catalog-plan.md` — the rich desktop tier
> (WinForms/Avalonia/Qt-class components), mirrored in §11 below.
> This file supersedes p2 item **I1** ("delete WIDGETS.md"): it is now the one status surface.

Legend: **✅ built** (themed, interactive, tested) · **🔶 built, tracked remainder** (→ p2 ID) ·
**⬜ open, tracked** (→ p2 ID) · **🆕 open, NOT yet tracked** — promote to a p2 item when
ratified · **🚫 decided OUT** (p2 §J — do not "helpfully" add).

Run `cargo run -p gallery` — ~50 dedicated screens, one per component family.

---

## 1. Layout & primitives (`crates/pebbles-widgets/src/widgets/`)

| Widget | Status | Notes |
|--------|--------|-------|
| `Container` | ✅ | full BoxDecoration + `foreground_decoration` |
| `Row` / `Column` (+ `Expanded`, `Flexible`, `spacer`, `gap_h`/`gap_w`) | ✅ 🔶G1 | `Flexible.fit` ✅; `vertical_direction` + real baseline alignment (G1) open |
| `Stack` / `Positioned` | ✅ 🔶G2 | `.fit(StackFit)` to replace `.expand()` (G2) open |
| `Wrap` | ✅ 🔶G3 | `alignment`/`run_alignment` (G3) open |
| `AspectRatio` · `Padding` · `Align`/`center` · `SizedBox` · `ConstrainedBox` | ✅ | |
| `ColoredBox` / `DecoratedBox` | ✅ | |
| `Opacity` · `ClipRRect` | ✅ | |
| `Transform` (`rotate`/`scale`/`translate`, also via `Style`) | ✅ | |
| `Text` | ✅ | ellipsis ✅; `soft_wrap(false)` ✅ |
| `EditableText` / `editable` | ✅ | low-level editor under TextField |
| `Icon` (lucide set) | ✅ | |
| `Spinner` | ✅ | indeterminate arc |
| `GestureDetector` | ✅ 🔶G7 | tap/double/secondary/down/up/hover/cursor/pan; axis-drag trios (G7) open |
| `Semantics` (+ `SemanticsRole`) | ✅ 🔶C7 | long-tail roles (Menu/Tab/Dialog/…) open (C7) |
| `SingleChildScrollView` | ✅ 🔶A4 | wheel/scrollbar/keys/snap/spring; drag-fling/overscroll/physics knobs (A4) open |
| `ListView::builder` (fixed extent) | ✅ | virtualized, `.horizontal/.reverse/.padding/.scrollbar/.controller` |
| `ListView::variable` (per-item extents) | ✅ 🔶A1 | caller-supplied extents; auto-MEASURED mode (`builder_auto`) is A1's remainder |
| `ListView::separated` | ✅ | Flutter's `ListView.separated`; virtualized separators |
| `GridView::builder` | ✅ | columns, `row_extent`, `.spans((col,row))`, `.spacing`, `.aspect_ratio`, `.max_extent`, reverse/padding |
| `ScrollController` / `use_scroll_controller` | ✅ | `offset/jump_to/animate_to/scroll_to_index` |
| `FittedBox` · `FractionallySizedBox` | ✅ | BoxFit scale-to-fit · fraction-of-constraints sizing |
| `IntrinsicWidth` · `IntrinsicHeight` | ✅ | shrink-wrap to child's intrinsic extent |
| `LimitedBox` · `OverflowBox` | ✅ | unbounded-axis cap · unclipped overflow |
| `AnimatedContainer` | ✅ | implicit tween on width/height/color/radius/padding/margin/opacity |
| `FocusScope` (focus trap) | ✅ | scoped Tab-cycling; dialogs/sheets contain focus |
| `View` | ✅ | window root background |

## 2. Input components (`components/input/`)

| Widget | Status | Notes |
|--------|--------|-------|
| `Button` / `IconButton` | ✅ | variants, sizes, hover/press/disabled/focus ring |
| `ButtonGroup` | ✅ | joined strip geometry |
| `Checkbox` | ✅ | + `.indeterminate` tri-state |
| `Radio` / `RadioGroup` | ✅ | |
| `Switch` · `Toggle` | ✅ | |
| `ToggleGroup` | ✅ | single/multi, joined segmented strip (C4 ✅) |
| `Slider` | ✅ | drag + keyboard |
| `TextField` / `text_area` | ✅ | 10 `InputKind`s (Text/Number/Integer/Decimal/Email/Url/Phone/Currency/Password/Search), bind/filter/format/max_length/obscured/leading/trailing/label/helper/error, IME preedit |
| `InputOtp` | ✅ | groups, paste, `on_complete` |
| `Field` | ✅ | generic label/description/error wrapper |
| `DateField` | ✅ | single + range (B6 ✅), formats, clearable, min/max/disabled dates |
| `TimeField` | ✅ | |
| `Calendar` | ✅ | range, bounds, disabled dates, caption layouts |
| `Select` | ✅ | groups, disabled options, clearable, keyboard nav |
| `Combobox` / `MultiSelect` | ✅ | filtered, keyboard nav |
| `Command` / `command_palette` | ✅ | groups, filter, keyboard; global Ctrl+K binding → B2 |
| `DropdownMenu` | ✅ | items/checks/labels/separators + `menu_sub` submenus (B5 ✅) |
| `ContextMenu` | ✅ | opens at cursor; shares menu machinery |
| `Popover` | ✅ | anchor + flip + scroll-follow |
| `ListNav` | ✅ | reusable list-keyboard-nav helper (SI-4) |

## 3. Display components (`components/display/`)

| Widget | Status | Notes |
|--------|--------|-------|
| typography (`heading`/`title`/`subtitle`/`body`/`label`/`muted`) | ✅ | full spec screen in gallery |
| `Card` · `Badge` (5 variants) · `Alert` (4 variants) | ✅ | |
| `Chip` | ✅ | deletable tag: icon + label + ✕ `on_deleted`, disabled |
| `Avatar` / `AvatarGroup` | ✅ | shapes; initials |
| `Separator` · `Kbd` · `Empty` | ✅ | |
| `Skeleton` | ✅ | + shimmer |
| `Progress` | ✅ | determinate bar; indeterminate = `Spinner` |
| `Tooltip` | ✅ 🔶C2 | delay/rich/style ✅; `.side()` + flip + show-on-focus (C2) open |
| `HoverCard` | ✅ | delay + stays-open-over-card grace |
| `Table` (data table) | ✅ | sort (multi, configurable glyph), selection + select-all, striped, hover, empty state |
| `ListTile` | ✅ | style/tap/selected/dense |
| `TreeView` / `TreeNode` | ✅ | multi-select, drag |
| `FileExplorer` / `FileTree` (+ `pick_folder`) | ✅ | real disk: native picker, lazy loading, mutations, multi-select/-drag, empty-space menu |
| `ImageView` | ✅ | `asset`/`network`/`memory`/`base64`/`image` + `ImageFit`; reactive source |

## 4. Layout & navigation components (`components/layout/`, `components/navigation/`)

| Widget | Status | Notes |
|--------|--------|-------|
| `Panel` · `ScrollArea` · `Resizable` · `SplitView` | ✅ | live drag-resize |
| `Accordion` · `Collapsible` | ✅ | single/multiple, default-open, events |
| `Scaffold` · `TopPanel` · `SideNav` · `BottomNav` | ✅ 🔶C5/C8g | SideNav rail-collapse (C5) open; dedicated chrome gallery screen (C8g) open |
| `Tabs` | ✅ | design variants, disabled tabs, keyboard, focus ring |
| `Breadcrumb` | ✅ | `.max_visible` middle-collapse |
| `Pagination` | ✅ | Numbers/Simple/Arrows designs, unified `on_page` |
| `Menubar` / `menubar_menu` | ✅ | hover-switch between open menus, submenus |
| `Toolbar` · `StatusBar` | ✅ 🔶C8f | work; dedicated gallery screen (C8f) open |
| `NavStack` / `RouteView` | ✅ 🔶C8h | push/replace/pop, fallback; dedicated gallery screen (C8h) open |

## 5. App services (`pebbles-widgets/src/*.rs`)

| Service | Status | Notes |
|---------|--------|-------|
| `dialog` / `alert_dialog` | ✅ | per-window modal; alert preset non-dismissible by default |
| `sheet` (Drawer) | ✅ 🔶C3 | `Side::{Left,Right,Top,Bottom}`; slide/scrim motion (C3) open |
| `toast` | ✅ 🔶C1 | variants/duration/action/dismiss, max-3 stack; hover-pause + motion (C1) open |
| overlay (`show_overlay`/`show_passive`/`OverlayHost`) | ✅ 🔶C6 | anchor+flip+scroll-follow; secondary-window scroll-follow (C6) open |
| global context menu | ✅ | disabled-by-default fallback, per-widget opt-in/out, styleable |
| windows (multi-window) | ✅ | `window()` + close/focus/minimize/maximize/position/resizable/title; `monitors()` (F5) open |
| fonts | ✅ 🔶F4 | 4 bundled builtins + host discovery (`families`/`has`); user-supplied bytes `App::font` (F4) open |

## 6. Styling & theming

| Surface | Status | Notes |
|---------|--------|-------|
| `Style` builder (`style()`, `styled`, `styles`, `.merge`) | ✅ | box: background/gradient/image/border (+ per-side)/radius/shape/shadow/opacity/blend/padding/margin/size/min-max/aspect_ratio/align/cursor · text: color/family/size/weight/bold/semibold/italic/underline/strikethrough/letter_spacing/line_height/text_align/max_lines/ellipsis · transform: rotate/scale/translate · presets: card/heading/circle |
| `BoxDecoration` | ✅ | `Gradient::{linear,vertical,horizontal,radial}`, `Border` per-side, `BorderRadius`, `BoxShadow` (offset/blur/spread), `BoxShape`, blend, image fill + `ImageFit` |
| Theme | ✅ | `Theme::light()/dark()`, `set_theme`/`toggle_theme`, `Colors` tokens, `shade`/`mix`; reactive |
| `palette` | ✅ | foundation color set |
| Sweep gradient | ✅ | `Gradient::sweep` / `sweep_arc` (conic fills) |
| Scoped theme override (`theme_override`) | ✅ | per-subtree `Theme` swap; global stays the default |
| `blur`/`backdrop` in Style · per-state styles (`.hover()`) · `text_transform`/cascade | 🚫 | p2 §J rulings |

## 7. Core capabilities backing the widgets (`pebbles-core`, `pebbles-shell`)

| Capability | Status | Notes |
|-----------|--------|-------|
| Reactivity (signals, memo, `Store`, resource, `spawn`) | ✅ | `select_memo` convenience (E4) open; tokio feature (F3) open |
| Focus (Tab traversal, activate, editor routing, focus ring) | ✅ | scoped `FocusScope`/focus-trap primitive ✅ |
| Keyboard | ✅ 🔶B1 | Shift/Ctrl only — Alt/Meta (B1), shortcut map (B2), native menubar (B3), global hotkeys (B4) open |
| Animation (`animated`, `animate_to`, loops, keyed timeouts, timer-wake) | ✅ 🔶H1 | curves/springs/`transition()` presence (H1) open |
| Clipboard · IME preedit | ✅ | |
| Accessibility (AccessKit: read + focus announce) | ✅ 🔶D1/C7 | AT-driven actions (D1), long-tail roles (C7) open |
| RTL / `TextDirection` threading | ⬜D2 | enum exists, unused |
| Custom drawing (`canvas`) | ⬜H2 | prerequisite for Charts (H3) |
| Lifecycle & memory hygiene | ✅ | teardown verified 2026-09-01 (screens fully unmount; zero frames/timers after navigation — `performance-standards.md` §0-1). E6 ✅ DONE 2026-09-02: `text_edit` leak fixed, `scroll_metrics` audited clean, census accessors + navigation-soak tripwire (`gallery/src/soak.rs`) green |

## 8. Open items — add-to-track (🆕 not in the p2 roadmap yet)

All 2026-09-01 untracked gaps have landed: `FittedBox` · `FractionallySizedBox` ·
`IntrinsicWidth`/`IntrinsicHeight` · `LimitedBox` · `OverflowBox` · `Chip` ·
`AnimatedContainer` · scoped theme override (`theme_override`) · `FocusScope` focus
trap · sweep gradient — each now tracked in its §1/§3/§6 row above with a gallery
sample and a headless test. Nothing remains untracked.

Baseline-as-a-widget rides **G1** (real baseline alignment); `CustomPaint` IS **H2** (`canvas`).

## 9. Open items — already tracked in `p2-roadmap.md`

Widget-visible only (infra/perf/tooling live in the roadmap): **A3** sticky/collapsing
headers · **A4** drag-fling/overscroll/physics · **A5** pull-to-refresh · **A6** Carousel ·
**B1–B4** keyboard tier · **C1** toast motion · **C2** tooltip sides · **C3** sheet slide ·
**C5** rail-collapse · **C6** secondary-window scroll-follow · **C7** semantics roles ·
**C8f/g/h** Toolbar/chrome/routing gallery screens · **D1** AT actions · **D2** RTL ·
**F4** `App::font` · **F5** monitors · **G1–G5, G7, G8** property parity · **H1** animation v2 ·
**H2** canvas · **H3** chart plan. (A1 remainder: auto-measured list extents. A2, B5, B6, C4 ✅ done. **E6 ✅ DONE 2026-09-02.**)

## 10. Decided OUT — do not re-add (p2 §J + desktop-catalog §5)

NavigationMenu (Menubar covers) · InputGroup (TextField leading/trailing) · Material ripple ·
`blur`/`backdrop` in Style · per-state styles · form-validation framework (parked) · full
Flutter sliver protocol · infinite carousel (v2) · wasm/web target · hot reload ·
`text_transform`/style cascade/`overflow`/`z_index` in Style · monitor hot-plug events ·
Ribbon · MDI · WebView · printing (deferred) · terminal emulator (deferred, plan-first) ·
media playback · auto-updater · OS jump lists · gauges (ride H3).

## 11. Desktop-rich suite — PROPOSED, tracked in `desktop-catalog-plan.md`

The beyond-shadcn/Flutter tier (WinForms/Avalonia/Qt/VB-class goodies) so developers
don't build desktop staples from scratch. Full specs, sizes, RATIFY flags, and build
order live in the plan file; at-a-glance status here (all ⬜ until ratified):

| Tier | Items |
|------|-------|
| **D1 essentials** | `pick_file`/`pick_files`/`save_file` (rfd already in-tree) · `number_input` (NumericUpDown) · `group_box` · `form_layout` · `task_dialog` · `busy` overlay · StatusBar panes · Toolbar overflow · `wizard` · `color_picker`/`color_field` · `font_picker` · `hotkey_field` (after B2) · `tag_input` (after Chip §8) · `rating` · title-bar kit (custom chrome) · OS file drag-in · tray icon (dep) · native notifications (dep) · single-instance guard · window extras (always-on-top/attention/fullscreen/taskbar-progress) · clipboard image+HTML |
| **D2 data-heavy** | editable DataGrid program (cell editors → column ops → grouping) · TreeDataGrid · `property_grid` (Gravel-critical) · `log_view` virtualized console (Gravel-critical) · masked input |
| **D3 IDE-grade, plan-first** | docking manager (`docking-plan.md` before code) · code editor (`editor-plan.md` before code) · `zoom_view` pan/zoom viewport |

Already-covered desktop staples (menubar, tray of dialogs, tree/file explorer, tabs,
splitters, status bar, virtualized list/grid, date/time pickers, …) are mapped in the
plan's §1 so nothing gets re-planned.
