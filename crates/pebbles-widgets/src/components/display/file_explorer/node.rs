//! The per-node row: [`render_node`] (selection, drag/drop, context menu, expansion)
//! and the inline rename editor. Child module of `file_explorer` — reads the
//! controller's private state directly.

#[allow(clippy::wildcard_imports)]
use super::*;
use pebbles_core::{component_memo, create_memo};

/// Props for one tree node.
pub(super) struct NodeProps {
    pub(super) explorer: FileExplorer,
    /// The compact chain the node's row shows (its identity is the LAST node).
    chain: Vec<FsNode>,
    pub(super) depth: usize,
    /// Filtering: an ancestor's name already matched (this subtree stays whole).
    pub(super) anc_match: bool,
    filtering: bool,
}

/// The tree node for `node`: a component KEYED by its row identity (the compact
/// chain's ids and whether a filter is on), so the per-node memos below always describe
/// the node they were made for — positional reuse would hand a shifted row another
/// node's memos.
pub(super) fn node_widget(explorer: FileExplorer, node: &FsNode, depth: usize, anc_match: bool, q: &str) -> AnyWidget {
    use std::hash::{Hash, Hasher};
    let filtering = !q.is_empty();
    // Compact folders: a single-child folder chain renders as one row whose
    // identity is its LAST folder, labelled `a/b/c`.
    let chain = if filtering { vec![node.clone()] } else { explorer.compact_chain(node) };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for c in &chain {
        c.id.hash(&mut h);
    }
    filtering.hash(&mut h);
    crate::widgets::keyed(
        h.finish(),
        component_memo(render_node, NodeProps { explorer, chain, depth, anc_match, filtering }),
    )
    .into_widget()
}

/// Same node, same look: everything a node or row renders from its props (signals are
/// read live). Deep for children — an unchanged subtree skips re-rendering entirely
/// when the tree above it re-renders (a load elsewhere, a reveal).
fn same_node(a: &FsNode, b: &FsNode) -> bool {
    a.id == b.id
        && a.name == b.name
        && a.kind == b.kind
        && a.loaded == b.loaded
        && a.icon == b.icon
        && a.color == b.color
        && a.children.len() == b.children.len()
        && a.children.iter().zip(&b.children).all(|(x, y)| same_node(x, y))
}

fn same_chain(a: &[FsNode], b: &[FsNode]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_node(x, y))
}

impl PartialEq for NodeProps {
    fn eq(&self, o: &Self) -> bool {
        self.explorer.tree == o.explorer.tree
            && self.depth == o.depth
            && self.anc_match == o.anc_match
            && self.filtering == o.filtering
            && same_chain(&self.chain, &o.chain)
    }
}

impl PartialEq for RowProps {
    fn eq(&self, o: &Self) -> bool {
        self.explorer.tree == o.explorer.tree
            && self.depth == o.depth
            && self.filtering == o.filtering
            && same_chain(&self.chain, &o.chain)
    }
}

/// One tree node: its row ([`render_row`], its own component) and, when expanded, its
/// children. Only structure is read here (expansion, renaming — through memos, the
/// filter text), so a row's hover, selection or focus ring re-renders that ROW, not the
/// folder's whole subtree (a hovered folder re-rendered every visible descendant on
/// each frame of its hover fade).
pub(super) fn render_node(p: &NodeProps) -> AnyWidget {
    let explorer = p.explorer;
    let filtering = p.filtering;
    let node = p.chain.last().expect("non-empty").clone();
    let is_folder = node.kind == FsKind::Folder;
    let ids: Vec<u64> = p.chain.iter().map(|c| c.id).collect();
    let open = create_memo(move || {
        filtering || {
            let exp = explorer.expanded.get();
            ids.iter().all(|i| exp.contains(i))
        }
    })
    .get();
    let id = node.id;
    let renaming = create_memo(move || explorer.renaming.get() == Some(id)).get();
    let mut kids = vec![
        component_memo(render_row, RowProps { explorer, chain: p.chain.clone(), depth: p.depth, filtering })
            .into_widget(),
    ];
    if is_folder && open && !renaming {
        let q = if filtering { explorer.filter.get().trim().to_lowercase() } else { String::new() };
        let anc_match = p.anc_match || name_matches(&node, &q);
        for child in &node.children {
            if filtering && !filter_keeps(child, &q, anc_match) {
                continue;
            }
            kids.push(node_widget(explorer, child, p.depth + 1, anc_match, &q));
        }
    }
    column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min).into_widget()
}

/// Props for one row.
struct RowProps {
    explorer: FileExplorer,
    /// The compact chain this row shows (its identity is the last node).
    chain: Vec<FsNode>,
    depth: usize,
    filtering: bool,
}

/// One tree row: indent, twistie, glyph, label — with select/expand (Ctrl/Shift
/// multi), inline rename, a context menu, and drag-to-move (a selection drags
/// together; folders highlight as drop targets and expand on hover-hold). Every
/// tree-wide signal is read through a per-row memo: a selection or focus change
/// re-renders only the rows whose look changed.
fn render_row(p: &RowProps) -> AnyWidget {
    let st = p.explorer.style();
    let explorer = p.explorer;
    // The window this row is in (rows dragged out of it start an OS drag there).
    let window = pebbles_core::reactive::current_window();
    let filtering = p.filtering;
    let chain = &p.chain;
    let node = chain.last().expect("non-empty");
    let chain_label = chain.iter().map(|n| n.name.as_str()).collect::<Vec<_>>().join("/");
    let is_folder = node.kind == FsKind::Folder;
    let id = node.id;
    let ids: Vec<u64> = chain.iter().map(|c| c.id).collect();
    // Hooks run unconditionally (a row element may be reused for a file or a folder).
    let open = create_memo(move || {
            filtering || {
                let exp = explorer.expanded.get();
                ids.iter().all(|i| exp.contains(i))
            }
        })
        .get();
    let expanded = is_folder && open;
    let selected = create_memo(move || explorer.selected.get().contains(&id)).get();
    // The FOCUS row gets the ring — driven by the focus signal (Mod+↑/↓ walks
    // it without selecting), falling back to the selection's active end.
    let active =
        create_memo(move || explorer.active.get().or_else(|| explorer.selected.get().last().copied()) == Some(id))
            .get();
    let renaming = create_memo(move || explorer.renaming.get() == Some(id)).get();
    let dragging = create_memo(move || explorer.dragging.get()).get();
    let dragged = dragging && selected;
    let drop_target = create_memo(move || explorer.dragging.get() && explorer.drop_target.get() == Some(id)).get();
    // A cut-pending row renders dimmed (like VSCode) until pasted or cancelled.
    let cut_pending = create_memo(
        move || matches!(explorer.clipboard.get(), Some((ref ids, ClipMode::Cut)) if ids.contains(&id)),
    )
    .get();
    // The selection reads "inactive" while another input (an editor, a field)
    // holds the keyboard — VS Code's list.inactiveSelection. Only selected rows care.
    let elsewhere = create_memo(move || pebbles_core::focus::focus_signal().get().is_some()).get();
    let inactive = selected && elsewhere;
    let hovered = create_signal(false);

    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let (sel_bg, sel_fg) = if inactive {
        (st.selection_inactive_bg, st.selection_inactive_fg)
    } else {
        (st.selection_active_bg, st.selection_active_fg)
    };
    let mut bg = if selected { sel_bg } else { st.row_bg };
    if drop_target {
        bg = mix(bg, st.drop_target_bg, st.drop_target_strength);
    }
    // A full-strength hover is a finished colour (VS Code's list.hoverBackground): the
    // selection paints over it, as `.selected` wins over `:hover` there. A partial one
    // is a tint, and tints the selection too.
    if !(selected && st.hover_strength >= 1.0) {
        bg = mix(bg, st.hover_bg, st.hover_strength * hv as f32);
    }

    let dim = if selected { sel_fg } else { st.icon_tint };
    let twistie: AnyWidget = if is_folder {
        icon(if expanded { IconKind::ChevronDown } else { IconKind::ChevronRight })
            .size(st.twistie_size)
            .color(dim)
            .into_widget()
    } else {
        gap_w(st.twistie_size).into_widget()
    };
    let glyph = explorer.resolved_icon(node, expanded).build(st.icon_size, dim);
    let deco = explorer.decoration(node.id);

    let label: AnyWidget = if renaming {
        component_props(
            render_rename_editor,
            RenameProps { explorer, id: node.id, buf: explorer.rename_buf, placeholder: node.name.clone() },
        )
        .into_widget()
    } else {
        let deco_color = deco.as_ref().and_then(|d| d.label_color(&st));
        let faded = deco.as_ref().is_some_and(|d| d.faded);
        let mut t =
            text(chain_label.clone()).size(st.font_size as f32).color(if dragged || cut_pending || faded {
                st.muted_text
            } else if selected {
                sel_fg
            } else {
                deco_color.unwrap_or(st.text)
            });
        if let Some(f) = &st.font_family {
            t = t.font_family(f.clone());
        }
        t.into_widget()
    };
    // The trailing decoration badge (git letter, problem count, ● dirty, •).
    let badge: AnyWidget = match deco.as_ref().and_then(|d| d.badge_text().map(|b| (b, d.label_color(&st)))) {
        Some((b, c)) if !renaming => text(b)
            .size((st.font_size - 1.5) as f32)
            .color(if selected { sel_fg } else { c.unwrap_or(st.muted_text) })
            .into_widget(),
        _ => gap_w(0.0).into_widget(),
    };

    let content =
        row(children![twistie, gap_w(4.0), glyph, gap_w(6.0), Expanded::new(label), gap_w(4.0), badge])
            .main_axis_size(MainAxisSize::Min);
    // Keep the content (the trailing badge above all) clear of the overlay scrollbar.
    let mut pad = st.row_padding;
    pad.right += scrollbar_gutter();
    let mut inner = Container::new().padding(pad).child(content);
    if let Some(h) = st.row_height {
        inner = inner.height(h).alignment(pebbles_foundation::Alignment::CENTER_LEFT);
    }
    // Indent, with VS Code's indent guides painted behind the row (one
    // centred line per depth level, full row height).
    let row_line = row(children![gap_w(p.depth as f64 * st.indent), Expanded::new(inner)]);
    let line: AnyWidget = match st.indent_guide {
        Some(gc) if p.depth > 0 => {
            let (depth, step) = (p.depth, st.indent);
            let guides = crate::widgets::canvas(move |cv| {
                let h = cv.size().height;
                for d in 0..depth {
                    let x = (d as f64 + 0.5) * step;
                    cv.stroke_line(Offset::new(x, 0.0), Offset::new(x, h), 1.0, gc);
                }
            });
            crate::widgets::stack(children![crate::widgets::Positioned::fill(guides), row_line]).into_widget()
        }
        _ => row_line.into_widget(),
    };
    let body = if active && !renaming {
        // The active row carries the focus outline (painted inside — no layout shift).
        Container::new()
            .decoration(
                BoxDecoration::new().color(bg).border(Border::new(st.focus_outline, st.focus_outline_width)),
            )
            .child(line)
    } else {
        Container::new().color(bg).child(line)
    };

    // Renaming: no row gestures — the editor owns the input.
    let id = node.id;
    let row_widget: AnyWidget = if renaming {
        body.into_widget()
    } else {
        let g = GestureDetector::new(body)
            .cursor(crate::design::theme::action_cursor())
            .on_hover_enter(move || {
                hovered.set(true);
                explorer.row_hover.set(Some(id));
                explorer.root_drop.set(false);
            })
            .on_hover_exit(move || {
                hovered.set(false);
                if explorer.row_hover.peek() == Some(id) {
                    explorer.row_hover.set(None);
                }
            })
            .on_hover_enter({
                move || {
                    // Hovering a folder while dragging marks it as the drop
                    // target; hovering a collapsed one also starts the
                    // expand-on-hold timer.
                    if explorer.dragging.peek() && is_folder && explorer.drop_ok(Some(id)) {
                        explorer.drop_target.set(Some(id));
                        explorer.root_drop.set(false);
                        if !explorer.expanded.get().contains(&id) {
                            let ex = explorer;
                            pebbles_core::animation::set_timeout(explorer.hover_key, 0.6, move || {
                                // try_peek: the explorer may unmount while the
                                // expand-on-hold timer is pending.
                                if ex.dragging.try_peek().unwrap_or(false)
                                    && ex.drop_target.try_peek().flatten() == Some(id)
                                {
                                    ex.expanded.update(|e| {
                                        e.insert(id);
                                    });
                                }
                            });
                        }
                    }
                }
            })
            .on_hover_exit({
                move || {
                    if explorer.drop_target.get() == Some(id) {
                        explorer.drop_target.set(None);
                    }
                    pebbles_core::animation::clear_timeout(explorer.hover_key);
                }
            })
            // The gesture layer sends pan start only once the pointer travels
            // past the pan slop (`pebbles_core::pan`), so a click — even with
            // jitter — never moves anything. Dropping needs an explicit
            // highlighted target.
            .on_pan_start(pebbles_core::action_event(move |e: pebbles_render::PointerEvent| {
                explorer.press.set(Some((id, e.global)));
            }))
            .on_pan_update(pebbles_core::action_event(move |e: pebbles_render::PointerEvent| {
                if explorer.dragging.peek() {
                    // Dragged out of the window: hand the files to the OS drag-and-drop
                    // (VS Code's drag to other apps); the in-tree drag ends.
                    let (w, h) = crate::services::overlay::window_size_of(window);
                    let outside = e.global.x < 0.0 || e.global.y < 0.0 || e.global.x >= w || e.global.y >= h;
                    if outside && w > 0.0 && explorer.caps.peek().allows(Capability::DragOut) {
                        let paths: Vec<PathBuf> =
                            explorer.selected.peek().iter().filter_map(|&n| explorer.path_of(n)).collect();
                        if !paths.is_empty() {
                            crate::window::start_file_drag(u64::from(window), paths);
                            explorer.dragging.set(false);
                            explorer.drop_target.set(None);
                            explorer.root_drop.set(false);
                            explorer.press.set(None);
                        }
                    }
                    return;
                }
                let Some((pid, _)) = explorer.press.peek() else { return };
                if pid != id {
                    return;
                }
                // Dragging an unselected node selects it; a selected node
                // drags the whole selection with it.
                if !explorer.selected.peek().contains(&id) {
                    explorer.select_only(id);
                }
                explorer.dragging.set(true);
                // Already over another folder when the slop is crossed: it is
                // the target (its hover-enter fired before the drag began).
                if let Some(h) = explorer.row_hover.peek()
                    && h != id
                    && explorer.tree.peek().node(h).is_some_and(|n| n.kind == FsKind::Folder)
                    && explorer.drop_ok(Some(h))
                {
                    explorer.drop_target.set(Some(h));
                }
            }))
            .on_pan_end({
                move || {
                    explorer.press.set(None);
                    if !explorer.dragging.peek() {
                        return;
                    }
                    let ids = explorer.selected.peek().clone();
                    let target = explorer.drop_target.peek();
                    let root = explorer.root_drop.peek() && explorer.row_hover.peek().is_none();
                    explorer.dragging.set(false);
                    explorer.drop_target.set(None);
                    explorer.root_drop.set(false);
                    // Ctrl (Windows/Linux) or Alt/Option (macOS) held on drop copies.
                    let copy = ctrl_held() || pebbles_core::keyboard::alt_held();
                    let dest = match (target, root) {
                        (Some(t), _) => Some(Some(t)),
                        (None, true) => Some(None),
                        (None, false) => None,
                    };
                    match dest {
                        Some(d) if copy => {
                            let _ = explorer.copy_nodes(&ids, d);
                        }
                        Some(d) => {
                            let _ = explorer.request_move(&ids, d);
                        }
                        // Released over a file, the dragged row, or outside
                        // the tree: no target, nothing moves.
                        None => {}
                    }
                }
            })
            .on_tap({
                move || {
                    if ctrl_held() {
                        explorer.toggle_select(id);
                    } else if shift_held() {
                        explorer.range_select(id);
                    } else if is_folder {
                        explorer.select_only(id);
                        explorer.toggle_folder(id);
                    } else {
                        // A single click opens a preview (VS Code).
                        explorer.open_entry(id, OpenMode::Preview);
                    }
                }
            })
            // Double-click: open a file (pinned); a folder toggles again.
            .on_double_tap(move || {
                if is_folder {
                    explorer.toggle_folder(id);
                } else {
                    explorer.open_entry(id, OpenMode::Open);
                }
            });
        // The selection sync lives on the menu's OWN right-click handler:
        // pointer dispatch fires only the TOPMOST listener per slot. Right-
        // clicking an unselected node selects just it (keeps the selection
        // otherwise) — the standard behavior.
        let sync: Rc<dyn Fn()> = Rc::new(move || {
            if !explorer.selected.get().contains(&id) {
                explorer.select_only(id);
            }
        });
        explorer.with_menu(g.into_widget(), explorer.menu_target(Some(id)), Some(sync))
    };

    row_widget
}

/// Props for the inline rename editor.
struct RenameProps {
    explorer: FileExplorer,
    id: u64,
    buf: Signal<String>,
    placeholder: String,
}

/// The inline rename field: Enter/blur commits, Escape cancels.
fn render_rename_editor(p: &RenameProps) -> AnyWidget {
    let explorer = p.explorer;
    let id = p.id;
    let buf = p.buf;
    let commit = move || {
        // Only the editor that is still live commits (blur after Enter/Escape).
        if explorer.renaming.peek() != Some(id) {
            return;
        }
        let name = buf.peek().clone();
        let _ = explorer.commit_rename(id, &name);
    };
    // Select the stem (name without the extension) so typing replaces it while
    // Right/End lets you edit in place — the standard rename UX. New nodes have
    // an EMPTY buffer (the default name shows as the placeholder).
    let stem = {
        let v = buf.peek();
        match v.rfind('.') {
            Some(i) if i > 0 => i,
            _ => v.len(),
        }
    };
    text_field()
        .placeholder(p.placeholder.clone())
        .bind(buf)
        .autofocus()
        .select_range(0, stem)
        .on_submit(move |_| commit())
        .on_focus_change(move |focused| {
            if !focused {
                commit();
            }
        })
        .on_nav(move |k| {
            if matches!(k, KeyInput::Escape) {
                explorer.cancel_rename();
                true
            } else {
                false
            }
        })
        .into_widget()
}
