#!/usr/bin/env python3
"""Assemble the two-window IPC demo assets from headless capture frames.

Reads the raw RGBA frames written by the gallery's capture harness
(`GALLERY_CAPTURE=<dir> cargo run -p gallery --release`) and produces:

  * windows-ipc.gif         — the animated demo (every step, looping)
  * windows-ipc-strip.png   — a 3-still screenshot strip

Each step composites the main "Windows & IPC" window beside the secondary
"Counter" window (each dressed with a faux OS titlebar) over a caption. Pure
Pillow — no system capture tools needed.

Usage:  python3 demo/build_demo.py <capture-dir> [out-dir]
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
FONT_BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"

BG = (244, 244, 245)       # zinc-100 canvas
TITLEBAR = (250, 250, 250)  # window chrome
BORDER = (214, 214, 219)    # zinc-300
CAPTION = (39, 39, 42)      # zinc-800
SUBTLE = (113, 113, 122)    # zinc-500
DOTS = [(255, 95, 86), (255, 189, 46), (39, 201, 63)]  # traffic lights

PAD = 28
GAP = 26
TITLEBAR_H = 30
CAPTION_H = 58


def load_rgba(cap: Path, name: str, w: int, h: int) -> Image.Image:
    return Image.frombytes("RGBA", (w, h), (cap / f"{name}.rgba").read_bytes())


def read_manifest(cap: Path):
    """-> [(step_index, caption, (main_name,mw,mh), (side_name,sw,sh))]"""
    dims, captions = {}, {}
    for line in (cap / "manifest.txt").read_text().splitlines():
        if line.startswith("# step "):
            i, cap_txt = line[len("# step "):].split(":", 1)
            captions[int(i)] = cap_txt.strip()
        elif line.strip():
            name, w, h = line.split()
            dims[name] = (int(w), int(h))
    steps = sorted(captions)
    out = []
    for i in steps:
        m = f"step{i}_main"
        s = f"step{i}_side"
        out.append((i, captions[i], (m, *dims[m]), (s, *dims[s])))
    return out


def window_frame(img: Image.Image, title: str, font) -> Image.Image:
    """Dress a rendered window with a faux OS titlebar + border."""
    w, h = img.size
    out = Image.new("RGB", (w, h + TITLEBAR_H), TITLEBAR)
    d = ImageDraw.Draw(out)
    for k, color in enumerate(DOTS):
        cx = 16 + k * 18
        d.ellipse([cx - 5, TITLEBAR_H // 2 - 5, cx + 5, TITLEBAR_H // 2 + 5], fill=color)
    tb = d.textbbox((0, 0), title, font=font)
    d.text(((w - (tb[2] - tb[0])) / 2, (TITLEBAR_H - (tb[3] - tb[1])) / 2 - tb[1]),
           title, fill=SUBTLE, font=font)
    out.paste(img.convert("RGB"), (0, TITLEBAR_H))
    d.rectangle([0, 0, w - 1, h + TITLEBAR_H - 1], outline=BORDER, width=1)
    return out


def compose(cap: Path, step, fonts) -> Image.Image:
    _, caption, (mn, mw, mh), (sn, sw, sh) = step
    main = window_frame(load_rgba(cap, mn, mw, mh), "Windows & IPC", fonts["small"])
    side = window_frame(load_rgba(cap, sn, sw, sh), "Counter", fonts["small"])

    inner_h = max(main.height, side.height)
    canvas_w = PAD + main.width + GAP + side.width + PAD
    canvas_h = PAD + inner_h + CAPTION_H + PAD
    canvas = Image.new("RGB", (canvas_w, canvas_h), BG)
    canvas.paste(main, (PAD, PAD + (inner_h - main.height) // 2))
    canvas.paste(side, (PAD + main.width + GAP, PAD + (inner_h - side.height) // 2))

    d = ImageDraw.Draw(canvas)
    cy = PAD + inner_h + 14
    d.text((PAD + 2, cy), "Two OS windows · one reactive runtime", fill=SUBTLE, font=fonts["small"])
    d.text((PAD + 2, cy + 22), caption, fill=CAPTION, font=fonts["cap"])
    return canvas


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    cap = Path(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).resolve().parent
    out.mkdir(parents=True, exist_ok=True)
    fonts = {
        "small": ImageFont.truetype(FONT, 13),
        "cap": ImageFont.truetype(FONT_BOLD, 17),
    }
    steps = read_manifest(cap)
    frames = [compose(cap, s, fonts) for s in steps]

    # Animated GIF — scale to a sane width, hold the final frame longer.
    target_w = 940
    scaled = []
    for fr in frames:
        r = target_w / fr.width
        scaled.append(fr.resize((target_w, round(fr.height * r)), Image.LANCZOS))
    durations = [900] * len(scaled)
    durations[0] = 1300   # let the "spawn" beat read
    durations[-1] = 1800  # hold the final synced state
    gif_path = out / "windows-ipc.gif"
    scaled[0].save(gif_path, save_all=True, append_images=scaled[1:], loop=0,
                   duration=durations, optimize=True, disposal=2)
    print(f"wrote {gif_path}  ({len(scaled)} frames, {gif_path.stat().st_size // 1024} KB)")

    # Screenshot strip — three representative beats stacked (spawn, message, synced).
    picks = [0, 3, len(frames) - 1]
    strip_w = max(frames[i].width for i in picks)
    strip = Image.new("RGB", (strip_w, sum(frames[i].height for i in picks) + 2 * 16), BG)
    y = 0
    for i in picks:
        strip.paste(frames[i], ((strip_w - frames[i].width) // 2, y))
        y += frames[i].height + 16
    strip_path = out / "windows-ipc-strip.png"
    strip.save(strip_path, optimize=True)
    print(f"wrote {strip_path}  ({strip.size[0]}x{strip.size[1]}, {strip_path.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
