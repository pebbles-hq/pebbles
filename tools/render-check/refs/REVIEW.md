# Self-golden review sheet (05 V12 step 3)

Every committed reference in `wgpu/linux-lavapipe/` (588 images), grouped by family, with its render-check case name.
Tick each case once judged correct by eye; compare with the case's spec (golden model / probes) in
`target/render-check/linux-lavapipe/report.md` from `cargo run -p render-check --release -- --out target/render-check`.

Contents: [brush](#brush) (6), [clip](#clip) (25), [effect](#effect) (11), [frag](#frag) (2), [gallery](#gallery) (286), [grad](#grad) (25), [img](#img) (28), [layer](#layer) (39), [mesh](#mesh) (3), [prim](#prim) (19), [screen](#screen) (10), [shadow](#shadow) (14), [sprite](#sprite) (2), [stress](#stress) (4), [stroke](#stroke) (50), [text](#text) (40), [widget](#widget) (17), [xform](#xform) (7)

## brush

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `brush/image_fill_repeat` | 128×128 | 1916 | <img src="wgpu/linux-lavapipe/brush__image_fill_repeat.png" width="128"> |
| [ ] | `brush/image_fill_rrect_reflect` | 128×128 | 7295 | <img src="wgpu/linux-lavapipe/brush__image_fill_rrect_reflect.png" width="128"> |
| [ ] | `brush/image_stroke` | 128×128 | 6797 | <img src="wgpu/linux-lavapipe/brush__image_stroke.png" width="128"> |
| [ ] | `brush/shader_rings_stroke` | 128×128 | 7124 | <img src="wgpu/linux-lavapipe/brush__shader_rings_stroke.png" width="128"> |
| [ ] | `brush/shader_star_uniforms` | 128×128 | 2862 | <img src="wgpu/linux-lavapipe/brush__shader_star_uniforms.png" width="128"> |
| [ ] | `brush/shader_uv_rect` | 128×128 | 482 | <img src="wgpu/linux-lavapipe/brush__shader_uv_rect.png" width="128"> |

## clip

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `clip/circle` | 128×128 | 2525 | <img src="wgpu/linux-lavapipe/clip__circle.png" width="128"> |
| [ ] | `clip/clip_inside_opacity` | 240×200 | 3610 | <img src="wgpu/linux-lavapipe/clip__clip_inside_opacity.png" width="240"> |
| [ ] | `clip/donut_evenodd` | 128×128 | 3490 | <img src="wgpu/linux-lavapipe/clip__donut_evenodd.png" width="128"> |
| [ ] | `clip/ellipse` | 128×128 | 2143 | <img src="wgpu/linux-lavapipe/clip__ellipse.png" width="128"> |
| [ ] | `clip/huge_rotated` | 640×640 | 7773 | <img src="wgpu/linux-lavapipe/clip__huge_rotated.png" width="240"> |
| [ ] | `clip/nested_depth8` | 128×128 | 732 | <img src="wgpu/linux-lavapipe/clip__nested_depth8.png" width="128"> |
| [ ] | `clip/nested_paths_3` | 128×128 | 2167 | <img src="wgpu/linux-lavapipe/clip__nested_paths_3.png" width="128"> |
| [ ] | `clip/oval_widget` | 240×160 | 3958 | <img src="wgpu/linux-lavapipe/clip__oval_widget.png" width="240"> |
| [ ] | `clip/path_star` | 128×128 | 2902 | <img src="wgpu/linux-lavapipe/clip__path_star.png" width="128"> |
| [ ] | `clip/path_star_evenodd` | 128×128 | 3518 | <img src="wgpu/linux-lavapipe/clip__path_star_evenodd.png" width="128"> |
| [ ] | `clip/rect_fractional` | 128×128 | 485 | <img src="wgpu/linux-lavapipe/clip__rect_fractional.png" width="128"> |
| [ ] | `clip/rect_fractional_s125` | 160×160 | 577 | <img src="wgpu/linux-lavapipe/clip__rect_fractional_s125.png" width="160"> |
| [ ] | `clip/rect_fractional_s150` | 192×192 | 677 | <img src="wgpu/linux-lavapipe/clip__rect_fractional_s150.png" width="192"> |
| [ ] | `clip/rect_fractional_s175` | 224×224 | 912 | <img src="wgpu/linux-lavapipe/clip__rect_fractional_s175.png" width="224"> |
| [ ] | `clip/rect_fractional_s200` | 256×256 | 997 | <img src="wgpu/linux-lavapipe/clip__rect_fractional_s200.png" width="240"> |
| [ ] | `clip/rect_negative_origin` | 128×128 | 336 | <img src="wgpu/linux-lavapipe/clip__rect_negative_origin.png" width="128"> |
| [ ] | `clip/rect_rotated` | 128×128 | 2443 | <img src="wgpu/linux-lavapipe/clip__rect_rotated.png" width="128"> |
| [ ] | `clip/rect_scissor` | 128×128 | 366 | <img src="wgpu/linux-lavapipe/clip__rect_scissor.png" width="128"> |
| [ ] | `clip/rotated_rect_30` | 128×128 | 3077 | <img src="wgpu/linux-lavapipe/clip__rotated_rect_30.png" width="128"> |
| [ ] | `clip/rrect` | 128×128 | 1512 | <img src="wgpu/linux-lavapipe/clip__rrect.png" width="128"> |
| [ ] | `clip/rrect_in_rrect` | 128×128 | 700 | <img src="wgpu/linux-lavapipe/clip__rrect_in_rrect.png" width="128"> |
| [ ] | `clip/rrect_overlap_edge` | 128×128 | 1150 | <img src="wgpu/linux-lavapipe/clip__rrect_overlap_edge.png" width="128"> |
| [ ] | `clip/rrect_zero_radius` | 128×128 | 366 | <img src="wgpu/linux-lavapipe/clip__rrect_zero_radius.png" width="128"> |
| [ ] | `clip/scroll_viewport` | 240×200 | 3859 | <img src="wgpu/linux-lavapipe/clip__scroll_viewport.png" width="240"> |
| [ ] | `clip/volatile_path` | 128×128 | 1658 | <img src="wgpu/linux-lavapipe/clip__volatile_path.png" width="128"> |

## effect

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `effect/backdrop_frosted` | 128×96 | 868 | <img src="wgpu/linux-lavapipe/effect__backdrop_frosted.png" width="128"> |
| [ ] | `effect/backdrop_nested` | 128×96 | 432 | <img src="wgpu/linux-lavapipe/effect__backdrop_nested.png" width="128"> |
| [ ] | `effect/blur_square` | 128×96 | 2466 | <img src="wgpu/linux-lavapipe/effect__blur_square.png" width="128"> |
| [ ] | `effect/blur_text` | 128×96 | 3179 | <img src="wgpu/linux-lavapipe/effect__blur_text.png" width="128"> |
| [ ] | `effect/chain_blur_matrix` | 128×96 | 1185 | <img src="wgpu/linux-lavapipe/effect__chain_blur_matrix.png" width="128"> |
| [ ] | `effect/color_matrix_alpha_offset` | 128×96 | 273 | <img src="wgpu/linux-lavapipe/effect__color_matrix_alpha_offset.png" width="128"> |
| [ ] | `effect/color_matrix_grayscale` | 128×96 | 260 | <img src="wgpu/linux-lavapipe/effect__color_matrix_grayscale.png" width="128"> |
| [ ] | `effect/dilate` | 128×96 | 1061 | <img src="wgpu/linux-lavapipe/effect__dilate.png" width="128"> |
| [ ] | `effect/erode` | 128×96 | 301 | <img src="wgpu/linux-lavapipe/effect__erode.png" width="128"> |
| [ ] | `effect/perspective_card` | 128×96 | 711 | <img src="wgpu/linux-lavapipe/effect__perspective_card.png" width="128"> |
| [ ] | `effect/perspective_text` | 128×96 | 2191 | <img src="wgpu/linux-lavapipe/effect__perspective_text.png" width="128"> |

## frag

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `frag/append_twice` | 128×128 | 1858 | <img src="wgpu/linux-lavapipe/frag__append_twice.png" width="128"> |
| [ ] | `frag/nested_transforms` | 128×128 | 2281 | <img src="wgpu/linux-lavapipe/frag__nested_transforms.png" width="128"> |


## gallery

Every `examples/gallery` page, headless (800×1000, bundled fonts, settled): Default light for every page; `_material` / `_tailwind` / `_dark` for the Input, Display, Navigation and Overlays groups; `_rtl` for the mirroring pages; `_open` with the page's first overlay open.

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `gallery/absorb_pointer` | 800×1000 | 16882 | <img src="wgpu/linux-lavapipe/gallery__absorb_pointer.png" width="240"> |
| [ ] | `gallery/accordion` | 800×1000 | 47418 | <img src="wgpu/linux-lavapipe/gallery__accordion.png" width="240"> |
| [ ] | `gallery/action_chip` | 800×1000 | 24021 | <img src="wgpu/linux-lavapipe/gallery__action_chip.png" width="240"> |
| [ ] | `gallery/action_chip_dark` | 800×1000 | 24864 | <img src="wgpu/linux-lavapipe/gallery__action_chip_dark.png" width="240"> |
| [ ] | `gallery/action_chip_material` | 800×1000 | 26328 | <img src="wgpu/linux-lavapipe/gallery__action_chip_material.png" width="240"> |
| [ ] | `gallery/action_chip_tailwind` | 800×1000 | 26193 | <img src="wgpu/linux-lavapipe/gallery__action_chip_tailwind.png" width="240"> |
| [ ] | `gallery/alert` | 800×1000 | 61585 | <img src="wgpu/linux-lavapipe/gallery__alert.png" width="240"> |
| [ ] | `gallery/alert_dark` | 800×1000 | 65559 | <img src="wgpu/linux-lavapipe/gallery__alert_dark.png" width="240"> |
| [ ] | `gallery/alert_material` | 800×1000 | 68707 | <img src="wgpu/linux-lavapipe/gallery__alert_material.png" width="240"> |
| [ ] | `gallery/alert_tailwind` | 800×1000 | 65184 | <img src="wgpu/linux-lavapipe/gallery__alert_tailwind.png" width="240"> |
| [ ] | `gallery/animated_align` | 800×1000 | 14024 | <img src="wgpu/linux-lavapipe/gallery__animated_align.png" width="240"> |
| [ ] | `gallery/animated_container` | 800×1000 | 27613 | <img src="wgpu/linux-lavapipe/gallery__animated_container.png" width="240"> |
| [ ] | `gallery/animated_cross_fade` | 800×1000 | 15264 | <img src="wgpu/linux-lavapipe/gallery__animated_cross_fade.png" width="240"> |
| [ ] | `gallery/animated_grid` | 800×1000 | 15981 | <img src="wgpu/linux-lavapipe/gallery__animated_grid.png" width="240"> |
| [ ] | `gallery/animated_list` | 800×1000 | 17828 | <img src="wgpu/linux-lavapipe/gallery__animated_list.png" width="240"> |
| [ ] | `gallery/animated_opacity` | 800×1000 | 14655 | <img src="wgpu/linux-lavapipe/gallery__animated_opacity.png" width="240"> |
| [ ] | `gallery/animated_padding` | 800×1000 | 15046 | <img src="wgpu/linux-lavapipe/gallery__animated_padding.png" width="240"> |
| [ ] | `gallery/animated_positioned` | 800×1000 | 15417 | <img src="wgpu/linux-lavapipe/gallery__animated_positioned.png" width="240"> |
| [ ] | `gallery/animated_rotation` | 800×1000 | 14646 | <img src="wgpu/linux-lavapipe/gallery__animated_rotation.png" width="240"> |
| [ ] | `gallery/animated_scale` | 800×1000 | 13737 | <img src="wgpu/linux-lavapipe/gallery__animated_scale.png" width="240"> |
| [ ] | `gallery/animated_slide` | 800×1000 | 13569 | <img src="wgpu/linux-lavapipe/gallery__animated_slide.png" width="240"> |
| [ ] | `gallery/animated_switcher` | 800×1000 | 14159 | <img src="wgpu/linux-lavapipe/gallery__animated_switcher.png" width="240"> |
| [ ] | `gallery/avatar` | 800×1000 | 50034 | <img src="wgpu/linux-lavapipe/gallery__avatar.png" width="240"> |
| [ ] | `gallery/avatar_dark` | 800×1000 | 52540 | <img src="wgpu/linux-lavapipe/gallery__avatar_dark.png" width="240"> |
| [ ] | `gallery/avatar_material` | 800×1000 | 50034 | <img src="wgpu/linux-lavapipe/gallery__avatar_material.png" width="240"> |
| [ ] | `gallery/avatar_tailwind` | 800×1000 | 50034 | <img src="wgpu/linux-lavapipe/gallery__avatar_tailwind.png" width="240"> |
| [ ] | `gallery/badge` | 800×1000 | 32541 | <img src="wgpu/linux-lavapipe/gallery__badge.png" width="240"> |
| [ ] | `gallery/badge_dark` | 800×1000 | 33901 | <img src="wgpu/linux-lavapipe/gallery__badge_dark.png" width="240"> |
| [ ] | `gallery/badge_material` | 800×1000 | 37327 | <img src="wgpu/linux-lavapipe/gallery__badge_material.png" width="240"> |
| [ ] | `gallery/badge_tailwind` | 800×1000 | 36835 | <img src="wgpu/linux-lavapipe/gallery__badge_tailwind.png" width="240"> |
| [ ] | `gallery/banner` | 800×1000 | 18772 | <img src="wgpu/linux-lavapipe/gallery__banner.png" width="240"> |
| [ ] | `gallery/banner_dark` | 800×1000 | 20558 | <img src="wgpu/linux-lavapipe/gallery__banner_dark.png" width="240"> |
| [ ] | `gallery/banner_material` | 800×1000 | 21429 | <img src="wgpu/linux-lavapipe/gallery__banner_material.png" width="240"> |
| [ ] | `gallery/banner_tailwind` | 800×1000 | 19461 | <img src="wgpu/linux-lavapipe/gallery__banner_tailwind.png" width="240"> |
| [ ] | `gallery/baseline` | 800×1000 | 13653 | <img src="wgpu/linux-lavapipe/gallery__baseline.png" width="240"> |
| [ ] | `gallery/boxes` | 800×1000 | 41994 | <img src="wgpu/linux-lavapipe/gallery__boxes.png" width="240"> |
| [ ] | `gallery/breadcrumb` | 800×1000 | 33941 | <img src="wgpu/linux-lavapipe/gallery__breadcrumb.png" width="240"> |
| [ ] | `gallery/breadcrumb_dark` | 800×1000 | 35521 | <img src="wgpu/linux-lavapipe/gallery__breadcrumb_dark.png" width="240"> |
| [ ] | `gallery/breadcrumb_material` | 800×1000 | 33941 | <img src="wgpu/linux-lavapipe/gallery__breadcrumb_material.png" width="240"> |
| [ ] | `gallery/breadcrumb_rtl` | 800×1000 | 34399 | <img src="wgpu/linux-lavapipe/gallery__breadcrumb_rtl.png" width="240"> |
| [ ] | `gallery/breadcrumb_tailwind` | 800×1000 | 33941 | <img src="wgpu/linux-lavapipe/gallery__breadcrumb_tailwind.png" width="240"> |
| [ ] | `gallery/button_group` | 800×1000 | 24911 | <img src="wgpu/linux-lavapipe/gallery__button_group.png" width="240"> |
| [ ] | `gallery/button_group_dark` | 800×1000 | 26379 | <img src="wgpu/linux-lavapipe/gallery__button_group_dark.png" width="240"> |
| [ ] | `gallery/button_group_material` | 800×1000 | 30133 | <img src="wgpu/linux-lavapipe/gallery__button_group_material.png" width="240"> |
| [ ] | `gallery/button_group_tailwind` | 800×1000 | 26994 | <img src="wgpu/linux-lavapipe/gallery__button_group_tailwind.png" width="240"> |
| [ ] | `gallery/buttons` | 800×1000 | 82579 | <img src="wgpu/linux-lavapipe/gallery__buttons.png" width="240"> |
| [ ] | `gallery/buttons_dark` | 800×1000 | 83529 | <img src="wgpu/linux-lavapipe/gallery__buttons_dark.png" width="240"> |
| [ ] | `gallery/buttons_material` | 800×1000 | 99169 | <img src="wgpu/linux-lavapipe/gallery__buttons_material.png" width="240"> |
| [ ] | `gallery/buttons_rtl` | 800×1000 | 49338 | <img src="wgpu/linux-lavapipe/gallery__buttons_rtl.png" width="240"> |
| [ ] | `gallery/buttons_tailwind` | 800×1000 | 90388 | <img src="wgpu/linux-lavapipe/gallery__buttons_tailwind.png" width="240"> |
| [ ] | `gallery/canvas` | 800×1000 | 12501 | <img src="wgpu/linux-lavapipe/gallery__canvas.png" width="240"> |
| [ ] | `gallery/card` | 800×1000 | 53389 | <img src="wgpu/linux-lavapipe/gallery__card.png" width="240"> |
| [ ] | `gallery/card_dark` | 800×1000 | 58162 | <img src="wgpu/linux-lavapipe/gallery__card_dark.png" width="240"> |
| [ ] | `gallery/card_material` | 800×1000 | 66935 | <img src="wgpu/linux-lavapipe/gallery__card_material.png" width="240"> |
| [ ] | `gallery/card_tailwind` | 800×1000 | 58585 | <img src="wgpu/linux-lavapipe/gallery__card_tailwind.png" width="240"> |
| [ ] | `gallery/chips` | 800×1000 | 21908 | <img src="wgpu/linux-lavapipe/gallery__chips.png" width="240"> |
| [ ] | `gallery/chips_dark` | 800×1000 | 21722 | <img src="wgpu/linux-lavapipe/gallery__chips_dark.png" width="240"> |
| [ ] | `gallery/chips_material` | 800×1000 | 26489 | <img src="wgpu/linux-lavapipe/gallery__chips_material.png" width="240"> |
| [ ] | `gallery/chips_tailwind` | 800×1000 | 26088 | <img src="wgpu/linux-lavapipe/gallery__chips_tailwind.png" width="240"> |
| [ ] | `gallery/choice_chip` | 800×1000 | 24516 | <img src="wgpu/linux-lavapipe/gallery__choice_chip.png" width="240"> |
| [ ] | `gallery/choice_chip_dark` | 800×1000 | 25423 | <img src="wgpu/linux-lavapipe/gallery__choice_chip_dark.png" width="240"> |
| [ ] | `gallery/choice_chip_material` | 800×1000 | 27046 | <img src="wgpu/linux-lavapipe/gallery__choice_chip_material.png" width="240"> |
| [ ] | `gallery/choice_chip_tailwind` | 800×1000 | 26872 | <img src="wgpu/linux-lavapipe/gallery__choice_chip_tailwind.png" width="240"> |
| [ ] | `gallery/chrome` | 800×1000 | 22650 | <img src="wgpu/linux-lavapipe/gallery__chrome.png" width="240"> |
| [ ] | `gallery/chrome_dark` | 800×1000 | 24087 | <img src="wgpu/linux-lavapipe/gallery__chrome_dark.png" width="240"> |
| [ ] | `gallery/chrome_material` | 800×1000 | 25035 | <img src="wgpu/linux-lavapipe/gallery__chrome_material.png" width="240"> |
| [ ] | `gallery/chrome_tailwind` | 800×1000 | 23195 | <img src="wgpu/linux-lavapipe/gallery__chrome_tailwind.png" width="240"> |
| [ ] | `gallery/clip_oval` | 800×1000 | 35022 | <img src="wgpu/linux-lavapipe/gallery__clip_oval.png" width="240"> |
| [ ] | `gallery/clip_path` | 800×1000 | 31995 | <img src="wgpu/linux-lavapipe/gallery__clip_path.png" width="240"> |
| [ ] | `gallery/clip_rect` | 800×1000 | 27380 | <img src="wgpu/linux-lavapipe/gallery__clip_rect.png" width="240"> |
| [ ] | `gallery/collapsible` | 800×1000 | 35271 | <img src="wgpu/linux-lavapipe/gallery__collapsible.png" width="240"> |
| [ ] | `gallery/color_filtered` | 800×1000 | 44600 | <img src="wgpu/linux-lavapipe/gallery__color_filtered.png" width="240"> |
| [ ] | `gallery/colors` | 800×1000 | 65024 | <img src="wgpu/linux-lavapipe/gallery__colors.png" width="240"> |
| [ ] | `gallery/columns` | 800×1000 | 35732 | <img src="wgpu/linux-lavapipe/gallery__columns.png" width="240"> |
| [ ] | `gallery/combobox` | 800×1000 | 23805 | <img src="wgpu/linux-lavapipe/gallery__combobox.png" width="240"> |
| [ ] | `gallery/combobox_dark` | 800×1000 | 25399 | <img src="wgpu/linux-lavapipe/gallery__combobox_dark.png" width="240"> |
| [ ] | `gallery/combobox_material` | 800×1000 | 27066 | <img src="wgpu/linux-lavapipe/gallery__combobox_material.png" width="240"> |
| [ ] | `gallery/combobox_tailwind` | 800×1000 | 25222 | <img src="wgpu/linux-lavapipe/gallery__combobox_tailwind.png" width="240"> |
| [ ] | `gallery/command` | 800×1000 | 26810 | <img src="wgpu/linux-lavapipe/gallery__command.png" width="240"> |
| [ ] | `gallery/command_dark` | 800×1000 | 28562 | <img src="wgpu/linux-lavapipe/gallery__command_dark.png" width="240"> |
| [ ] | `gallery/command_material` | 800×1000 | 30590 | <img src="wgpu/linux-lavapipe/gallery__command_material.png" width="240"> |
| [ ] | `gallery/command_open` | 800×1000 | 41307 | <img src="wgpu/linux-lavapipe/gallery__command_open.png" width="240"> |
| [ ] | `gallery/command_tailwind` | 800×1000 | 28363 | <img src="wgpu/linux-lavapipe/gallery__command_tailwind.png" width="240"> |
| [ ] | `gallery/context_menu` | 800×1000 | 49334 | <img src="wgpu/linux-lavapipe/gallery__context_menu.png" width="240"> |
| [ ] | `gallery/context_menu_dark` | 800×1000 | 52305 | <img src="wgpu/linux-lavapipe/gallery__context_menu_dark.png" width="240"> |
| [ ] | `gallery/context_menu_material` | 800×1000 | 55479 | <img src="wgpu/linux-lavapipe/gallery__context_menu_material.png" width="240"> |
| [ ] | `gallery/context_menu_tailwind` | 800×1000 | 51973 | <img src="wgpu/linux-lavapipe/gallery__context_menu_tailwind.png" width="240"> |
| [ ] | `gallery/custom_multi_child` | 800×1000 | 16009 | <img src="wgpu/linux-lavapipe/gallery__custom_multi_child.png" width="240"> |
| [ ] | `gallery/custom_single_child` | 800×1000 | 15880 | <img src="wgpu/linux-lavapipe/gallery__custom_single_child.png" width="240"> |
| [ ] | `gallery/data_table` | 800×1000 | 57830 | <img src="wgpu/linux-lavapipe/gallery__data_table.png" width="240"> |
| [ ] | `gallery/data_table_dark` | 800×1000 | 61801 | <img src="wgpu/linux-lavapipe/gallery__data_table_dark.png" width="240"> |
| [ ] | `gallery/data_table_material` | 800×1000 | 61142 | <img src="wgpu/linux-lavapipe/gallery__data_table_material.png" width="240"> |
| [ ] | `gallery/data_table_tailwind` | 800×1000 | 57740 | <img src="wgpu/linux-lavapipe/gallery__data_table_tailwind.png" width="240"> |
| [ ] | `gallery/date_picker` | 800×1000 | 59342 | <img src="wgpu/linux-lavapipe/gallery__date_picker.png" width="240"> |
| [ ] | `gallery/date_picker_dark` | 800×1000 | 62481 | <img src="wgpu/linux-lavapipe/gallery__date_picker_dark.png" width="240"> |
| [ ] | `gallery/date_picker_material` | 800×1000 | 64447 | <img src="wgpu/linux-lavapipe/gallery__date_picker_material.png" width="240"> |
| [ ] | `gallery/date_picker_tailwind` | 800×1000 | 59960 | <img src="wgpu/linux-lavapipe/gallery__date_picker_tailwind.png" width="240"> |
| [ ] | `gallery/decorated_box_transition` | 800×1000 | 16166 | <img src="wgpu/linux-lavapipe/gallery__decorated_box_transition.png" width="240"> |
| [ ] | `gallery/default_text_style` | 800×1000 | 32970 | <img src="wgpu/linux-lavapipe/gallery__default_text_style.png" width="240"> |
| [ ] | `gallery/dialog` | 800×1000 | 23691 | <img src="wgpu/linux-lavapipe/gallery__dialog.png" width="240"> |
| [ ] | `gallery/dialog_dark` | 800×1000 | 24753 | <img src="wgpu/linux-lavapipe/gallery__dialog_dark.png" width="240"> |
| [ ] | `gallery/dialog_material` | 800×1000 | 31577 | <img src="wgpu/linux-lavapipe/gallery__dialog_material.png" width="240"> |
| [ ] | `gallery/dialog_open` | 800×1000 | 41777 | <img src="wgpu/linux-lavapipe/gallery__dialog_open.png" width="240"> |
| [ ] | `gallery/dialog_tailwind` | 800×1000 | 25235 | <img src="wgpu/linux-lavapipe/gallery__dialog_tailwind.png" width="240"> |
| [ ] | `gallery/dismissible` | 800×1000 | 22246 | <img src="wgpu/linux-lavapipe/gallery__dismissible.png" width="240"> |
| [ ] | `gallery/drag_target` | 800×1000 | 18263 | <img src="wgpu/linux-lavapipe/gallery__drag_target.png" width="240"> |
| [ ] | `gallery/draggable` | 800×1000 | 18428 | <img src="wgpu/linux-lavapipe/gallery__draggable.png" width="240"> |
| [ ] | `gallery/draggable_sheet` | 800×1000 | 16345 | <img src="wgpu/linux-lavapipe/gallery__draggable_sheet.png" width="240"> |
| [ ] | `gallery/empty` | 800×1000 | 39025 | <img src="wgpu/linux-lavapipe/gallery__empty.png" width="240"> |
| [ ] | `gallery/empty_dark` | 800×1000 | 41719 | <img src="wgpu/linux-lavapipe/gallery__empty_dark.png" width="240"> |
| [ ] | `gallery/empty_material` | 800×1000 | 44337 | <img src="wgpu/linux-lavapipe/gallery__empty_material.png" width="240"> |
| [ ] | `gallery/empty_tailwind` | 800×1000 | 41324 | <img src="wgpu/linux-lavapipe/gallery__empty_tailwind.png" width="240"> |
| [ ] | `gallery/fab` | 800×1000 | 29272 | <img src="wgpu/linux-lavapipe/gallery__fab.png" width="240"> |
| [ ] | `gallery/fab_dark` | 800×1000 | 26583 | <img src="wgpu/linux-lavapipe/gallery__fab_dark.png" width="240"> |
| [ ] | `gallery/fab_material` | 800×1000 | 29149 | <img src="wgpu/linux-lavapipe/gallery__fab_material.png" width="240"> |
| [ ] | `gallery/fab_tailwind` | 800×1000 | 29148 | <img src="wgpu/linux-lavapipe/gallery__fab_tailwind.png" width="240"> |
| [ ] | `gallery/fade_transition` | 800×1000 | 14299 | <img src="wgpu/linux-lavapipe/gallery__fade_transition.png" width="240"> |
| [ ] | `gallery/file_explorer` | 800×1000 | 41143 | <img src="wgpu/linux-lavapipe/gallery__file_explorer.png" width="240"> |
| [ ] | `gallery/file_explorer_dark` | 800×1000 | 43805 | <img src="wgpu/linux-lavapipe/gallery__file_explorer_dark.png" width="240"> |
| [ ] | `gallery/file_explorer_material` | 800×1000 | 45820 | <img src="wgpu/linux-lavapipe/gallery__file_explorer_material.png" width="240"> |
| [ ] | `gallery/file_explorer_tailwind` | 800×1000 | 43703 | <img src="wgpu/linux-lavapipe/gallery__file_explorer_tailwind.png" width="240"> |
| [ ] | `gallery/filter_chip` | 800×1000 | 23926 | <img src="wgpu/linux-lavapipe/gallery__filter_chip.png" width="240"> |
| [ ] | `gallery/filter_chip_dark` | 800×1000 | 24692 | <img src="wgpu/linux-lavapipe/gallery__filter_chip_dark.png" width="240"> |
| [ ] | `gallery/filter_chip_material` | 800×1000 | 27086 | <img src="wgpu/linux-lavapipe/gallery__filter_chip_material.png" width="240"> |
| [ ] | `gallery/filter_chip_tailwind` | 800×1000 | 26889 | <img src="wgpu/linux-lavapipe/gallery__filter_chip_tailwind.png" width="240"> |
| [ ] | `gallery/flow` | 800×1000 | 17694 | <img src="wgpu/linux-lavapipe/gallery__flow.png" width="240"> |
| [ ] | `gallery/fonts` | 800×1000 | 72333 | <img src="wgpu/linux-lavapipe/gallery__fonts.png" width="240"> |
| [ ] | `gallery/fractional_translation` | 800×1000 | 14268 | <img src="wgpu/linux-lavapipe/gallery__fractional_translation.png" width="240"> |
| [ ] | `gallery/grid_tile` | 800×1000 | 74704 | <img src="wgpu/linux-lavapipe/gallery__grid_tile.png" width="240"> |
| [ ] | `gallery/grid_tile_dark` | 800×1000 | 84009 | <img src="wgpu/linux-lavapipe/gallery__grid_tile_dark.png" width="240"> |
| [ ] | `gallery/grid_tile_material` | 800×1000 | 74891 | <img src="wgpu/linux-lavapipe/gallery__grid_tile_material.png" width="240"> |
| [ ] | `gallery/grid_tile_tailwind` | 800×1000 | 74793 | <img src="wgpu/linux-lavapipe/gallery__grid_tile_tailwind.png" width="240"> |
| [ ] | `gallery/grid_view` | 800×1000 | 26482 | <img src="wgpu/linux-lavapipe/gallery__grid_view.png" width="240"> |
| [ ] | `gallery/hero` | 800×1000 | 13191 | <img src="wgpu/linux-lavapipe/gallery__hero.png" width="240"> |
| [ ] | `gallery/icons` | 800×1000 | 50840 | <img src="wgpu/linux-lavapipe/gallery__icons.png" width="240"> |
| [ ] | `gallery/icons_dark` | 800×1000 | 54594 | <img src="wgpu/linux-lavapipe/gallery__icons_dark.png" width="240"> |
| [ ] | `gallery/icons_material` | 800×1000 | 52571 | <img src="wgpu/linux-lavapipe/gallery__icons_material.png" width="240"> |
| [ ] | `gallery/icons_tailwind` | 800×1000 | 51518 | <img src="wgpu/linux-lavapipe/gallery__icons_tailwind.png" width="240"> |
| [ ] | `gallery/ignore_pointer` | 800×1000 | 15754 | <img src="wgpu/linux-lavapipe/gallery__ignore_pointer.png" width="240"> |
| [ ] | `gallery/images` | 800×1000 | 157261 | <img src="wgpu/linux-lavapipe/gallery__images.png" width="240"> |
| [ ] | `gallery/images_dark` | 800×1000 | 158428 | <img src="wgpu/linux-lavapipe/gallery__images_dark.png" width="240"> |
| [ ] | `gallery/images_material` | 800×1000 | 157261 | <img src="wgpu/linux-lavapipe/gallery__images_material.png" width="240"> |
| [ ] | `gallery/images_tailwind` | 800×1000 | 157261 | <img src="wgpu/linux-lavapipe/gallery__images_tailwind.png" width="240"> |
| [ ] | `gallery/indexed_stack` | 800×1000 | 16365 | <img src="wgpu/linux-lavapipe/gallery__indexed_stack.png" width="240"> |
| [ ] | `gallery/interactive_viewer` | 800×1000 | 17912 | <img src="wgpu/linux-lavapipe/gallery__interactive_viewer.png" width="240"> |
| [ ] | `gallery/kbd` | 800×1000 | 27973 | <img src="wgpu/linux-lavapipe/gallery__kbd.png" width="240"> |
| [ ] | `gallery/kbd_dark` | 800×1000 | 29170 | <img src="wgpu/linux-lavapipe/gallery__kbd_dark.png" width="240"> |
| [ ] | `gallery/kbd_material` | 800×1000 | 34395 | <img src="wgpu/linux-lavapipe/gallery__kbd_material.png" width="240"> |
| [ ] | `gallery/kbd_tailwind` | 800×1000 | 30922 | <img src="wgpu/linux-lavapipe/gallery__kbd_tailwind.png" width="240"> |
| [ ] | `gallery/layout_builder` | 800×1000 | 14914 | <img src="wgpu/linux-lavapipe/gallery__layout_builder.png" width="240"> |
| [ ] | `gallery/list` | 800×1000 | 56253 | <img src="wgpu/linux-lavapipe/gallery__list.png" width="240"> |
| [ ] | `gallery/list_body` | 800×1000 | 14502 | <img src="wgpu/linux-lavapipe/gallery__list_body.png" width="240"> |
| [ ] | `gallery/list_dark` | 800×1000 | 59946 | <img src="wgpu/linux-lavapipe/gallery__list_dark.png" width="240"> |
| [ ] | `gallery/list_material` | 800×1000 | 61478 | <img src="wgpu/linux-lavapipe/gallery__list_material.png" width="240"> |
| [ ] | `gallery/list_rtl` | 800×1000 | 55281 | <img src="wgpu/linux-lavapipe/gallery__list_rtl.png" width="240"> |
| [ ] | `gallery/list_tailwind` | 800×1000 | 57249 | <img src="wgpu/linux-lavapipe/gallery__list_tailwind.png" width="240"> |
| [ ] | `gallery/list_tiles` | 800×1000 | 20476 | <img src="wgpu/linux-lavapipe/gallery__list_tiles.png" width="240"> |
| [ ] | `gallery/list_tiles_dark` | 800×1000 | 21604 | <img src="wgpu/linux-lavapipe/gallery__list_tiles_dark.png" width="240"> |
| [ ] | `gallery/list_tiles_material` | 800×1000 | 20885 | <img src="wgpu/linux-lavapipe/gallery__list_tiles_material.png" width="240"> |
| [ ] | `gallery/list_tiles_tailwind` | 800×1000 | 20821 | <img src="wgpu/linux-lavapipe/gallery__list_tiles_tailwind.png" width="240"> |
| [ ] | `gallery/listener` | 800×1000 | 38643 | <img src="wgpu/linux-lavapipe/gallery__listener.png" width="240"> |
| [ ] | `gallery/list_view` | 800×1000 | 45398 | <img src="wgpu/linux-lavapipe/gallery__list_view.png" width="240"> |
| [ ] | `gallery/long_press_draggable` | 800×1000 | 16409 | <img src="wgpu/linux-lavapipe/gallery__long_press_draggable.png" width="240"> |
| [ ] | `gallery/media_query` | 800×1000 | 26136 | <img src="wgpu/linux-lavapipe/gallery__media_query.png" width="240"> |
| [ ] | `gallery/menubar` | 800×1000 | 17639 | <img src="wgpu/linux-lavapipe/gallery__menubar.png" width="240"> |
| [ ] | `gallery/menubar_dark` | 800×1000 | 18622 | <img src="wgpu/linux-lavapipe/gallery__menubar_dark.png" width="240"> |
| [ ] | `gallery/menubar_material` | 800×1000 | 18614 | <img src="wgpu/linux-lavapipe/gallery__menubar_material.png" width="240"> |
| [ ] | `gallery/menubar_open` | 800×1000 | 20447 | <img src="wgpu/linux-lavapipe/gallery__menubar_open.png" width="240"> |
| [ ] | `gallery/menubar_tailwind` | 800×1000 | 18063 | <img src="wgpu/linux-lavapipe/gallery__menubar_tailwind.png" width="240"> |
| [ ] | `gallery/mobile_runtime` | 800×1000 | 36940 | <img src="wgpu/linux-lavapipe/gallery__mobile_runtime.png" width="240"> |
| [ ] | `gallery/nested_scroll` | 800×1000 | 22320 | <img src="wgpu/linux-lavapipe/gallery__nested_scroll.png" width="240"> |
| [ ] | `gallery/offstage` | 800×1000 | 20608 | <img src="wgpu/linux-lavapipe/gallery__offstage.png" width="240"> |
| [ ] | `gallery/orientation_builder` | 800×1000 | 15482 | <img src="wgpu/linux-lavapipe/gallery__orientation_builder.png" width="240"> |
| [ ] | `gallery/pagination` | 800×1000 | 50820 | <img src="wgpu/linux-lavapipe/gallery__pagination.png" width="240"> |
| [ ] | `gallery/pagination_dark` | 800×1000 | 53783 | <img src="wgpu/linux-lavapipe/gallery__pagination_dark.png" width="240"> |
| [ ] | `gallery/pagination_material` | 800×1000 | 73149 | <img src="wgpu/linux-lavapipe/gallery__pagination_material.png" width="240"> |
| [ ] | `gallery/pagination_tailwind` | 800×1000 | 59243 | <img src="wgpu/linux-lavapipe/gallery__pagination_tailwind.png" width="240"> |
| [ ] | `gallery/placeholder` | 800×1000 | 18910 | <img src="wgpu/linux-lavapipe/gallery__placeholder.png" width="240"> |
| [ ] | `gallery/placeholder_dark` | 800×1000 | 18904 | <img src="wgpu/linux-lavapipe/gallery__placeholder_dark.png" width="240"> |
| [ ] | `gallery/placeholder_material` | 800×1000 | 18910 | <img src="wgpu/linux-lavapipe/gallery__placeholder_material.png" width="240"> |
| [ ] | `gallery/placeholder_tailwind` | 800×1000 | 18910 | <img src="wgpu/linux-lavapipe/gallery__placeholder_tailwind.png" width="240"> |
| [ ] | `gallery/popovers` | 800×1000 | 18276 | <img src="wgpu/linux-lavapipe/gallery__popovers.png" width="240"> |
| [ ] | `gallery/popovers_dark` | 800×1000 | 19542 | <img src="wgpu/linux-lavapipe/gallery__popovers_dark.png" width="240"> |
| [ ] | `gallery/popovers_material` | 800×1000 | 21850 | <img src="wgpu/linux-lavapipe/gallery__popovers_material.png" width="240"> |
| [ ] | `gallery/popovers_open` | 800×1000 | 22692 | <img src="wgpu/linux-lavapipe/gallery__popovers_open.png" width="240"> |
| [ ] | `gallery/popovers_tailwind` | 800×1000 | 19952 | <img src="wgpu/linux-lavapipe/gallery__popovers_tailwind.png" width="240"> |
| [ ] | `gallery/positioned_transition` | 800×1000 | 14542 | <img src="wgpu/linux-lavapipe/gallery__positioned_transition.png" width="240"> |
| [ ] | `gallery/pressable` | 800×1000 | 23203 | <img src="wgpu/linux-lavapipe/gallery__pressable.png" width="240"> |
| [ ] | `gallery/pressable_dark` | 800×1000 | 25576 | <img src="wgpu/linux-lavapipe/gallery__pressable_dark.png" width="240"> |
| [ ] | `gallery/pressable_material` | 800×1000 | 23203 | <img src="wgpu/linux-lavapipe/gallery__pressable_material.png" width="240"> |
| [ ] | `gallery/pressable_tailwind` | 800×1000 | 23203 | <img src="wgpu/linux-lavapipe/gallery__pressable_tailwind.png" width="240"> |
| [ ] | `gallery/progress` | 800×1000 | 17325 | <img src="wgpu/linux-lavapipe/gallery__progress.png" width="240"> |
| [ ] | `gallery/progress_dark` | 800×1000 | 18331 | <img src="wgpu/linux-lavapipe/gallery__progress_dark.png" width="240"> |
| [ ] | `gallery/progress_material` | 800×1000 | 19699 | <img src="wgpu/linux-lavapipe/gallery__progress_material.png" width="240"> |
| [ ] | `gallery/progress_tailwind` | 800×1000 | 19699 | <img src="wgpu/linux-lavapipe/gallery__progress_tailwind.png" width="240"> |
| [ ] | `gallery/radio_group` | 800×1000 | 48450 | <img src="wgpu/linux-lavapipe/gallery__radio_group.png" width="240"> |
| [ ] | `gallery/radio_group_dark` | 800×1000 | 50994 | <img src="wgpu/linux-lavapipe/gallery__radio_group_dark.png" width="240"> |
| [ ] | `gallery/radio_group_material` | 800×1000 | 48450 | <img src="wgpu/linux-lavapipe/gallery__radio_group_material.png" width="240"> |
| [ ] | `gallery/radio_group_tailwind` | 800×1000 | 48450 | <img src="wgpu/linux-lavapipe/gallery__radio_group_tailwind.png" width="240"> |
| [ ] | `gallery/reorderable_list_view` | 800×1000 | 25666 | <img src="wgpu/linux-lavapipe/gallery__reorderable_list_view.png" width="240"> |
| [ ] | `gallery/resizable` | 800×1000 | 20393 | <img src="wgpu/linux-lavapipe/gallery__resizable.png" width="240"> |
| [ ] | `gallery/rotated_box` | 800×1000 | 15786 | <img src="wgpu/linux-lavapipe/gallery__rotated_box.png" width="240"> |
| [ ] | `gallery/rotation_transition` | 800×1000 | 15169 | <img src="wgpu/linux-lavapipe/gallery__rotation_transition.png" width="240"> |
| [ ] | `gallery/routing` | 800×1000 | 21942 | <img src="wgpu/linux-lavapipe/gallery__routing.png" width="240"> |
| [ ] | `gallery/routing_dark` | 800×1000 | 23305 | <img src="wgpu/linux-lavapipe/gallery__routing_dark.png" width="240"> |
| [ ] | `gallery/routing_material` | 800×1000 | 27544 | <img src="wgpu/linux-lavapipe/gallery__routing_material.png" width="240"> |
| [ ] | `gallery/routing_tailwind` | 800×1000 | 22771 | <img src="wgpu/linux-lavapipe/gallery__routing_tailwind.png" width="240"> |
| [ ] | `gallery/rows` | 800×1000 | 33524 | <img src="wgpu/linux-lavapipe/gallery__rows.png" width="240"> |
| [ ] | `gallery/rows_rtl` | 800×1000 | 27419 | <img src="wgpu/linux-lavapipe/gallery__rows_rtl.png" width="240"> |
| [ ] | `gallery/safe_area` | 800×1000 | 10645 | <img src="wgpu/linux-lavapipe/gallery__safe_area.png" width="240"> |
| [ ] | `gallery/scaffold_drawer` | 800×1000 | 21247 | <img src="wgpu/linux-lavapipe/gallery__scaffold_drawer.png" width="240"> |
| [ ] | `gallery/scaffold_drawer_dark` | 800×1000 | 23037 | <img src="wgpu/linux-lavapipe/gallery__scaffold_drawer_dark.png" width="240"> |
| [ ] | `gallery/scaffold_drawer_material` | 800×1000 | 23536 | <img src="wgpu/linux-lavapipe/gallery__scaffold_drawer_material.png" width="240"> |
| [ ] | `gallery/scaffold_drawer_tailwind` | 800×1000 | 22107 | <img src="wgpu/linux-lavapipe/gallery__scaffold_drawer_tailwind.png" width="240"> |
| [ ] | `gallery/scaffold_slots` | 800×1000 | 23079 | <img src="wgpu/linux-lavapipe/gallery__scaffold_slots.png" width="240"> |
| [ ] | `gallery/scale_transition` | 800×1000 | 14961 | <img src="wgpu/linux-lavapipe/gallery__scale_transition.png" width="240"> |
| [ ] | `gallery/scroll_notification` | 800×1000 | 22127 | <img src="wgpu/linux-lavapipe/gallery__scroll_notification.png" width="240"> |
| [ ] | `gallery/scrolling` | 800×1000 | 39961 | <img src="wgpu/linux-lavapipe/gallery__scrolling.png" width="240"> |
| [ ] | `gallery/select` | 800×1000 | 37831 | <img src="wgpu/linux-lavapipe/gallery__select.png" width="240"> |
| [ ] | `gallery/select_dark` | 800×1000 | 40475 | <img src="wgpu/linux-lavapipe/gallery__select_dark.png" width="240"> |
| [ ] | `gallery/select_material` | 800×1000 | 42588 | <img src="wgpu/linux-lavapipe/gallery__select_material.png" width="240"> |
| [ ] | `gallery/select_tailwind` | 800×1000 | 40013 | <img src="wgpu/linux-lavapipe/gallery__select_tailwind.png" width="240"> |
| [ ] | `gallery/selectable_text` | 800×1000 | 29062 | <img src="wgpu/linux-lavapipe/gallery__selectable_text.png" width="240"> |
| [ ] | `gallery/semantics_combinators` | 800×1000 | 36072 | <img src="wgpu/linux-lavapipe/gallery__semantics_combinators.png" width="240"> |
| [ ] | `gallery/separator` | 800×1000 | 21753 | <img src="wgpu/linux-lavapipe/gallery__separator.png" width="240"> |
| [ ] | `gallery/separator_dark` | 800×1000 | 23071 | <img src="wgpu/linux-lavapipe/gallery__separator_dark.png" width="240"> |
| [ ] | `gallery/separator_material` | 800×1000 | 21753 | <img src="wgpu/linux-lavapipe/gallery__separator_material.png" width="240"> |
| [ ] | `gallery/separator_tailwind` | 800×1000 | 21753 | <img src="wgpu/linux-lavapipe/gallery__separator_tailwind.png" width="240"> |
| [ ] | `gallery/shader_mask` | 800×1000 | 60723 | <img src="wgpu/linux-lavapipe/gallery__shader_mask.png" width="240"> |
| [ ] | `gallery/sheets` | 800×1000 | 24553 | <img src="wgpu/linux-lavapipe/gallery__sheets.png" width="240"> |
| [ ] | `gallery/sheets_dark` | 800×1000 | 26488 | <img src="wgpu/linux-lavapipe/gallery__sheets_dark.png" width="240"> |
| [ ] | `gallery/sheets_material` | 800×1000 | 27908 | <img src="wgpu/linux-lavapipe/gallery__sheets_material.png" width="240"> |
| [ ] | `gallery/sheets_open` | 800×1000 | 30952 | <img src="wgpu/linux-lavapipe/gallery__sheets_open.png" width="240"> |
| [ ] | `gallery/sheets_tailwind` | 800×1000 | 26110 | <img src="wgpu/linux-lavapipe/gallery__sheets_tailwind.png" width="240"> |
| [ ] | `gallery/size_transition` | 800×1000 | 15056 | <img src="wgpu/linux-lavapipe/gallery__size_transition.png" width="240"> |
| [ ] | `gallery/sized_overflow_box` | 800×1000 | 15078 | <img src="wgpu/linux-lavapipe/gallery__sized_overflow_box.png" width="240"> |
| [ ] | `gallery/skeleton` | 800×1000 | 15325 | <img src="wgpu/linux-lavapipe/gallery__skeleton.png" width="240"> |
| [ ] | `gallery/skeleton_dark` | 800×1000 | 16244 | <img src="wgpu/linux-lavapipe/gallery__skeleton_dark.png" width="240"> |
| [ ] | `gallery/skeleton_material` | 800×1000 | 22896 | <img src="wgpu/linux-lavapipe/gallery__skeleton_material.png" width="240"> |
| [ ] | `gallery/skeleton_tailwind` | 800×1000 | 19619 | <img src="wgpu/linux-lavapipe/gallery__skeleton_tailwind.png" width="240"> |
| [ ] | `gallery/slide_transition` | 800×1000 | 14175 | <img src="wgpu/linux-lavapipe/gallery__slide_transition.png" width="240"> |
| [ ] | `gallery/slider` | 800×1000 | 22156 | <img src="wgpu/linux-lavapipe/gallery__slider.png" width="240"> |
| [ ] | `gallery/slider_dark` | 800×1000 | 22083 | <img src="wgpu/linux-lavapipe/gallery__slider_dark.png" width="240"> |
| [ ] | `gallery/slider_material` | 800×1000 | 22156 | <img src="wgpu/linux-lavapipe/gallery__slider_material.png" width="240"> |
| [ ] | `gallery/slider_rtl` | 800×1000 | 22058 | <img src="wgpu/linux-lavapipe/gallery__slider_rtl.png" width="240"> |
| [ ] | `gallery/slider_tailwind` | 800×1000 | 22156 | <img src="wgpu/linux-lavapipe/gallery__slider_tailwind.png" width="240"> |
| [ ] | `gallery/split_view` | 800×1000 | 30475 | <img src="wgpu/linux-lavapipe/gallery__split_view.png" width="240"> |
| [ ] | `gallery/stacks` | 800×1000 | 53959 | <img src="wgpu/linux-lavapipe/gallery__stacks.png" width="240"> |
| [ ] | `gallery/stepper` | 800×1000 | 24240 | <img src="wgpu/linux-lavapipe/gallery__stepper.png" width="240"> |
| [ ] | `gallery/stepper_dark` | 800×1000 | 25457 | <img src="wgpu/linux-lavapipe/gallery__stepper_dark.png" width="240"> |
| [ ] | `gallery/stepper_material` | 800×1000 | 27049 | <img src="wgpu/linux-lavapipe/gallery__stepper_material.png" width="240"> |
| [ ] | `gallery/stepper_tailwind` | 800×1000 | 24680 | <img src="wgpu/linux-lavapipe/gallery__stepper_tailwind.png" width="240"> |
| [ ] | `gallery/stream_builder` | 800×1000 | 17378 | <img src="wgpu/linux-lavapipe/gallery__stream_builder.png" width="240"> |
| [ ] | `gallery/styling` | 800×1000 | 124818 | <img src="wgpu/linux-lavapipe/gallery__styling.png" width="240"> |
| [ ] | `gallery/table_layout` | 800×1000 | 17764 | <img src="wgpu/linux-lavapipe/gallery__table_layout.png" width="240"> |
| [ ] | `gallery/tabs` | 800×1000 | 45878 | <img src="wgpu/linux-lavapipe/gallery__tabs.png" width="240"> |
| [ ] | `gallery/tabs_dark` | 800×1000 | 48381 | <img src="wgpu/linux-lavapipe/gallery__tabs_dark.png" width="240"> |
| [ ] | `gallery/tabs_material` | 800×1000 | 48264 | <img src="wgpu/linux-lavapipe/gallery__tabs_material.png" width="240"> |
| [ ] | `gallery/tabs_rtl` | 800×1000 | 45407 | <img src="wgpu/linux-lavapipe/gallery__tabs_rtl.png" width="240"> |
| [ ] | `gallery/tabs_tailwind` | 800×1000 | 48030 | <img src="wgpu/linux-lavapipe/gallery__tabs_tailwind.png" width="240"> |
| [ ] | `gallery/text_fields` | 800×1000 | 50838 | <img src="wgpu/linux-lavapipe/gallery__text_fields.png" width="240"> |
| [ ] | `gallery/text_fields_dark` | 800×1000 | 53362 | <img src="wgpu/linux-lavapipe/gallery__text_fields_dark.png" width="240"> |
| [ ] | `gallery/text_fields_material` | 800×1000 | 55692 | <img src="wgpu/linux-lavapipe/gallery__text_fields_material.png" width="240"> |
| [ ] | `gallery/text_fields_rtl` | 800×1000 | 47463 | <img src="wgpu/linux-lavapipe/gallery__text_fields_rtl.png" width="240"> |
| [ ] | `gallery/text_fields_tailwind` | 800×1000 | 50900 | <img src="wgpu/linux-lavapipe/gallery__text_fields_tailwind.png" width="240"> |
| [ ] | `gallery/toasts` | 800×1000 | 19530 | <img src="wgpu/linux-lavapipe/gallery__toasts.png" width="240"> |
| [ ] | `gallery/toasts_dark` | 800×1000 | 20940 | <img src="wgpu/linux-lavapipe/gallery__toasts_dark.png" width="240"> |
| [ ] | `gallery/toasts_material` | 800×1000 | 24288 | <img src="wgpu/linux-lavapipe/gallery__toasts_material.png" width="240"> |
| [ ] | `gallery/toasts_open` | 800×1000 | 23457 | <img src="wgpu/linux-lavapipe/gallery__toasts_open.png" width="240"> |
| [ ] | `gallery/toasts_tailwind` | 800×1000 | 21245 | <img src="wgpu/linux-lavapipe/gallery__toasts_tailwind.png" width="240"> |
| [ ] | `gallery/toggles` | 800×1000 | 71489 | <img src="wgpu/linux-lavapipe/gallery__toggles.png" width="240"> |
| [ ] | `gallery/toggles_dark` | 800×1000 | 74931 | <img src="wgpu/linux-lavapipe/gallery__toggles_dark.png" width="240"> |
| [ ] | `gallery/toggles_material` | 800×1000 | 77161 | <img src="wgpu/linux-lavapipe/gallery__toggles_material.png" width="240"> |
| [ ] | `gallery/toggles_rtl` | 800×1000 | 69362 | <img src="wgpu/linux-lavapipe/gallery__toggles_rtl.png" width="240"> |
| [ ] | `gallery/toggles_tailwind` | 800×1000 | 74040 | <img src="wgpu/linux-lavapipe/gallery__toggles_tailwind.png" width="240"> |
| [ ] | `gallery/toolbar` | 800×1000 | 16555 | <img src="wgpu/linux-lavapipe/gallery__toolbar.png" width="240"> |
| [ ] | `gallery/toolbar_dark` | 800×1000 | 17263 | <img src="wgpu/linux-lavapipe/gallery__toolbar_dark.png" width="240"> |
| [ ] | `gallery/toolbar_material` | 800×1000 | 19913 | <img src="wgpu/linux-lavapipe/gallery__toolbar_material.png" width="240"> |
| [ ] | `gallery/toolbar_tailwind` | 800×1000 | 17125 | <img src="wgpu/linux-lavapipe/gallery__toolbar_tailwind.png" width="240"> |
| [ ] | `gallery/tooltips` | 800×1000 | 22941 | <img src="wgpu/linux-lavapipe/gallery__tooltips.png" width="240"> |
| [ ] | `gallery/tooltips_dark` | 800×1000 | 24613 | <img src="wgpu/linux-lavapipe/gallery__tooltips_dark.png" width="240"> |
| [ ] | `gallery/tooltips_material` | 800×1000 | 25823 | <img src="wgpu/linux-lavapipe/gallery__tooltips_material.png" width="240"> |
| [ ] | `gallery/tooltips_tailwind` | 800×1000 | 24318 | <img src="wgpu/linux-lavapipe/gallery__tooltips_tailwind.png" width="240"> |
| [ ] | `gallery/tooltips_open` | 800×1000 | 25145 | <img src="wgpu/linux-lavapipe/gallery__tooltips_open.png" width="240"> |
| [ ] | `gallery/typography` | 800×1000 | 98727 | <img src="wgpu/linux-lavapipe/gallery__typography.png" width="240"> |
| [ ] | `gallery/unconstrained_box` | 800×1000 | 13747 | <img src="wgpu/linux-lavapipe/gallery__unconstrained_box.png" width="240"> |
| [ ] | `gallery/visibility` | 800×1000 | 19192 | <img src="wgpu/linux-lavapipe/gallery__visibility.png" width="240"> |
| [ ] | `gallery/wraps` | 800×1000 | 49938 | <img src="wgpu/linux-lavapipe/gallery__wraps.png" width="240"> |

## grad

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `grad/brush_transform_rotate30` | 128×128 | 6912 | <img src="wgpu/linux-lavapipe/grad__brush_transform_rotate30.png" width="128"> |
| [ ] | `grad/decoration_gradient_rrect` | 160×120 | 5830 | <img src="wgpu/linux-lavapipe/grad__decoration_gradient_rrect.png" width="160"> |
| [ ] | `grad/degenerate` | 128×128 | 360 | <img src="wgpu/linux-lavapipe/grad__degenerate.png" width="128"> |
| [ ] | `grad/gradient_text` | 200×64 | 4650 | <img src="wgpu/linux-lavapipe/grad__gradient_text.png" width="200"> |
| [ ] | `grad/hard_stops` | 128×128 | 300 | <img src="wgpu/linux-lavapipe/grad__hard_stops.png" width="128"> |
| [ ] | `grad/interpolation_cs` | 128×128 | 1487 | <img src="wgpu/linux-lavapipe/grad__interpolation_cs.png" width="128"> |
| [ ] | `grad/linear_alpha_stops` | 128×128 | 946 | <img src="wgpu/linux-lavapipe/grad__linear_alpha_stops.png" width="128"> |
| [ ] | `grad/linear_multistop` | 128×128 | 1372 | <img src="wgpu/linux-lavapipe/grad__linear_multistop.png" width="128"> |
| [ ] | `grad/linear_pad` | 128×128 | 1120 | <img src="wgpu/linux-lavapipe/grad__linear_pad.png" width="128"> |
| [ ] | `grad/linear_reflect` | 128×128 | 1284 | <img src="wgpu/linux-lavapipe/grad__linear_reflect.png" width="128"> |
| [ ] | `grad/linear_repeat` | 128×128 | 927 | <img src="wgpu/linux-lavapipe/grad__linear_repeat.png" width="128"> |
| [ ] | `grad/radial_focal_on_circle` | 128×128 | 5745 | <img src="wgpu/linux-lavapipe/grad__radial_focal_on_circle.png" width="128"> |
| [ ] | `grad/radial_pad` | 128×128 | 3067 | <img src="wgpu/linux-lavapipe/grad__radial_pad.png" width="128"> |
| [ ] | `grad/radial_reflect` | 128×128 | 15103 | <img src="wgpu/linux-lavapipe/grad__radial_reflect.png" width="128"> |
| [ ] | `grad/radial_repeat` | 128×128 | 14863 | <img src="wgpu/linux-lavapipe/grad__radial_repeat.png" width="128"> |
| [ ] | `grad/radial_strip` | 128×128 | 2390 | <img src="wgpu/linux-lavapipe/grad__radial_strip.png" width="128"> |
| [ ] | `grad/stroke_gradient` | 128×128 | 4907 | <img src="wgpu/linux-lavapipe/grad__stroke_gradient.png" width="128"> |
| [ ] | `grad/sweep_negative_start` | 128×128 | 7701 | <img src="wgpu/linux-lavapipe/grad__sweep_negative_start.png" width="128"> |
| [ ] | `grad/sweep_pad` | 128×128 | 6048 | <img src="wgpu/linux-lavapipe/grad__sweep_pad.png" width="128"> |
| [ ] | `grad/sweep_reflect` | 128×128 | 10890 | <img src="wgpu/linux-lavapipe/grad__sweep_reflect.png" width="128"> |
| [ ] | `grad/sweep_repeat` | 128×128 | 10746 | <img src="wgpu/linux-lavapipe/grad__sweep_repeat.png" width="128"> |
| [ ] | `grad/two_point_pad` | 128×128 | 5775 | <img src="wgpu/linux-lavapipe/grad__two_point_pad.png" width="128"> |
| [ ] | `grad/two_point_reflect` | 128×128 | 14280 | <img src="wgpu/linux-lavapipe/grad__two_point_reflect.png" width="128"> |
| [ ] | `grad/two_point_repeat` | 128×128 | 14477 | <img src="wgpu/linux-lavapipe/grad__two_point_repeat.png" width="128"> |
| [ ] | `grad/unpremul_alpha` | 128×128 | 1716 | <img src="wgpu/linux-lavapipe/grad__unpremul_alpha.png" width="128"> |

## img

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `img/aligned_1x` | 128×128 | 392 | <img src="wgpu/linux-lavapipe/img__aligned_1x.png" width="128"> |
| [ ] | `img/bgra8_source` | 128×128 | 25733 | <img src="wgpu/linux-lavapipe/img__bgra8_source.png" width="128"> |
| [ ] | `img/brush_alpha_0_5` | 128×128 | 391 | <img src="wgpu/linux-lavapipe/img__brush_alpha_0_5.png" width="128"> |
| [ ] | `img/brush_fill_alpha_0_5` | 64×64 | 196 | <img src="wgpu/linux-lavapipe/img__brush_fill_alpha_0_5.png" width="64"> |
| [ ] | `img/brush_fill_pad` | 64×64 | 177 | <img src="wgpu/linux-lavapipe/img__brush_fill_pad.png" width="64"> |
| [ ] | `img/brush_fill_reflect` | 64×64 | 194 | <img src="wgpu/linux-lavapipe/img__brush_fill_reflect.png" width="64"> |
| [ ] | `img/brush_fill_repeat` | 64×64 | 198 | <img src="wgpu/linux-lavapipe/img__brush_fill_repeat.png" width="64"> |
| [ ] | `img/brush_stroke` | 64×64 | 691 | <img src="wgpu/linux-lavapipe/img__brush_stroke.png" width="64"> |
| [ ] | `img/brush_transform_rotate` | 64×64 | 914 | <img src="wgpu/linux-lavapipe/img__brush_transform_rotate.png" width="64"> |
| [ ] | `img/dynamic_update` | 128×128 | 25705 | <img src="wgpu/linux-lavapipe/img__dynamic_update.png" width="128"> |
| [ ] | `img/edge_sampling` | 128×128 | 8436 | <img src="wgpu/linux-lavapipe/img__edge_sampling.png" width="128"> |
| [ ] | `img/fit_cover_rrect` | 200×120 | 1411 | <img src="wgpu/linux-lavapipe/img__fit_cover_rrect.png" width="200"> |
| [ ] | `img/many_200_distinct` | 256×256 | 183002 | <img src="wgpu/linux-lavapipe/img__many_200_distinct.png" width="240"> |
| [ ] | `img/minify_checker_high` | 128×128 | 323 | <img src="wgpu/linux-lavapipe/img__minify_checker_high.png" width="128"> |
| [ ] | `img/minify_checker_medium` | 128×128 | 323 | <img src="wgpu/linux-lavapipe/img__minify_checker_medium.png" width="128"> |
| [ ] | `img/nine_slice_button` | 128×128 | 394 | <img src="wgpu/linux-lavapipe/img__nine_slice_button.png" width="128"> |
| [ ] | `img/nonuniform_scale` | 128×128 | 13869 | <img src="wgpu/linux-lavapipe/img__nonuniform_scale.png" width="128"> |
| [ ] | `img/premultiplied_alpha` | 128×128 | 23477 | <img src="wgpu/linux-lavapipe/img__premultiplied_alpha.png" width="128"> |
| [ ] | `img/quality_low_nearest` | 128×128 | 762 | <img src="wgpu/linux-lavapipe/img__quality_low_nearest.png" width="128"> |
| [ ] | `img/rotated_30` | 128×128 | 6152 | <img src="wgpu/linux-lavapipe/img__rotated_30.png" width="128"> |
| [ ] | `img/scaled_down_quarter` | 128×128 | 412 | <img src="wgpu/linux-lavapipe/img__scaled_down_quarter.png" width="128"> |
| [ ] | `img/scaled_up_3x` | 128×128 | 6698 | <img src="wgpu/linux-lavapipe/img__scaled_up_3x.png" width="128"> |
| [ ] | `img/scaled_up_3x_s125` | 160×160 | 10097 | <img src="wgpu/linux-lavapipe/img__scaled_up_3x_s125.png" width="160"> |
| [ ] | `img/scaled_up_3x_s150` | 192×192 | 14034 | <img src="wgpu/linux-lavapipe/img__scaled_up_3x_s150.png" width="192"> |
| [ ] | `img/scaled_up_3x_s175` | 224×224 | 17963 | <img src="wgpu/linux-lavapipe/img__scaled_up_3x_s175.png" width="224"> |
| [ ] | `img/scaled_up_3x_s200` | 256×256 | 22776 | <img src="wgpu/linux-lavapipe/img__scaled_up_3x_s200.png" width="240"> |
| [ ] | `img/straight_alpha` | 128×128 | 23477 | <img src="wgpu/linux-lavapipe/img__straight_alpha.png" width="128"> |
| [ ] | `img/under_clip_and_opacity` | 128×128 | 1221 | <img src="wgpu/linux-lavapipe/img__under_clip_and_opacity.png" width="128"> |

## layer

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `layer/blend_color` | 128×96 | 432 | <img src="wgpu/linux-lavapipe/layer__blend_color.png" width="128"> |
| [ ] | `layer/blend_color_burn` | 128×96 | 422 | <img src="wgpu/linux-lavapipe/layer__blend_color_burn.png" width="128"> |
| [ ] | `layer/blend_color_dodge` | 128×96 | 490 | <img src="wgpu/linux-lavapipe/layer__blend_color_dodge.png" width="128"> |
| [ ] | `layer/blend_darken` | 128×96 | 398 | <img src="wgpu/linux-lavapipe/layer__blend_darken.png" width="128"> |
| [ ] | `layer/blend_difference` | 128×96 | 492 | <img src="wgpu/linux-lavapipe/layer__blend_difference.png" width="128"> |
| [ ] | `layer/blend_exclusion` | 128×96 | 491 | <img src="wgpu/linux-lavapipe/layer__blend_exclusion.png" width="128"> |
| [ ] | `layer/blend_hard_light` | 128×96 | 496 | <img src="wgpu/linux-lavapipe/layer__blend_hard_light.png" width="128"> |
| [ ] | `layer/blend_hue` | 128×96 | 433 | <img src="wgpu/linux-lavapipe/layer__blend_hue.png" width="128"> |
| [ ] | `layer/blend_in_sdf_clip` | 128×96 | 1118 | <img src="wgpu/linux-lavapipe/layer__blend_in_sdf_clip.png" width="128"> |
| [ ] | `layer/blend_inside_clip` | 128×96 | 284 | <img src="wgpu/linux-lavapipe/layer__blend_inside_clip.png" width="128"> |
| [ ] | `layer/blend_lighten` | 128×96 | 461 | <img src="wgpu/linux-lavapipe/layer__blend_lighten.png" width="128"> |
| [ ] | `layer/blend_luminosity` | 128×96 | 474 | <img src="wgpu/linux-lavapipe/layer__blend_luminosity.png" width="128"> |
| [ ] | `layer/blend_multiply` | 128×96 | 432 | <img src="wgpu/linux-lavapipe/layer__blend_multiply.png" width="128"> |
| [ ] | `layer/blend_normal` | 128×96 | 428 | <img src="wgpu/linux-lavapipe/layer__blend_normal.png" width="128"> |
| [ ] | `layer/blend_overlay` | 128×96 | 510 | <img src="wgpu/linux-lavapipe/layer__blend_overlay.png" width="128"> |
| [ ] | `layer/blend_saturation` | 128×96 | 456 | <img src="wgpu/linux-lavapipe/layer__blend_saturation.png" width="128"> |
| [ ] | `layer/blend_screen` | 128×96 | 475 | <img src="wgpu/linux-lavapipe/layer__blend_screen.png" width="128"> |
| [ ] | `layer/blend_soft_light` | 128×96 | 507 | <img src="wgpu/linux-lavapipe/layer__blend_soft_light.png" width="128"> |
| [ ] | `layer/color_filtered` | 160×120 | 343 | <img src="wgpu/linux-lavapipe/layer__color_filtered.png" width="160"> |
| [ ] | `layer/compose_clear` | 128×96 | 242 | <img src="wgpu/linux-lavapipe/layer__compose_clear.png" width="128"> |
| [ ] | `layer/compose_copy` | 128×96 | 1424 | <img src="wgpu/linux-lavapipe/layer__compose_copy.png" width="128"> |
| [ ] | `layer/compose_dest` | 128×96 | 255 | <img src="wgpu/linux-lavapipe/layer__compose_dest.png" width="128"> |
| [ ] | `layer/compose_dest_atop` | 128×96 | 1721 | <img src="wgpu/linux-lavapipe/layer__compose_dest_atop.png" width="128"> |
| [ ] | `layer/compose_dest_in` | 128×96 | 1001 | <img src="wgpu/linux-lavapipe/layer__compose_dest_in.png" width="128"> |
| [ ] | `layer/compose_dest_out` | 128×96 | 1035 | <img src="wgpu/linux-lavapipe/layer__compose_dest_out.png" width="128"> |
| [ ] | `layer/compose_dest_over` | 128×96 | 1098 | <img src="wgpu/linux-lavapipe/layer__compose_dest_over.png" width="128"> |
| [ ] | `layer/compose_plus` | 128×96 | 1779 | <img src="wgpu/linux-lavapipe/layer__compose_plus.png" width="128"> |
| [ ] | `layer/compose_plus_lighter` | 128×96 | 1779 | <img src="wgpu/linux-lavapipe/layer__compose_plus_lighter.png" width="128"> |
| [ ] | `layer/compose_src_atop` | 128×96 | 1058 | <img src="wgpu/linux-lavapipe/layer__compose_src_atop.png" width="128"> |
| [ ] | `layer/compose_src_in` | 128×96 | 1038 | <img src="wgpu/linux-lavapipe/layer__compose_src_in.png" width="128"> |
| [ ] | `layer/compose_src_out` | 128×96 | 1068 | <img src="wgpu/linux-lavapipe/layer__compose_src_out.png" width="128"> |
| [ ] | `layer/compose_src_over` | 128×96 | 1793 | <img src="wgpu/linux-lavapipe/layer__compose_src_over.png" width="128"> |
| [ ] | `layer/compose_xor` | 128×96 | 1783 | <img src="wgpu/linux-lavapipe/layer__compose_xor.png" width="128"> |
| [ ] | `layer/decoration_blend` | 160×120 | 650 | <img src="wgpu/linux-lavapipe/layer__decoration_blend.png" width="160"> |
| [ ] | `layer/fragment_append` | 128×96 | 573 | <img src="wgpu/linux-lavapipe/layer__fragment_append.png" width="128"> |
| [ ] | `layer/opacity_0_5` | 128×96 | 309 | <img src="wgpu/linux-lavapipe/layer__opacity_0_5.png" width="128"> |
| [ ] | `layer/opacity_clip` | 128×96 | 355 | <img src="wgpu/linux-lavapipe/layer__opacity_clip.png" width="128"> |
| [ ] | `layer/opacity_fold_single` | 128×96 | 736 | <img src="wgpu/linux-lavapipe/layer__opacity_fold_single.png" width="128"> |
| [ ] | `layer/opacity_nested` | 128×96 | 366 | <img src="wgpu/linux-lavapipe/layer__opacity_nested.png" width="128"> |

## mesh

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `mesh/strip_fan` | 128×96 | 527 | <img src="wgpu/linux-lavapipe/mesh__strip_fan.png" width="128"> |
| [ ] | `mesh/textured` | 128×96 | 3308 | <img src="wgpu/linux-lavapipe/mesh__textured.png" width="128"> |
| [ ] | `mesh/vertex_colors` | 128×96 | 1886 | <img src="wgpu/linux-lavapipe/mesh__vertex_colors.png" width="128"> |

## prim

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `prim/circles` | 128×128 | 1988 | <img src="wgpu/linux-lavapipe/prim__circles.png" width="128"> |
| [ ] | `prim/circles_s125` | 160×160 | 2731 | <img src="wgpu/linux-lavapipe/prim__circles_s125.png" width="160"> |
| [ ] | `prim/circles_s150` | 192×192 | 3472 | <img src="wgpu/linux-lavapipe/prim__circles_s150.png" width="192"> |
| [ ] | `prim/circles_s175` | 224×224 | 4213 | <img src="wgpu/linux-lavapipe/prim__circles_s175.png" width="224"> |
| [ ] | `prim/circles_s200` | 256×256 | 4960 | <img src="wgpu/linux-lavapipe/prim__circles_s200.png" width="240"> |
| [ ] | `prim/curves` | 128×128 | 1805 | <img src="wgpu/linux-lavapipe/prim__curves.png" width="128"> |
| [ ] | `prim/donut_evenodd` | 128×128 | 2588 | <img src="wgpu/linux-lavapipe/prim__donut_evenodd.png" width="128"> |
| [ ] | `prim/donut_nonzero` | 128×128 | 1972 | <img src="wgpu/linux-lavapipe/prim__donut_nonzero.png" width="128"> |
| [ ] | `prim/ellipse_rotated` | 128×128 | 1703 | <img src="wgpu/linux-lavapipe/prim__ellipse_rotated.png" width="128"> |
| [ ] | `prim/icons` | 256×160 | 7090 | <img src="wgpu/linux-lavapipe/prim__icons.png" width="240"> |
| [ ] | `prim/many_subpaths` | 128×128 | 391 | <img src="wgpu/linux-lavapipe/prim__many_subpaths.png" width="128"> |
| [ ] | `prim/rect_aligned` | 128×128 | 319 | <img src="wgpu/linux-lavapipe/prim__rect_aligned.png" width="128"> |
| [ ] | `prim/rect_fractional` | 128×128 | 319 | <img src="wgpu/linux-lavapipe/prim__rect_fractional.png" width="128"> |
| [ ] | `prim/rect_translucent_overlap` | 128×128 | 342 | <img src="wgpu/linux-lavapipe/prim__rect_translucent_overlap.png" width="128"> |
| [ ] | `prim/rrect_per_corner` | 128×128 | 1096 | <img src="wgpu/linux-lavapipe/prim__rrect_per_corner.png" width="128"> |
| [ ] | `prim/rrect_radii` | 128×128 | 1549 | <img src="wgpu/linux-lavapipe/prim__rrect_radii.png" width="128"> |
| [ ] | `prim/slivers` | 128×128 | 312 | <img src="wgpu/linux-lavapipe/prim__slivers.png" width="128"> |
| [ ] | `prim/star_evenodd` | 128×128 | 2525 | <img src="wgpu/linux-lavapipe/prim__star_evenodd.png" width="128"> |
| [ ] | `prim/star_nonzero` | 128×128 | 2126 | <img src="wgpu/linux-lavapipe/prim__star_nonzero.png" width="128"> |

## screen

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `screen/app_shell` | 1200×800 | 102714 | <img src="wgpu/linux-lavapipe/screen__app_shell.png" width="240"> |
| [ ] | `screen/charts_like` | 900×600 | 24308 | <img src="wgpu/linux-lavapipe/screen__charts_like.png" width="240"> |
| [ ] | `screen/code_editor_like` | 900×600 | 57970 | <img src="wgpu/linux-lavapipe/screen__code_editor_like.png" width="240"> |
| [ ] | `screen/dashboard_1200x800` | 1200×800 | 191874 | <img src="wgpu/linux-lavapipe/screen__dashboard_1200x800.png" width="240"> |
| [ ] | `screen/flutter_parity_article` | 800×1000 | 55716 | <img src="wgpu/linux-lavapipe/screen__flutter_parity_article.png" width="240"> |
| [ ] | `screen/flutter_parity_card_stack` | 900×600 | 94526 | <img src="wgpu/linux-lavapipe/screen__flutter_parity_card_stack.png" width="240"> |
| [ ] | `screen/flutter_parity_frosted_app_bar` | 900×700 | 55266 | <img src="wgpu/linux-lavapipe/screen__flutter_parity_frosted_app_bar.png" width="240"> |
| [ ] | `screen/form` | 600×700 | 18682 | <img src="wgpu/linux-lavapipe/screen__form.png" width="240"> |
| [ ] | `screen/landing_like` | 1200×860 | 118328 | <img src="wgpu/linux-lavapipe/screen__landing_like.png" width="240"> |
| [ ] | `screen/markdown_like` | 900×1000 | 32335 | <img src="wgpu/linux-lavapipe/screen__markdown_like.png" width="240"> |

## shadow

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `shadow/blur_64` | 256×256 | 8401 | <img src="wgpu/linux-lavapipe/shadow__blur_64.png" width="240"> |
| [ ] | `shadow/blur_min` | 128×128 | 372 | <img src="wgpu/linux-lavapipe/shadow__blur_min.png" width="128"> |
| [ ] | `shadow/blurred_rect_basic` | 128×128 | 2240 | <img src="wgpu/linux-lavapipe/shadow__blurred_rect_basic.png" width="128"> |
| [ ] | `shadow/elevation_card` | 200×200 | 2334 | <img src="wgpu/linux-lavapipe/shadow__elevation_card.png" width="200"> |
| [ ] | `shadow/elevation_card_s125` | 250×250 | 3200 | <img src="wgpu/linux-lavapipe/shadow__elevation_card_s125.png" width="240"> |
| [ ] | `shadow/elevation_card_s150` | 300×300 | 4076 | <img src="wgpu/linux-lavapipe/shadow__elevation_card_s150.png" width="240"> |
| [ ] | `shadow/elevation_card_s175` | 350×350 | 4898 | <img src="wgpu/linux-lavapipe/shadow__elevation_card_s175.png" width="240"> |
| [ ] | `shadow/elevation_card_s200` | 400×400 | 5661 | <img src="wgpu/linux-lavapipe/shadow__elevation_card_s200.png" width="240"> |
| [ ] | `shadow/inside_scroll_clip` | 200×200 | 3278 | <img src="wgpu/linux-lavapipe/shadow__inside_scroll_clip.png" width="200"> |
| [ ] | `shadow/overlapping_cards` | 200×200 | 3747 | <img src="wgpu/linux-lavapipe/shadow__overlapping_cards.png" width="200"> |
| [ ] | `shadow/path_blur_styles` | 256×128 | 4236 | <img src="wgpu/linux-lavapipe/shadow__path_blur_styles.png" width="240"> |
| [ ] | `shadow/path_star` | 128×128 | 3420 | <img src="wgpu/linux-lavapipe/shadow__path_star.png" width="128"> |
| [ ] | `shadow/radius_gt_half` | 128×128 | 2988 | <img src="wgpu/linux-lavapipe/shadow__radius_gt_half.png" width="128"> |
| [ ] | `shadow/translated_scaled` | 128×128 | 3265 | <img src="wgpu/linux-lavapipe/shadow__translated_scaled.png" width="128"> |

## sprite

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `sprite/grid_1000` | 128×96 | 773 | <img src="wgpu/linux-lavapipe/sprite__grid_1000.png" width="128"> |
| [ ] | `sprite/rotated_scaled` | 128×96 | 4240 | <img src="wgpu/linux-lavapipe/sprite__rotated_scaled.png" width="128"> |

## stress

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `stress/clips_8deep_x100` | 1920×1080 | 42139 | <img src="wgpu/linux-lavapipe/stress__clips_8deep_x100.png" width="240"> |
| [ ] | `stress/hairline_grid_dense` | 1920×1080 | 14020 | <img src="wgpu/linux-lavapipe/stress__hairline_grid_dense.png" width="240"> |
| [ ] | `stress/huge_path_100k_segments` | 1920×1080 | 32635 | <img src="wgpu/linux-lavapipe/stress__huge_path_100k_segments.png" width="240"> |
| [ ] | `stress/rects_10k` | 1920×1080 | 205931 | <img src="wgpu/linux-lavapipe/stress__rects_10k.png" width="240"> |

## stroke

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `stroke/caps` | 128×128 | 526 | <img src="wgpu/linux-lavapipe/stroke__caps.png" width="128"> |
| [ ] | `stroke/caps_s125` | 160×160 | 809 | <img src="wgpu/linux-lavapipe/stroke__caps_s125.png" width="160"> |
| [ ] | `stroke/caps_s150` | 192×192 | 846 | <img src="wgpu/linux-lavapipe/stroke__caps_s150.png" width="192"> |
| [ ] | `stroke/caps_s175` | 224×224 | 1210 | <img src="wgpu/linux-lavapipe/stroke__caps_s175.png" width="224"> |
| [ ] | `stroke/caps_s200` | 256×256 | 1379 | <img src="wgpu/linux-lavapipe/stroke__caps_s200.png" width="240"> |
| [ ] | `stroke/closed_rrect_border` | 128×128 | 1060 | <img src="wgpu/linux-lavapipe/stroke__closed_rrect_border.png" width="128"> |
| [ ] | `stroke/closed_rrect_border_s125` | 160×160 | 1329 | <img src="wgpu/linux-lavapipe/stroke__closed_rrect_border_s125.png" width="160"> |
| [ ] | `stroke/closed_rrect_border_s150` | 192×192 | 1801 | <img src="wgpu/linux-lavapipe/stroke__closed_rrect_border_s150.png" width="192"> |
| [ ] | `stroke/closed_rrect_border_s175` | 224×224 | 2322 | <img src="wgpu/linux-lavapipe/stroke__closed_rrect_border_s175.png" width="224"> |
| [ ] | `stroke/closed_rrect_border_s200` | 256×256 | 2745 | <img src="wgpu/linux-lavapipe/stroke__closed_rrect_border_s200.png" width="240"> |
| [ ] | `stroke/curve` | 128×128 | 2268 | <img src="wgpu/linux-lavapipe/stroke__curve.png" width="128"> |
| [ ] | `stroke/curve_s125` | 160×160 | 2905 | <img src="wgpu/linux-lavapipe/stroke__curve_s125.png" width="160"> |
| [ ] | `stroke/curve_s150` | 192×192 | 3591 | <img src="wgpu/linux-lavapipe/stroke__curve_s150.png" width="192"> |
| [ ] | `stroke/curve_s175` | 224×224 | 4342 | <img src="wgpu/linux-lavapipe/stroke__curve_s175.png" width="224"> |
| [ ] | `stroke/curve_s200` | 256×256 | 4969 | <img src="wgpu/linux-lavapipe/stroke__curve_s200.png" width="240"> |
| [ ] | `stroke/dashes` | 128×128 | 3871 | <img src="wgpu/linux-lavapipe/stroke__dashes.png" width="128"> |
| [ ] | `stroke/dashes_s125` | 160×160 | 5810 | <img src="wgpu/linux-lavapipe/stroke__dashes_s125.png" width="160"> |
| [ ] | `stroke/dashes_s150` | 192×192 | 6834 | <img src="wgpu/linux-lavapipe/stroke__dashes_s150.png" width="192"> |
| [ ] | `stroke/dashes_s175` | 224×224 | 8868 | <img src="wgpu/linux-lavapipe/stroke__dashes_s175.png" width="224"> |
| [ ] | `stroke/dashes_s200` | 256×256 | 9029 | <img src="wgpu/linux-lavapipe/stroke__dashes_s200.png" width="240"> |
| [ ] | `stroke/hairline_grid` | 128×128 | 433 | <img src="wgpu/linux-lavapipe/stroke__hairline_grid.png" width="128"> |
| [ ] | `stroke/hairline_grid_s125` | 160×160 | 619 | <img src="wgpu/linux-lavapipe/stroke__hairline_grid_s125.png" width="160"> |
| [ ] | `stroke/hairline_grid_s150` | 192×192 | 694 | <img src="wgpu/linux-lavapipe/stroke__hairline_grid_s150.png" width="192"> |
| [ ] | `stroke/hairline_grid_s175` | 224×224 | 923 | <img src="wgpu/linux-lavapipe/stroke__hairline_grid_s175.png" width="224"> |
| [ ] | `stroke/hairline_grid_s200` | 256×256 | 1004 | <img src="wgpu/linux-lavapipe/stroke__hairline_grid_s200.png" width="240"> |
| [ ] | `stroke/joins` | 128×128 | 1680 | <img src="wgpu/linux-lavapipe/stroke__joins.png" width="128"> |
| [ ] | `stroke/joins_s125` | 160×160 | 2027 | <img src="wgpu/linux-lavapipe/stroke__joins_s125.png" width="160"> |
| [ ] | `stroke/joins_s150` | 192×192 | 3714 | <img src="wgpu/linux-lavapipe/stroke__joins_s150.png" width="192"> |
| [ ] | `stroke/joins_s175` | 224×224 | 4582 | <img src="wgpu/linux-lavapipe/stroke__joins_s175.png" width="224"> |
| [ ] | `stroke/joins_s200` | 256×256 | 5148 | <img src="wgpu/linux-lavapipe/stroke__joins_s200.png" width="240"> |
| [ ] | `stroke/miter_limit` | 128×128 | 1636 | <img src="wgpu/linux-lavapipe/stroke__miter_limit.png" width="128"> |
| [ ] | `stroke/miter_limit_s125` | 160×160 | 2029 | <img src="wgpu/linux-lavapipe/stroke__miter_limit_s125.png" width="160"> |
| [ ] | `stroke/miter_limit_s150` | 192×192 | 4193 | <img src="wgpu/linux-lavapipe/stroke__miter_limit_s150.png" width="192"> |
| [ ] | `stroke/miter_limit_s175` | 224×224 | 5280 | <img src="wgpu/linux-lavapipe/stroke__miter_limit_s175.png" width="224"> |
| [ ] | `stroke/miter_limit_s200` | 256×256 | 6039 | <img src="wgpu/linux-lavapipe/stroke__miter_limit_s200.png" width="240"> |
| [ ] | `stroke/under_nonuniform_scale` | 128×128 | 1070 | <img src="wgpu/linux-lavapipe/stroke__under_nonuniform_scale.png" width="128"> |
| [ ] | `stroke/under_nonuniform_scale_s125` | 160×160 | 1347 | <img src="wgpu/linux-lavapipe/stroke__under_nonuniform_scale_s125.png" width="160"> |
| [ ] | `stroke/under_nonuniform_scale_s150` | 192×192 | 1601 | <img src="wgpu/linux-lavapipe/stroke__under_nonuniform_scale_s150.png" width="192"> |
| [ ] | `stroke/under_nonuniform_scale_s175` | 224×224 | 2091 | <img src="wgpu/linux-lavapipe/stroke__under_nonuniform_scale_s175.png" width="224"> |
| [ ] | `stroke/under_nonuniform_scale_s200` | 256×256 | 2400 | <img src="wgpu/linux-lavapipe/stroke__under_nonuniform_scale_s200.png" width="240"> |
| [ ] | `stroke/under_rotation` | 128×128 | 3679 | <img src="wgpu/linux-lavapipe/stroke__under_rotation.png" width="128"> |
| [ ] | `stroke/under_rotation_s125` | 160×160 | 4848 | <img src="wgpu/linux-lavapipe/stroke__under_rotation_s125.png" width="160"> |
| [ ] | `stroke/under_rotation_s150` | 192×192 | 5903 | <img src="wgpu/linux-lavapipe/stroke__under_rotation_s150.png" width="192"> |
| [ ] | `stroke/under_rotation_s175` | 224×224 | 7092 | <img src="wgpu/linux-lavapipe/stroke__under_rotation_s175.png" width="224"> |
| [ ] | `stroke/under_rotation_s200` | 256×256 | 8398 | <img src="wgpu/linux-lavapipe/stroke__under_rotation_s200.png" width="240"> |
| [ ] | `stroke/widths` | 128×128 | 1617 | <img src="wgpu/linux-lavapipe/stroke__widths.png" width="128"> |
| [ ] | `stroke/widths_s125` | 160×160 | 2116 | <img src="wgpu/linux-lavapipe/stroke__widths_s125.png" width="160"> |
| [ ] | `stroke/widths_s150` | 192×192 | 2254 | <img src="wgpu/linux-lavapipe/stroke__widths_s150.png" width="192"> |
| [ ] | `stroke/widths_s175` | 224×224 | 3186 | <img src="wgpu/linux-lavapipe/stroke__widths_s175.png" width="224"> |
| [ ] | `stroke/widths_s200` | 256×256 | 3319 | <img src="wgpu/linux-lavapipe/stroke__widths_s200.png" width="240"> |

## text

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `text/bidi_mixed` | 480×320 | 5269 | <img src="wgpu/linux-lavapipe/text__bidi_mixed.png" width="240"> |
| [ ] | `text/cjk` | 480×320 | 7121 | <img src="wgpu/linux-lavapipe/text__cjk.png" width="240"> |
| [ ] | `text/colored_runs` | 480×320 | 5566 | <img src="wgpu/linux-lavapipe/text__colored_runs.png" width="240"> |
| [ ] | `text/colr_foreground` | 160×160 | 400 | <img src="wgpu/linux-lavapipe/text__colr_foreground.png" width="160"> |
| [ ] | `text/colr_v0_layers` | 160×160 | 1447 | <img src="wgpu/linux-lavapipe/text__colr_v0_layers.png" width="160"> |
| [ ] | `text/colr_v0_rotated` | 160×160 | 2811 | <img src="wgpu/linux-lavapipe/text__colr_v0_rotated.png" width="160"> |
| [ ] | `text/colr_v1_composite` | 160×160 | 393 | <img src="wgpu/linux-lavapipe/text__colr_v1_composite.png" width="160"> |
| [ ] | `text/colr_v1_gradient` | 160×160 | 668 | <img src="wgpu/linux-lavapipe/text__colr_v1_gradient.png" width="160"> |
| [ ] | `text/decorations` | 480×320 | 5706 | <img src="wgpu/linux-lavapipe/text__decorations.png" width="240"> |
| [ ] | `text/decorations_all_styles` | 480×320 | 17444 | <img src="wgpu/linux-lavapipe/text__decorations_all_styles.png" width="240"> |
| [ ] | `text/dense_paragraph` | 480×320 | 39124 | <img src="wgpu/linux-lavapipe/text__dense_paragraph.png" width="240"> |
| [ ] | `text/sharpness_13px` | 480×320 | 29344 | <img src="wgpu/linux-lavapipe/text__sharpness_13px.png" width="240"> |
| [ ] | `text/ellipsis_maxlines` | 480×320 | 8841 | <img src="wgpu/linux-lavapipe/text__ellipsis_maxlines.png" width="240"> |
| [ ] | `text/emoji_color` | 480×320 | 14287 | <img src="wgpu/linux-lavapipe/text__emoji_color.png" width="240"> |
| [ ] | `text/glyph_run_synthetic` | 480×320 | 21899 | <img src="wgpu/linux-lavapipe/text__glyph_run_synthetic.png" width="240"> |
| [ ] | `text/italic_synth` | 480×320 | 6073 | <img src="wgpu/linux-lavapipe/text__italic_synth.png" width="240"> |
| [ ] | `text/large_96` | 640×160 | 10814 | <img src="wgpu/linux-lavapipe/text__large_96.png" width="240"> |
| [ ] | `text/latin_sizes` | 480×320 | 25818 | <img src="wgpu/linux-lavapipe/text__latin_sizes.png" width="240"> |
| [ ] | `text/latin_sizes_s125` | 600×400 | 35363 | <img src="wgpu/linux-lavapipe/text__latin_sizes_s125.png" width="240"> |
| [ ] | `text/latin_sizes_s150` | 720×480 | 43450 | <img src="wgpu/linux-lavapipe/text__latin_sizes_s150.png" width="240"> |
| [ ] | `text/latin_sizes_s175` | 840×560 | 53149 | <img src="wgpu/linux-lavapipe/text__latin_sizes_s175.png" width="240"> |
| [ ] | `text/latin_sizes_s200` | 960×640 | 65826 | <img src="wgpu/linux-lavapipe/text__latin_sizes_s200.png" width="240"> |
| [ ] | `text/mixed_scripts_fallback` | 480×320 | 5057 | <img src="wgpu/linux-lavapipe/text__mixed_scripts_fallback.png" width="240"> |
| [ ] | `text/mono_tabs` | 480×320 | 4101 | <img src="wgpu/linux-lavapipe/text__mono_tabs.png" width="240"> |
| [ ] | `text/partially_offscreen_top` | 480×320 | 4486 | <img src="wgpu/linux-lavapipe/text__partially_offscreen_top.png" width="240"> |
| [ ] | `text/rotated_30` | 480×320 | 5717 | <img src="wgpu/linux-lavapipe/text__rotated_30.png" width="240"> |
| [ ] | `text/rtl_arabic` | 480×320 | 3848 | <img src="wgpu/linux-lavapipe/text__rtl_arabic.png" width="240"> |
| [ ] | `text/rtl_hebrew` | 480×320 | 3257 | <img src="wgpu/linux-lavapipe/text__rtl_hebrew.png" width="240"> |
| [ ] | `text/scaled_transform_3x` | 480×320 | 1326 | <img src="wgpu/linux-lavapipe/text__scaled_transform_3x.png" width="240"> |
| [ ] | `text/selection_caret` | 480×320 | 2614 | <img src="wgpu/linux-lavapipe/text__selection_caret.png" width="240"> |
| [ ] | `text/serif` | 480×320 | 18536 | <img src="wgpu/linux-lavapipe/text__serif.png" width="240"> |
| [ ] | `text/shadow_blur` | 320×96 | 9519 | <img src="wgpu/linux-lavapipe/text__shadow_blur.png" width="240"> |
| [ ] | `text/shadows_and_background` | 480×320 | 20947 | <img src="wgpu/linux-lavapipe/text__shadows_and_background.png" width="240"> |
| [ ] | `text/subpixel_positions` | 480×320 | 1922 | <img src="wgpu/linux-lavapipe/text__subpixel_positions.png" width="240"> |
| [ ] | `text/subpixel_positions_s125` | 600×400 | 2591 | <img src="wgpu/linux-lavapipe/text__subpixel_positions_s125.png" width="240"> |
| [ ] | `text/subpixel_positions_s150` | 720×480 | 3132 | <img src="wgpu/linux-lavapipe/text__subpixel_positions_s150.png" width="240"> |
| [ ] | `text/subpixel_positions_s175` | 840×560 | 3959 | <img src="wgpu/linux-lavapipe/text__subpixel_positions_s175.png" width="240"> |
| [ ] | `text/subpixel_positions_s200` | 960×640 | 5219 | <img src="wgpu/linux-lavapipe/text__subpixel_positions_s200.png" width="240"> |
| [ ] | `text/tiny_6px` | 480×320 | 2927 | <img src="wgpu/linux-lavapipe/text__tiny_6px.png" width="240"> |
| [ ] | `text/two_fonts_same_glyph_ids` | 480×320 | 12587 | <img src="wgpu/linux-lavapipe/text__two_fonts_same_glyph_ids.png" width="240"> |
| [ ] | `text/weights` | 480×320 | 26519 | <img src="wgpu/linux-lavapipe/text__weights.png" width="240"> |

## widget

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `widget/button_variants_default` | 480×320 | 8450 | <img src="wgpu/linux-lavapipe/widget__button_variants_default.png" width="240"> |
| [ ] | `widget/button_variants_material` | 480×320 | 14110 | <img src="wgpu/linux-lavapipe/widget__button_variants_material.png" width="240"> |
| [ ] | `widget/button_variants_tailwind` | 480×320 | 10020 | <img src="wgpu/linux-lavapipe/widget__button_variants_tailwind.png" width="240"> |
| [ ] | `widget/calendar` | 480×320 | 12877 | <img src="wgpu/linux-lavapipe/widget__calendar.png" width="240"> |
| [ ] | `widget/card_grid` | 480×320 | 8969 | <img src="wgpu/linux-lavapipe/widget__card_grid.png" width="240"> |
| [ ] | `widget/chips` | 480×320 | 2942 | <img src="wgpu/linux-lavapipe/widget__chips.png" width="240"> |
| [ ] | `widget/data_table` | 480×320 | 12162 | <img src="wgpu/linux-lavapipe/widget__data_table.png" width="240"> |
| [ ] | `widget/dialog_over_scrim` | 480×320 | 5259 | <img src="wgpu/linux-lavapipe/widget__dialog_over_scrim.png" width="240"> |
| [ ] | `widget/list_tiles_divider` | 480×320 | 12869 | <img src="wgpu/linux-lavapipe/widget__list_tiles_divider.png" width="240"> |
| [ ] | `widget/popover_open` | 480×320 | 2625 | <img src="wgpu/linux-lavapipe/widget__popover_open.png" width="240"> |
| [ ] | `widget/sheet_open` | 480×320 | 2353 | <img src="wgpu/linux-lavapipe/widget__sheet_open.png" width="240"> |
| [ ] | `widget/spinner_settled` | 480×320 | 2331 | <img src="wgpu/linux-lavapipe/widget__spinner_settled.png" width="240"> |
| [ ] | `widget/tabs` | 480×320 | 2626 | <img src="wgpu/linux-lavapipe/widget__tabs.png" width="240"> |
| [ ] | `widget/text_field_states` | 480×320 | 5051 | <img src="wgpu/linux-lavapipe/widget__text_field_states.png" width="240"> |
| [ ] | `widget/toast_shown` | 480×320 | 2353 | <img src="wgpu/linux-lavapipe/widget__toast_shown.png" width="240"> |
| [ ] | `widget/toggles` | 480×320 | 4766 | <img src="wgpu/linux-lavapipe/widget__toggles.png" width="240"> |
| [ ] | `widget/tooltip_visible` | 480×320 | 2257 | <img src="wgpu/linux-lavapipe/widget__tooltip_visible.png" width="240"> |

## xform

| ok | case | size | bytes | image |
|---|---|---|---|---|
| [ ] | `xform/mirror_x` | 128×128 | 1664 | <img src="wgpu/linux-lavapipe/xform__mirror_x.png" width="128"> |
| [ ] | `xform/nested_append` | 128×128 | 2705 | <img src="wgpu/linux-lavapipe/xform__nested_append.png" width="128"> |
| [ ] | `xform/rotate_15` | 128×128 | 3813 | <img src="wgpu/linux-lavapipe/xform__rotate_15.png" width="128"> |
| [ ] | `xform/rotate_45` | 128×128 | 3477 | <img src="wgpu/linux-lavapipe/xform__rotate_45.png" width="128"> |
| [ ] | `xform/rotate_90` | 128×128 | 3108 | <img src="wgpu/linux-lavapipe/xform__rotate_90.png" width="128"> |
| [ ] | `xform/scale_10x_circle` | 128×128 | 299 | <img src="wgpu/linux-lavapipe/xform__scale_10x_circle.png" width="128"> |
| [ ] | `xform/skew_x_0_3` | 128×128 | 1638 | <img src="wgpu/linux-lavapipe/xform__skew_x_0_3.png" width="128"> |
