use super::*;

fn result(name: &str, status: Status) -> CaseResult {
    CaseResult {
        name: name.to_string(),
        slice: Slice::P1,
        category: Category::Geometry,
        status,
        expect: "pass".to_string(),
        xfail_until: None,
        device_size: (4, 4),
        scale: 1.0,
        metrics: None,
        limits: None,
        probes: vec![("p", Some("bad \"q\"".to_string()))],
        fallbacks: vec![("unknown-op", 2)],
        note: String::new(),
        artifacts: None,
        millis_reference: 1,
        millis_candidate: 2,
    }
}

#[test]
fn json_escapes_quotes_and_backslashes() {
    assert_eq!(json_escape(r#"a"b\c"#), r#"a\"b\\c"#);
    assert_eq!(json_escape("x\ny\u{1}"), "x\\ny\\u0001");
}

#[test]
fn json_report_lists_totals_and_cases() {
    let mut a = result("prim/a", Status::Pass);
    a.xfail_until = Some("C10");
    let rs = [a, result("prim/b", Status::Panic)];
    let j = report_json(&RunInfo::default(), &rs);
    assert!(j.contains("\"PASS\": 1") && j.contains("\"PANIC\": 1"));
    assert!(j.contains("\"name\": \"prim/b\"") && j.contains("\"status\": \"PANIC\""));
    assert!(j.contains("bad \\\"q\\\"") && j.contains("\"unknown-op\": 2"));
    let md = report_md(&RunInfo::default(), &rs);
    assert!(md.contains("| C10 | prim/a |") && md.contains("| prim/b | P1 | Geometry | PANIC |"));
    assert!(md.contains("| prim/a | unknown-op=2 |"));
}

#[test]
fn diff_and_side_by_side_shapes() {
    let r = Rgba8::filled(4, 4, [255; 4]);
    let mut c = r.clone();
    c.set(1, 1, [0, 0, 0, 255]);
    let l = Limits { over_threshold: 2, max_pct_over: 0.0, max_interior: 1, max_mae: 0.1 };
    let d = diff_image(&r, &c, &l);
    assert_eq!(d.px(3, 3), [255, 255, 255, 255], "white within T maps to gray 0.3*255+179");
    assert_ne!(d.px(1, 1), d.px(3, 3));
    let s = side_by_side(&r, &c, &d);
    assert_eq!((s.width, s.height), (12, 4));
    assert_eq!(s.px(5, 1), [0, 0, 0, 255]);
}
