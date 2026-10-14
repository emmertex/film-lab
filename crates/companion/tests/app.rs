//! The automatic mode against a stub of PhotoCraft's control channel: the authentication, the
//! copy saved and read back, the analysis, and the plug-in run or the dialog filled in.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use film_lab_companion::app::{AppOptions, convert_in_app};
use film_lab_companion::{AnalyzeOptions, Plugin};
use film_lab_core::{RANGE_STOPS, color, film};
use image::{ImageBuffer, Rgb};
use serde_json::{Map, Value, json};

const BASE: [f32; 3] = [0.6, 0.35, 0.15];
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Requests the stub has seen, as `(method, params)`.
type Log = Arc<Mutex<Vec<(String, Value)>>>;

/// A stub app: logs every request, writes a synthetic negative when asked to save, answers the rest.
fn stub(root: std::path::PathBuf) -> (String, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let log: Log = Arc::default();
    let seen = Arc::clone(&log);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(conn) = conn else { break };
            let mut writer = conn.try_clone().unwrap();
            let mut authed = false;
            for line in BufReader::new(conn).lines() {
                let Ok(line) = line else { break };
                let req: Value = serde_json::from_str(&line).unwrap();
                let method = req["method"].as_str().unwrap().to_string();
                let params = req.get("params").cloned().unwrap_or(Value::Null);
                seen.lock().unwrap().push((method.clone(), params.clone()));
                let result: Result<Value, String> = if method == "auth" {
                    authed = params["token"] == TOKEN;
                    if authed { Ok(Value::Null) } else { Err("bad token".into()) }
                } else if !authed {
                    Err("not authenticated".into())
                } else {
                    match method.as_str() {
                        "ui.inspect" => Ok(json!({"document": {"name": "scan.tif"}, "dialogs": []})),
                        "app.save" => {
                            let path = root.join(params["path"].as_str().unwrap());
                            synthetic(320, 240, 16).save(&path).unwrap();
                            Ok(json!({"path": params["path"], "warnings": ["Layers were flattened"]}))
                        }
                        "engine.execute" => match params["command"].as_str() {
                            Some("image.duplicate") | Some("file.close") => Ok(Value::Null),
                            Some("plugin.install") => {
                                let data = params["params"]["data"].as_str().unwrap();
                                assert!(data.len() > 100, "the module went over as base64");
                                let text = String::from_utf8_lossy(&decode64(data)).into_owned();
                                assert!(text.contains("\"white_r\":"), "the module carries the measurement: {text}");
                                Ok(json!({"id": "installed"}))
                            }
                            Some("plugin.run") => Ok(json!({"layer": 2, "plugin": params["params"]["id"]})),
                            other => Err(format!("unknown command {other:?}")),
                        },
                        "ui.menu.invoke" => Ok(json!({"dialog": 7})),
                        "ui.dialog.set" => Ok(Value::Null),
                        other => Err(format!("unknown method {other}")),
                    }
                };
                let reply = match result {
                    Ok(r) => json!({"id": req["id"], "ok": true, "result": r}),
                    Err(e) => json!({"id": req["id"], "ok": false, "error": e}),
                };
                writeln!(writer, "{reply}").unwrap();
            }
        }
    });
    (addr, log)
}

fn decode64(s: &str) -> Vec<u8> {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let vals: Vec<u8> = s.bytes().filter(|b| *b != b'=').map(|b| A.iter().position(|a| *a == b).unwrap() as u8).collect();
    let mut out = Vec::new();
    for chunk in vals.chunks(4) {
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() {
            n |= u32::from(*v) << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    out
}

fn synthetic(w: u32, h: u32, border: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let white = libm::exp2f(RANGE_STOPS) - 1.0;
    ImageBuffer::from_fn(w, h, |x, y| {
        let inside = x >= border && y >= border && x < w - border && y < h - border;
        let s = if inside { (x - border) as f32 / (w - 2 * border - 1) as f32 } else { 0.0 };
        let mut px = [0u8; 3];
        for ch in 0..3 {
            px[ch] = (color::srgb_encode(film::transmittance(1.0 + s * white, BASE[ch])) * 255.0).round() as u8;
        }
        Rgb(px)
    })
}

fn options(addr: &str, root: &std::path::Path, dialog: bool) -> AppOptions {
    // A stand-in module with a measurement block.
    let plugin = root.join("plugin.wasm");
    let mut module = b"\0asm".to_vec();
    module.extend_from_slice(film_lab_companion::MEASUREMENT_MARKER);
    module.extend(std::iter::repeat_n(b' ', film_lab_companion::MEASUREMENT_LEN - film_lab_companion::MEASUREMENT_MARKER.len()));
    std::fs::write(&plugin, module).unwrap();
    let mut set = Map::new();
    set.insert("preset".into(), json!("Ektar 100"));
    AppOptions {
        addr: addr.to_string(),
        token: TOKEN.into(),
        root: root.to_path_buf(),
        dialog,
        analyze: AnalyzeOptions { border_percent: 0.0, ..AnalyzeOptions::default() },
        set,
        plugin: Plugin::Convert,
        wasm: plugin,
    }
}

fn commands(log: &Mutex<Vec<(String, Value)>>) -> Vec<String> {
    log.lock().unwrap().iter().map(|(m, p)| if m == "engine.execute" { p["command"].as_str().unwrap().to_string() } else { m.clone() }).collect()
}

#[test]
fn converts_the_open_document_with_a_whole_image_analysis() {
    let root = std::env::temp_dir().join(format!("film-lab-app-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (addr, log) = stub(root.clone());
    let r = convert_in_app(&options(&addr, &root, false)).unwrap();
    assert_eq!(r["applied"]["plugin"], Plugin::Convert.id());
    let p = &r["params"];
    assert_eq!(p["preset"], "Ektar 100");
    assert!(p.get("base_r").is_none(), "the measurement is in the module, not the parameters");
    let base = color::srgb_encode(BASE[0]) * 255.0;
    assert!((r["measurement"]["base_r"].as_f64().unwrap() as f32 - base).abs() < 1.5, "{}", r["measurement"]);
    assert_eq!(commands(&log), ["auth", "ui.inspect", "image.duplicate", "app.save", "file.close", "ui.inspect", "plugin.install", "plugin.run"]);
    assert!(std::fs::read_dir(&root).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with("film-lab-")), "the copy is deleted");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn dialog_mode_fills_the_dialog_in() {
    let root = std::env::temp_dir().join(format!("film-lab-app-dialog-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (addr, log) = stub(root.clone());
    let r = convert_in_app(&options(&addr, &root, true)).unwrap();
    assert_eq!(r["dialog"], 7);
    let log = log.lock().unwrap();
    let sets: Vec<&Value> = log.iter().filter(|(m, _)| m == "ui.dialog.set").map(|(_, p)| p).collect();
    assert!(sets.iter().all(|p| p["dialog"] == 7));
    assert!(sets.iter().any(|p| p["field"] == "preset"), "the preset is set in the dialog");
    assert!(!sets.iter().any(|p| p["field"] == "base_r"), "the measurement is not: it is in the module");
    assert!(!log.iter().any(|(m, p)| m == "engine.execute" && p["command"] == "plugin.run"), "no run in dialog mode");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_advanced_plugin_is_opened_and_run_by_its_own_id() {
    let root = std::env::temp_dir().join(format!("film-lab-app-advanced-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (addr, log) = stub(root.clone());
    let mut o = options(&addr, &root, false);
    o.plugin = Plugin::Advanced;
    let r = convert_in_app(&o).unwrap();
    assert_eq!(r["applied"]["plugin"], Plugin::Advanced.id());
    let mut o = options(&addr, &root, true);
    o.plugin = Plugin::Advanced;
    convert_in_app(&o).unwrap();
    let log = log.lock().unwrap();
    let menu: Vec<&Value> = log.iter().filter(|(m, _)| m == "ui.menu.invoke").map(|(_, p)| &p["id"]).collect();
    assert_eq!(menu, [&json!(format!("plugin.filter.{}", Plugin::Advanced.id()))]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_bad_token_is_refused_before_anything_runs() {
    let root = std::env::temp_dir().join(format!("film-lab-app-token-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (addr, log) = stub(root.clone());
    let mut o = options(&addr, &root, false);
    o.token = "nope".into();
    let err = convert_in_app(&o).unwrap_err();
    assert!(err.contains("authentication refused"), "{err}");
    assert_eq!(commands(&log), ["auth"]);
    std::fs::remove_dir_all(&root).ok();
}
