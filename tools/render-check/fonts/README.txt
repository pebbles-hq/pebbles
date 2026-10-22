Test fonts for render-check and the shell GPU tests (wgpu migration V22). TEST-ONLY:
never part of the framework's bundled set. All SIL OFL 1.1 (see OFL.txt).

Source: the Debian/Ubuntu font packages installed on the reference Linux machine
(the same upstream Noto releases), subset with fonttools 4.67.0 (pyftsubset).

File                     Upstream file (package version)                      sha256 of the original
NotoSansArabic-test.ttf  NotoSansArabic-Regular.ttf (fonts-noto-core 20201225-2)      504d7407d86875acf7d04dfaa0fd7524d0b8797723bc4aa18022f29db25b0b6e
NotoSansHebrew-test.ttf  NotoSansHebrew-Regular.ttf (fonts-noto-core 20201225-2)      436900d5ad77d33e4234247f3076eaecd25b92b9bea0519514f684140e3566a7
NotoSansSC-test.otf      NotoSansCJK-Regular.ttc face 2 "Noto Sans CJK SC"
                         (fonts-noto-cjk 1:20230817+repack1-3)                        b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a
NotoColorEmoji-test.ttf  NotoColorEmoji.ttf, CBDT (fonts-noto-color-emoji 2.047)      93cdc4ee9aa40e2afceecc63da0ca05ec7aab4bec991ece51a6b52389f48a477

Commands (subset text files contain exactly the strings of the text/ cases):
  arabic.txt = "مرحبا بالعالم Hello مرحبا 你好 "
  hebrew.txt = "שלום עולם abc אבג 123 def "
  cjk.txt    = "你好，世界。渲染测试 Hello مرحبا 你好 "
  pyftsubset NotoSansArabic-Regular.ttf --text-file=arabic.txt --layout-features='*' --no-hinting --output-file=NotoSansArabic-test.ttf
  pyftsubset NotoSansHebrew-Regular.ttf --text-file=hebrew.txt --layout-features='*' --no-hinting --output-file=NotoSansHebrew-test.ttf
  pyftsubset NotoSansCJK-Regular.ttc --font-number=2 --text-file=cjk.txt --layout-features='*' --no-hinting --output-file=NotoSansSC-test.otf
  pyftsubset NotoColorEmoji.ttf --unicodes=U+1F600,U+1F44D,U+1F680,U+1F389,U+2764,U+FE0F --output-file=NotoColorEmoji-test.ttf

colr-test.ttf / make_colr_test.py: the synthetic COLR test font (see the script).
