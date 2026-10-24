// M32 matched scenes, Flutter side. Same scenes and reporting as ../src/main.rs (README.md).
// Frame ticks come from a Ticker (one callback per engine frame). Flutter's own FrameTiming
// (build + raster) is reported as an extra line.
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui' show FrameTiming;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

double envF(String k, double d) => double.tryParse(Platform.environment[k] ?? '') ?? d;
final String scene = Platform.environment['M32_SCENE'] ?? 'dashboard';
final Stopwatch clock = Stopwatch()..start();

double pct(List<double> s, int p) => s.isEmpty ? 0 : s[((s.length - 1) * p + 50) ~/ 100];

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final timings = <double>[];
  SchedulerBinding.instance.addTimingsCallback((List<FrameTiming> ts) {
    if (clock.elapsedMilliseconds >= envF('M32_WARM', 2) * 1000) {
      for (final t in ts) {
        timings.add(t.totalSpan.inMicroseconds / 1000.0);
      }
    }
  });
  runApp(Bench(timings: timings));
}

class Bench extends StatefulWidget {
  const Bench({super.key, required this.timings});
  final List<double> timings;
  @override
  State<Bench> createState() => _BenchState();
}

class _BenchState extends State<Bench> with SingleTickerProviderStateMixin {
  late final Ticker ticker;
  final list = ScrollController();
  final samples = <double>[];
  Duration? last;
  bool first = false;
  double phase = 0;

  @override
  void initState() {
    super.initState();
    ticker = createTicker(onTick)..start();
  }

  void onTick(Duration _) {
    final now = clock.elapsedMicroseconds / 1e6;
    if (!first) {
      first = true;
      stdout.writeln('M32-first-frame epoch_ms=${DateTime.now().millisecondsSinceEpoch}');
    }
    final d = clock.elapsed;
    if (now >= envF('M32_WARM', 2) && last != null) {
      samples.add((d - last!).inMicroseconds / 1000.0);
    }
    last = d;
    if (now >= envF('M32_SECS', 12)) {
      ticker.stop();
      samples.sort();
      final t = List<double>.from(widget.timings)..sort();
      stdout.writeln('M32 toolkit=flutter scene=$scene frames=${samples.length} '
          'p50_ms=${pct(samples, 50).toStringAsFixed(2)} p95_ms=${pct(samples, 95).toStringAsFixed(2)}');
      stdout.writeln('M32-own toolkit=flutter scene=$scene frametiming_total p50_ms=${pct(t, 50).toStringAsFixed(2)} '
          'p95_ms=${pct(t, 95).toStringAsFixed(2)} n=${t.length}');
      return;
    }
    if (scene == 'list_10k_scroll' && list.hasClients && list.position.hasContentDimensions) {
      list.jumpTo(math.min(now * 1200, list.position.maxScrollExtent));
    }
    setState(() => phase = now);
  }

  @override
  void dispose() {
    ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    Widget body;
    switch (scene) {
      case 'rects_10k':
        body = CustomPaint(size: const Size(1200, 800), painter: RectsPainter(phase));
      case 'list_10k_scroll':
        body = ListView.builder(
          controller: list,
          itemCount: 10000,
          itemExtent: 48,
          itemBuilder: (_, i) => Row(children: [
            Text('Row $i', style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600)),
            const SizedBox(width: 16),
            Text('The quick brown fox jumps over the lazy dog — item $i', style: const TextStyle(fontSize: 14)),
          ]),
        );
      default:
        body = dashboard();
    }
    return MaterialApp(debugShowCheckedModeBanner: false, home: Material(child: body));
  }

  Widget card(int k) => Container(
        width: 360,
        height: 330,
        margin: const EdgeInsets.all(12),
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: const Color.fromARGB(255, 32, 36, 52),
          borderRadius: BorderRadius.circular(14),
          boxShadow: const [BoxShadow(color: Color.fromARGB(120, 0, 0, 0), offset: Offset(0, 8), blurRadius: 24)],
        ),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('Metric $k',
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600, color: Color.fromARGB(255, 230, 230, 240))),
          Text((1000 + 50 * math.sin(phase * 6 + k)).toStringAsFixed(1),
              style: const TextStyle(fontSize: 32, color: Colors.white)),
          SizedBox(height: 160, width: double.infinity, child: CustomPaint(painter: ChartPainter(phase, k))),
        ]),
      );

  Widget dashboard() => Container(
        width: 1200,
        height: 800,
        color: const Color.fromARGB(255, 15, 17, 26),
        child: Column(children: [
          Row(children: [card(0), card(1), card(2)]),
          Row(children: [card(3), card(4), card(5)]),
        ]),
      );
}

class RectsPainter extends CustomPainter {
  RectsPainter(this.phase);
  final double phase;
  @override
  void paint(Canvas c, Size size) {
    final shift = (phase * 20) % 20;
    final a = Paint()..color = const Color.fromARGB(255, 66, 133, 244);
    final b = Paint()..color = const Color.fromARGB(255, 234, 67, 53);
    for (var i = 0; i < 100; i++) {
      for (var j = 0; j < 100; j++) {
        c.drawRect(Rect.fromLTWH(i * 10 + shift, j * 10 + shift * 0.5, 8, 8), (i + j) % 2 == 0 ? a : b);
      }
    }
  }

  @override
  bool shouldRepaint(RectsPainter old) => old.phase != phase;
}

class ChartPainter extends CustomPainter {
  ChartPainter(this.phase, this.k);
  final double phase;
  final int k;
  @override
  void paint(Canvas c, Size s) {
    final p = Path();
    for (var i = 0; i <= 60; i++) {
      final x = s.width * i / 60;
      final y = s.height * (0.5 + 0.4 * math.sin(i * 0.25 + phase * 6 + k));
      i == 0 ? p.moveTo(x, y) : p.lineTo(x, y);
    }
    c.drawPath(
        p,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2
          ..color = const Color.fromARGB(255, 120, 200, 255));
  }

  @override
  bool shouldRepaint(ChartPainter old) => old.phase != phase;
}
