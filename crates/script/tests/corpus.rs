//! The script corpus: original scripts covering what people script a vector app for (generative
//! art, bulk edits, layout, text, export, reports, failures) and the JavaScript language, each run
//! against a fresh 400 × 300 document and checked. Third-party libraries (tests/thirdparty, MIT)
//! run before the scripts that use them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use serde_json::{Map, Value, json};
use vectorcraft_engine::Session;
use vectorcraft_script::{ScriptError, ScriptOptions, ScriptOutcome, run_script};

type Outcome = Result<ScriptOutcome, ScriptError>;

struct Case {
    file: &'static str,
    /// (file in tests/thirdparty, the global its `module.exports` becomes), run first.
    libs: &'static [(&'static str, &'static str)],
    setup: fn(&mut Session),
    args: fn(&Path) -> Map<String, Value>,
    check: fn(&Session, Outcome, &Path),
}

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    s
}

fn source(case: &Case) -> String {
    let mut src = String::new();
    for (lib, global) in case.libs {
        let code = std::fs::read_to_string(manifest().join("tests/thirdparty").join(lib)).unwrap();
        src.push_str(&format!("var module = {{ exports: {{}} }}; var exports = module.exports;\n{code}\nvar {global} = module.exports;\n"));
    }
    src.push_str(&std::fs::read_to_string(manifest().join("tests/corpus").join(case.file)).unwrap());
    src
}

fn run_case(case: &Case) {
    let dir = std::env::temp_dir().join(format!("vectorcraft-script-corpus-{}-{}", std::process::id(), case.file));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session();
    (case.setup)(&mut s);
    let opts = ScriptOptions { args: (case.args)(&dir), ..Default::default() };
    let out = run_script(&mut s, &source(case), case.file, &opts);
    (case.check)(&s, out, &dir);
}

// ---------- helpers ----------

fn no_setup(_: &mut Session) {}
fn no_args(_: &Path) -> Map<String, Value> {
    Map::new()
}
fn ok(out: Outcome) -> ScriptOutcome {
    out.unwrap_or_else(|e| panic!("script failed: {e}\nlog: {:?}", e.log))
}
fn err(out: Outcome) -> ScriptError {
    match out {
        Ok(o) => panic!("script succeeded with {}", o.value),
        Err(e) => e,
    }
}
fn inspect(s: &Session) -> Value {
    vectorcraft_engine::inspect::document(s)
}
/// The first layer's objects, top of the stack first (as document.inspect lists them).
fn children(s: &Session) -> Vec<Value> {
    inspect(s)["layers"][0]["children"].as_array().cloned().unwrap_or_default()
}
fn labels(s: &Session) -> Vec<String> {
    s.doc().unwrap().history.undo.iter().map(|h| h.label.clone()).collect()
}
/// The run is the last undo step.
fn one_step(s: &Session, file: &str) {
    assert_eq!(labels(s).last().cloned(), Some(format!("Script: {file}")), "{:?}", labels(s));
}
/// Untouched: nothing on the first layer and no undo step past the setup's `setup_steps`.
fn untouched(s: &Session, setup_steps: usize, objects: usize) {
    assert_eq!(children(s).len(), objects);
    assert_eq!(labels(s).len(), setup_steps);
    assert!(!s.in_interaction());
}
/// The first layer's fills as #rrggbb, bottom first (creation order).
fn fills(s: &Session) -> Vec<String> {
    s.doc().unwrap().doc.layers[0]
        .children()
        .unwrap()
        .iter()
        .map(|n| {
            let [r, g, b] = n.appearance.fill_paint().color().map(|c| c.to_rgb_uncalibrated()).unwrap_or([0.0; 3]);
            let byte = |v: f32| (v * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
        })
        .collect()
}
fn bounds(v: &Value) -> (f64, f64, f64, f64) {
    let b = &v["bounds"];
    (b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap(), b["width"].as_f64().unwrap(), b["height"].as_f64().unwrap())
}
fn add(s: &mut Session, id: &str, p: Value) -> u64 {
    s.execute(id, &p).unwrap()["id"].as_u64().unwrap_or_default()
}
fn rect_at(s: &mut Session, x: f64) -> u64 {
    add(s, "shape.rectangle", json!({"x": x, "y": 20, "width": 30, "height": 30}))
}
fn arg(key: &str, value: impl Into<Value>) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(key.into(), value.into());
    m
}

// ---------- cases ----------

const CASES: &[Case] = &[
    // Generative art
    Case {
        file: "gen_grid.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(20));
            assert_eq!(children(s).len(), 20);
            assert_eq!(labels(s), ["Script: gen_grid.js"]);
        },
    },
    Case {
        file: "gen_spiral.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(36));
            let c = children(s);
            assert_eq!(c.len(), 36);
            // Top of the stack (last drawn) is the biggest.
            assert!(bounds(&c[0]).2 > bounds(&c[35]).2);
        },
    },
    Case {
        file: "gen_scatter.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            let v = ok(o).value;
            assert_eq!(v.as_array().unwrap().len(), 50);
            // The same seed draws the same picture.
            let mut again = session();
            let src = std::fs::read_to_string(manifest().join("tests/corpus/gen_scatter.js")).unwrap();
            assert_eq!(run_script(&mut again, &src, "gen_scatter.js", &ScriptOptions::default()).unwrap().value, v);
            for c in children(s) {
                let (x, y, w, h) = bounds(&c);
                assert!(x >= -1e-6 && y >= -1e-6 && x + w <= 400.0 + 1e-6 && y + h <= 300.0 + 1e-6, "{c}");
            }
        },
    },
    Case {
        file: "gen_stars.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(126));
            assert_eq!(children(s).len(), 12);
        },
    },
    Case {
        file: "gen_polygons.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            let v = ok(o).value;
            let want: Vec<String> = v.as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
            assert_eq!(want.len(), 8);
            assert_eq!(want[0], "#e61919");
            assert_eq!(fills(s), want);
        },
    },
    Case {
        file: "gen_rays.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(30));
            assert_eq!(children(s).len(), 30);
        },
    },
    // Bulk edits
    Case {
        file: "edit_recolor_all.js",
        libs: &[],
        setup: |s| {
            for i in 0..3 {
                rect_at(s, 20.0 + 40.0 * i as f64);
            }
            add(s, "shape.ellipse", json!({"x": 200, "y": 100, "width": 40, "height": 40}));
            add(s, "shape.ellipse", json!({"x": 260, "y": 100, "width": 40, "height": 40}));
            s.execute("select.all", &json!({})).unwrap();
            s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
        },
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(5));
            assert_eq!(fills(s), vec!["#00aa55"; 5]);
            one_step(s, "edit_recolor_all.js");
        },
    },
    Case {
        file: "edit_rename_layers.js",
        libs: &[],
        setup: |s| {
            s.execute("layer.new", &json!({"name": "Sky"})).unwrap();
            s.execute("layer.new", &json!({"name": "Ground"})).unwrap();
        },
        args: no_args,
        check: |s, o, _| {
            let names: Vec<String> = inspect(s)["layers"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap().to_string()).collect();
            assert_eq!(ok(o).value, json!(names));
            assert!(names.iter().all(|n| n.starts_with("Part ")), "{names:?}");
            assert_eq!(names.len(), 3);
        },
    },
    Case {
        file: "edit_align_left.js",
        libs: &[],
        setup: |s| {
            rect_at(s, 30.0);
            rect_at(s, 120.0);
            rect_at(s, 250.0);
        },
        args: no_args,
        check: |s, o, _| {
            ok(o);
            let xs: Vec<f64> = children(s).iter().map(|c| bounds(c).0).collect();
            assert!(xs.iter().all(|x| (x - 30.0).abs() < 1e-6), "{xs:?}");
        },
    },
    Case {
        file: "edit_distribute.js",
        libs: &[],
        setup: |s| {
            for x in [10.0, 40.0, 200.0, 330.0] {
                rect_at(s, x);
            }
        },
        args: no_args,
        check: |s, o, _| {
            ok(o);
            let mut centres: Vec<f64> = children(s)
                .iter()
                .map(|c| {
                    let (x, _, w, _) = bounds(c);
                    x + w / 2.0
                })
                .collect();
            centres.sort_by(f64::total_cmp);
            let gaps: Vec<f64> = centres.windows(2).map(|w| w[1] - w[0]).collect();
            assert!(gaps.iter().all(|g| (g - gaps[0]).abs() < 1e-6), "{gaps:?}");
        },
    },
    Case {
        file: "edit_rotate_each.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(8));
            assert_eq!(children(s).len(), 8);
            assert_eq!(labels(s), ["Script: edit_rotate_each.js"]);
        },
    },
    Case {
        file: "edit_name_objects.js",
        libs: &[],
        setup: |s| {
            for i in 0..4 {
                rect_at(s, 20.0 + 50.0 * i as f64);
            }
        },
        args: no_args,
        check: |s, o, _| {
            ok(o);
            let mut names: Vec<String> = children(s).iter().map(|c| c["name"].as_str().unwrap().to_string()).collect();
            names.sort();
            assert_eq!(names, ["Tile 1", "Tile 2", "Tile 3", "Tile 4"]);
        },
    },
    Case {
        file: "edit_group_rows.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(3));
            let c = children(s);
            assert_eq!(c.len(), 3);
            assert!(c.iter().all(|g| g["children"].as_array().is_some_and(|k| k.len() == 4)), "{c:?}");
        },
    },
    // Layout
    Case {
        file: "layout_artboards.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(5));
            let names: Vec<String> = s.doc().unwrap().doc.artboards.iter().map(|a| a.name.clone()).collect();
            assert_eq!(names.len(), 5);
            assert_eq!(names[1..], ["Icon 16", "Icon 32", "Icon 64", "Icon 128"]);
        },
    },
    Case {
        file: "layout_contact_sheet.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(18));
            assert_eq!(children(s).len(), 18);
        },
    },
    // Text
    Case {
        file: "text_from_data.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(["Ada — 36", "Grace — 85", "Linus — 54"]));
            assert_eq!(children(s).iter().filter(|c| c["kind"].as_str().is_some_and(|k| k.contains("Text") || k.contains("Type"))).count(), 3);
        },
    },
    Case {
        file: "text_args.js",
        libs: &[],
        setup: no_setup,
        args: |_| arg("title", "Hello"),
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!("Hello"));
            assert_eq!(children(s).len(), 1);
        },
    },
    // Export
    Case {
        file: "export_svg.js",
        libs: &[],
        setup: no_setup,
        args: |d| arg("out", d.join("art.svg").to_string_lossy().to_string()),
        check: |_, o, d| {
            ok(o);
            let svg = std::fs::read_to_string(d.join("art.svg")).unwrap();
            assert!(svg.contains("<svg") && svg.contains("ff6600"), "{svg}");
        },
    },
    Case {
        file: "export_png_per_artboard.js",
        libs: &[],
        setup: no_setup,
        args: |d| arg("dir", d.to_string_lossy().to_string()),
        check: |_, o, d| {
            assert_eq!(ok(o).value, json!(2));
            for i in 0..2 {
                let png = std::fs::read(d.join(format!("board-{i}.png"))).unwrap();
                assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            }
        },
    },
    // Reports (read-only)
    Case {
        file: "report_kinds.js",
        libs: &[],
        setup: |s| {
            rect_at(s, 10.0);
            rect_at(s, 60.0);
            add(s, "shape.ellipse", json!({"x": 200, "y": 100, "width": 40, "height": 40}));
        },
        args: no_args,
        check: |s, o, _| {
            let v = ok(o).value;
            let total: u64 = v.as_object().unwrap().values().map(|n| n.as_u64().unwrap()).sum();
            assert_eq!(total, 3);
            assert_eq!(labels(s).len(), 3, "a report adds no undo step");
        },
    },
    Case {
        file: "report_commands.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            let want = vectorcraft_engine::command_specs().iter().filter(|c| c.id.starts_with("shape.")).count();
            assert_eq!(ok(o).value, json!(want));
        },
    },
    Case {
        file: "report_colours.js",
        libs: &[],
        setup: |s| {
            for (x, c) in [(10.0, "#ff0000"), (60.0, "#ff0000"), (110.0, "#0000ff")] {
                let id = rect_at(s, x);
                s.execute("paint.setFill", &json!({"ids": [id], "color": c})).unwrap();
            }
        },
        args: no_args,
        check: |_, o, _| {
            assert_eq!(ok(o).value.as_array().unwrap().len(), 2);
        },
    },
    // Failures
    Case {
        file: "fail_throw_midway.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            let e = err(o);
            assert!(e.message.contains("stop here"), "{e}");
            assert_eq!(e.line, Some(4), "{e}");
            assert_eq!(e.log, ["drew 3"]);
            untouched(s, 0, 0);
        },
    },
    Case {
        file: "fail_unknown_command.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert!(err(o).message.contains("unknown command `shape.hexagram`"));
            untouched(s, 0, 0);
        },
    },
    Case {
        file: "fail_bad_params.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert!(err(o).message.contains("shape.rectangle"));
            untouched(s, 0, 0);
        },
    },
    Case {
        file: "fail_runaway_loop.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert!(err(o).message.contains("loop"));
            untouched(s, 0, 0);
        },
    },
    Case {
        file: "fail_recursion.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            let e = err(o);
            assert!(e.message.contains("recursi") || e.message.contains("stack"), "{e}");
            untouched(s, 0, 0);
        },
    },
    Case {
        file: "fail_caught.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert!(ok(o).value.as_str().unwrap().contains("unknown command"));
            assert_eq!(children(s).len(), 1);
        },
    },
    // Third-party libraries (MIT, tests/thirdparty) driven by original scripts
    Case {
        file: "thirdparty_tinycolor.js",
        libs: &[("tinycolor.js", "tinycolor")],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            let v = ok(o).value;
            let want: Vec<String> = v.as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
            assert_eq!(want[0], "#3366ff");
            assert_eq!(want[3], "#ffcc33", "the complement");
            assert_eq!(fills(s), want);
        },
    },
    Case {
        file: "thirdparty_alea.js",
        libs: &[("alea.js", "alea")],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            let v = ok(o).value;
            assert_eq!(v.as_array().unwrap().len(), 40);
            assert_eq!(children(s).len(), 40);
            let mut again = session();
            let case = Case { file: "thirdparty_alea.js", libs: &[("alea.js", "alea")], setup: no_setup, args: no_args, check: |_, _, _| {} };
            assert_eq!(run_script(&mut again, &source(&case), case.file, &ScriptOptions::default()).unwrap().value, v);
        },
    },
    Case {
        file: "thirdparty_simplex.js",
        libs: &[("simplex-noise.js", "SimplexNoise")],
        setup: no_setup,
        args: no_args,
        check: |s, o, _| {
            assert_eq!(ok(o).value, json!(20));
            let heights: Vec<f64> = children(s).iter().map(|c| bounds(c).3).collect();
            assert!(heights.iter().all(|h| (20.0..=120.0 + 1e-6).contains(h)), "{heights:?}");
            assert!(heights.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-3), "noise varies");
        },
    },
    Case {
        file: "thirdparty_bezier.js",
        libs: &[("bezier-easing.js", "BezierEasing")],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            let xs: Vec<i64> = ok(o).value.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
            assert_eq!((xs[0], xs[5], xs[10]), (20, 200, 380));
            assert!(xs[1] - xs[0] < xs[5] - xs[4], "eases in");
        },
    },
    // The language
    Case {
        file: "lang_classes.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(ok(o).value, json!({"count": 3, "descriptions": ["rect: 12.0", "circle: 3.1", "rect: 5.0"], "isShape": true}));
        },
    },
    Case {
        file: "lang_closures_generators.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(
                ok(o).value,
                json!({"counter": 7, "firstTen": [0, 1, 1, 2, 3, 5, 8, 13, 21, 34], "x": 1, "y": 20, "rest": {"z": 30, "w": 40}, "sum": 88})
            );
        },
    },
    Case {
        file: "lang_collections.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(
                ok(o).value,
                json!({
                    "mapKeys": ["a", "b", "c"], "mapTotal": 6, "setSorted": [1, 2, 3],
                    "byLength": {"3": ["fig"], "4": ["pear", "kiwi", "date"], "5": ["apple"]},
                    "pairs": [["p", 1], ["q", 2]], "fromPairs": {"k": "v"}, "flat": [1, 2, 3], "flatMap": [1, 10, 2, 20],
                    "stable": "adbc", "includes": true
                })
            );
        },
    },
    Case {
        file: "lang_regex_strings.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(
                ok(o).value,
                json!({
                    "year": 2026, "month": "10", "replaced": "a+b+c", "template": "VectorCraft has 11 letters", "padded": "007",
                    "emojiLength": 2, "codePoints": 3, "raw": "\\n", "upper": "STRASSE", "sticky": false,
                    "split": ["one", "two", "three"], "trimmed": "x"
                })
            );
        },
    },
    Case {
        file: "lang_json_math_date.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(
                ok(o).value,
                json!({
                    "roundTrip": true, "pretty": "{\n \"k\": [\n  1\n ]\n}", "hypot": 5, "sin": 1, "epsilonSmall": true,
                    "maxSafe": 9007199254740991_i64, "utc": "2026-10-11T12:30:00.000Z", "dayOfWeek": 0, "parsed": 1791676800000_i64,
                    "fixed": "3.14"
                })
            );
        },
    },
    Case {
        file: "lang_typed_arrays.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(
                ok(o).value,
                json!({"sum": 7.5, "clamped": [255, 0, 128], "bytes": [18, 52, 52, 18], "firstTwo": [18, 52], "mapped": [3, 5, 7], "sub": [2, 3]})
            );
        },
    },
    Case {
        file: "lang_legacy_web.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(ok(o).value, json!({"substr": "ef", "escape": "a%20b", "unescape": "A", "bold": "<b>x</b>", "proto": true, "trimLeft": "x"}));
        },
    },
    Case {
        file: "lang_modern.js",
        libs: &[],
        setup: no_setup,
        args: no_args,
        check: |_, o, _| {
            assert_eq!(
                ok(o).value,
                json!({
                    "optional": "none", "call": "no call", "count": 6, "missing": "default", "big": "18446744073709551616",
                    "at": 30, "labels": ["low", "mid", "high"], "exp": 1024, "numericSep": 1000000, "objectSpread": {"a": 1, "b": 2},
                    "hasOwn": true, "findLast": 20
                })
            );
        },
    },
];

#[test]
fn every_corpus_script_passes() {
    // Every case runs, so one run names all the failures.
    let failed: Vec<String> = CASES
        .iter()
        .filter_map(|case| std::panic::catch_unwind(|| run_case(case)).err().map(|e| format!("{}: {}", case.file, panic_text(&*e))))
        .collect();
    assert!(failed.is_empty(), "{} of {} scripts failed:\n{}", failed.len(), CASES.len(), failed.join("\n"));
}

fn panic_text(e: &(dyn std::any::Any + Send)) -> String {
    e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

/// Each script in tests/corpus has a case (none goes unchecked), and each case its script.
#[test]
fn every_corpus_script_has_a_case() {
    let mut files: Vec<String> = std::fs::read_dir(manifest().join("tests/corpus"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".js"))
        .collect();
    files.sort();
    let mut cases: Vec<String> = CASES.iter().map(|c| c.file.to_string()).collect();
    cases.sort();
    assert_eq!(files, cases);
}
