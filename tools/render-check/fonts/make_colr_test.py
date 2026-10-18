#!/usr/bin/env python3
"""Build colr-test.ttf (02 R29 step 6): a tiny deterministic COLR font for tests.

  A  COLRv0: two layers — red square (palette 0), then blue circle (palette 1)
  B  COLRv1: PaintLinearGradient red -> blue over a square
  C  COLRv1: PaintComposite Multiply of two overlapping squares (red, green)
  D  COLRv0: one layer using palette index 0xFFFF (the run's foreground color)

Requires fontTools (`pip install fonttools`). Run from this directory:
  python3 make_colr_test.py   # writes colr-test.ttf
"""
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.ttLib.tables.otTables import PaintFormat, CompositeMode
import math

UPEM = 1000


def rect(x0, y0, x1, y1):
    pen = TTGlyphPen(None)
    pen.moveTo((x0, y0))
    pen.lineTo((x0, y1))
    pen.lineTo((x1, y1))
    pen.lineTo((x1, y0))
    pen.closePath()
    return pen.glyph()


def circle(cx, cy, r, n=32):
    pen = TTGlyphPen(None)
    pts = [(round(cx + r * math.cos(2 * math.pi * i / n)), round(cy + r * math.sin(2 * math.pi * i / n))) for i in range(n)]
    pen.moveTo(pts[0])
    for p in reversed(pts[1:]):
        pen.lineTo(p)
    pen.closePath()
    return pen.glyph()


order = [".notdef", "A", "B", "C", "D", "sq", "circ", "sqL", "sqR"]
glyphs = {
    ".notdef": rect(100, 0, 500, 700),
    "A": rect(0, 0, 800, 800),
    "B": rect(0, 0, 800, 800),
    "C": rect(0, 0, 800, 800),
    "D": rect(0, 0, 800, 800),
    "sq": rect(100, 100, 700, 700),
    "circ": circle(500, 500, 280),
    "sqL": rect(100, 100, 550, 700),
    "sqR": rect(350, 100, 800, 700),
}

fb = FontBuilder(UPEM, isTTF=True)
fb.setupGlyphOrder(order)
fb.setupCharacterMap({ord("A"): "A", ord("B"): "B", ord("C"): "C", ord("D"): "D"})
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics({g: (1000, 0) for g in order})
fb.setupHorizontalHeader(ascent=900, descent=-100)
fb.setupNameTable({"familyName": "Pebbles COLR Test", "styleName": "Regular"})
fb.setupOS2(sTypoAscender=900, sTypoDescender=-100, usWinAscent=900, usWinDescent=100)
fb.setupPost()

# Palette 0: red, blue, green (RGBA floats).
fb.font["CPAL"] = buildCPAL([[(1.0, 0.0, 0.0, 1.0), (0.0, 0.0, 1.0, 1.0), (0.0, 0.8, 0.0, 1.0)]])


def solid(idx, alpha=1.0):
    return {"Format": PaintFormat.PaintSolid, "PaletteIndex": idx, "Alpha": alpha}


def glyph(name, paint):
    return {"Format": PaintFormat.PaintGlyph, "Glyph": name, "Paint": paint}


colr = {
    # COLRv0 layer lists.
    "A": [("sq", 0), ("circ", 1)],
    "D": [("sq", 0xFFFF)],
    # COLRv1 paint graphs.
    "B": glyph(
        "sq",
        {
            "Format": PaintFormat.PaintLinearGradient,
            "ColorLine": {"Extend": "pad", "ColorStop": [{"StopOffset": 0.0, "PaletteIndex": 0, "Alpha": 1.0}, {"StopOffset": 1.0, "PaletteIndex": 1, "Alpha": 1.0}]},
            "x0": 100, "y0": 400, "x1": 700, "y1": 400, "x2": 100, "y2": 1000,
        },
    ),
    "C": {
        "Format": PaintFormat.PaintComposite,
        "SourcePaint": glyph("sqR", solid(2)),
        "CompositeMode": CompositeMode.MULTIPLY,
        "BackdropPaint": glyph("sqL", solid(0)),
    },
}
# version=None: A and D are encoded as COLRv0 records, B and C as COLRv1 paints.
fb.font["COLR"] = buildCOLR(colr, version=None, glyphMap=fb.font.getReverseGlyphMap())
# Fixed timestamps: the committed bytes are reproducible.
fb.font["head"].created = fb.font["head"].modified = 0
fb.save("colr-test.ttf")
