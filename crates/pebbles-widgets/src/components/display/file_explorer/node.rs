//! The per-node row: [`render_node`] (selection, drag/drop, context menu, expansion)
//! and the inline rename editor. Child module of `file_explorer` — reads the
//! controller's private state directly.

#[allow(clippy::wildcard_imports)]
use super::*;

/// Props for one tree row.
pub(super) struct NodeProps {
    pub(super) explorer: FileExplorer,
    pub(super) node: FsNode,
    pub(super) depth: usize,
}

/// One tree row: indent, twistie, glyph, label — with select/expand (Ctrl/Shift
/// multi), inline rename, a context menu, and drag-to-move (a selection drags
/// together; folders highlight as drop targets and expand on hover-hold).
pub(super) fn render_node(p: &NodeProps) -> AnyWidget {
    let c = theme().colors;
    let explorer = p.explorer;
    let node = &p.node;
    let is_folder = node.kind == FsKind::Folder;
    let expanded = is_folder && explorer.expanded.get().contains(&node.id);
    let sel = explorer.selected.get();
    let selected = sel.contains(&node.id);
    // The ACTIVE row (the last selected — keyboard/rename target) gets the
    // focus ring on top of the selection tint, VSCode-style.
    let active = selected && sel.last() == Some(&node.id);
    let renaming = explorer.renaming.get() == Some(node.id);
    let dragging = explorer.dragging.get();
    let dragged = dragging && selected;
    let drop_target = dragging && explorer.drop_target.get() == Some(node.id);
    // A cut-pending row renders dimmed (like VSCode) until pasted or cancelled.
    let cut_pending = matches!(explorer.clipboard.get(), Some((ref ids, ClipMode::Cut)) if ids.contains(&node.id));
    let hovered = create_signal(false);

    // Row background: FULL accent for selection (a subtle mix reads as "nothing
    // is selected"), hover tint on top, primary tint for the drop target.
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let mut bg = c.background;
    if selected {
        bg = c.accent;
    }
    if drop_target {
        bg = mix(bg, c.primary, 0.18);
    }
    bg = mix(bg, c.foreground, 0.05 * hv as f32);

    let indent = gap_w(p.depth as f64 * 14.0);
    let dim = if selected { c.accent_foreground } else { c.muted_foreground };
    let twistie: AnyWidget = if is_folder {
        icon(if expanded { IconKind::ChevronDown } else { IconKind::ChevronRight })
            .size(14.0)
            .color(dim)
            .into_widget()
    } else {
        gap_w(14.0).into_widget()
    };
    // Glyph resolution: per-node override → installed icon theme → the
    // defaults (open/closed folder, plain file).
    let (glyph_data, glyph_color) = explorer.resolved_icon(&p.node, expanded);
    let glyph = icon(glyph_data).size(16.0).color(glyph_color.unwrap_or(dim));

    let label: AnyWidget = if renaming {
        component_props(
            render_rename_editor,
            RenameProps { explorer, id: node.id, buf: explorer.rename_buf, placeholder: node.name.clone() },
        )
        .into_widget()
    } else {
        text(node.name.clone())
            .size(13.5)
            .color(if dragged || cut_pending {
                c.muted_foreground
            } else if selected {
                c.accent_foreground
            } else {
                c.foreground
            })
            .into_widget()
    };

    let content = row(children![indent, twistie, gap_w(4.0), glyph, gap_w(6.0), Expanded::new(label)])
        .main_axis_size(MainAxisSize::Min);
    let body = if active && !renaming {
        // The active row carries the focus ring (painted inside — no layout shift).
        Container::new()
            .decoration(BoxDecoration::new().color(bg).border(Border::new(c.ring, 1.0)))
            .padding(EdgeInsets::symmetric(6.0, 3.0))
            .child(content)
    } else {
        Container::new().color(bg).padding(EdgeInsets::symmetric(6.0, 3.0)).child(content)
    };

    // Renaming: no row gestures — the editor owns the input.
    let id = node.id;
    let row_widget: AnyWidget = if renaming {
        body.into_widget()
    } else {
        let g = GestureDetector::new(body)
            .cursor(Cursor::Pointer)
            .on_hover_enter({
                let hovered = hovered;
                move || hovered.set(true)
            })
            .on_hover_exit({
                let hovered = hovered;
                move || hovered.set(false)
            })
            .on_hover_enter({
                move || {
                    // Hovering a folder while dragging marks it as the drop
                    // target; hovering a collapsed one also starts the
                    // expand-on-hold timer.
                    if explorer.dragging.get() && is_folder {
                        explorer.drop_target.set(Some(id));
                        explorer.root_drop.set(false);
                        if !explorer.expanded.get().contains(&id) {
                            let ex = explorer;
                            pebbles_core::animation::set_timeout(explorer.hover_key, 0.6, move || {
                                if ex.dragging.get() && ex.drop_target.get() == Some(id) {
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
            .on_pan_start({
                move || {
                    // Dragging an unselected node selects it; a selected node
                    // drags the whole selection with it.
                    if !explorer.selected.get().contains(&id) {
                        explorer.select_only(id);
                    }
                    explorer.dragging.set(true);
                }
            })
            .on_pan_end({
                move || {
                    if !explorer.dragging.get() {
                        return;
                    }
                    let ids = explorer.selected.get();
                    let target = explorer.drop_target.get();
                    let root = explorer.root_drop.get();
                    explorer.move_nodes(&ids, if root { None } else { target });
                    explorer.dragging.set(false);
                    explorer.drop_target.set(None);
                    explorer.root_drop.set(false);
                }
            })
            .on_tap({
                move || {
                    if ctrl_held() {
                        explorer.toggle_select(id);
                    } else if shift_held() {
                        explorer.range_select(id);
                    } else {
                        explorer.select_only(id);
                        if is_folder {
                            explorer.toggle_folder(id);
                        }
                    }
                }
            })
            .on_double_tap(move || explorer.start_rename_for(id));
        context_menu(g)
            // The selection sync lives on the menu's OWN right-click handler:
            // pointer dispatch fires only the TOPMOST listener per slot, so a
            // handler on the inner row would starve the menu (the bug where the
            // explorer's menu never opened).
            .on_open(move |_| {
                // Right-clicking an unselected node selects just it (keeps the
                // selection otherwise) — the standard behavior.
                if !explorer.selected.get().contains(&id) {
                    explorer.select_only(id);
                }
            })
            .item(menu_item("New File").on_select(explorer.new_file()))
            .item(menu_item("New Folder").on_select(explorer.new_folder()))
            .separator()
            .item(menu_item("Cut").on_select(move || explorer.cut_selection()))
            .item(menu_item("Copy").on_select(move || explorer.copy_selection()))
            .item(
                menu_item("Paste")
                    .disabled(explorer.clipboard.get().is_none())
                    .on_select(move || explorer.paste_clipboard()),
            )
            .separator()
            .item(menu_item("Rename").shortcut("F2").on_select(explorer.rename_selected()))
            .item(
                menu_item("Delete")
                    .shortcut("Del")
                    .destructive()
                    .on_select(explorer.delete_selected()),
            )
            .into_widget()
    };

    let mut kids = vec![row_widget];
    if is_folder && expanded && !renaming {
        for child in &node.children {
            kids.push(
                component_props(
                    render_node,
                    NodeProps { explorer, node: child.clone(), depth: p.depth + 1 },
                )
                .into_widget(),
            );
        }
    }
    column(kids)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
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
        let name = buf.peek().trim().to_string();
        if !name.is_empty() {
            // Filesystem mode renames on disk; in-memory renames the model.
            explorer.rename_node(id, name);
        } else {
            explorer.renaming.set(None);
        }
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
                explorer.renaming.set(None);
                true
            } else {
                false
            }
        })
        .into_widget()
}
