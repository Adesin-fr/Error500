//! Vue d'un événement pour l'interface : exceptions et frames, résolus via les source maps.

use crate::sourcemaps::{resolve_frame, MapSet};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Location {
    pub filename: String,
    pub lineno: Option<u32>,
    pub colno: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FrameView {
    pub filename: Option<String>,
    pub function: Option<String>,
    pub lineno: Option<u32>,
    pub colno: Option<u32>,
    pub in_app: bool,
    pub pre_context: Vec<String>,
    pub context_line: Option<String>,
    pub post_context: Vec<String>,
    /// Vrai si le frame a été remappé vers le code d'origine par une source map.
    pub resolved: bool,
    /// Position dans le fichier minifié, quand `resolved`.
    pub minified: Option<Location>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExceptionView {
    #[serde(rename = "type")]
    pub ty: Option<String>,
    pub value: Option<String>,
    /// Frames du plus récent au plus ancien.
    pub frames: Vec<FrameView>,
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

fn opt_u32(v: &Value, key: &str) -> Option<u32> {
    v.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok())
}

fn str_list(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Renvoie le frame et le nom de fonction que la source map donne *à la position de ce frame*.
/// Ce nom est celui de la fonction appelée à cet endroit, donc celui du frame suivant (voir `build_view`).
fn frame_view(frame: &Value, maps: &MapSet) -> (FrameView, Option<String>) {
    let filename = opt_str(frame, "filename").or_else(|| opt_str(frame, "abs_path"));
    let in_app = frame
        .get("in_app")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| !filename.as_deref().unwrap_or("").contains("node_modules"));
    let mut view = FrameView {
        function: opt_str(frame, "function"),
        lineno: opt_u32(frame, "lineno"),
        colno: opt_u32(frame, "colno"),
        pre_context: str_list(frame, "pre_context"),
        context_line: opt_str(frame, "context_line"),
        post_context: str_list(frame, "post_context"),
        filename,
        in_app,
        resolved: false,
        minified: None,
    };

    let resolved = opt_str(frame, "abs_path")
        .and_then(|path| maps.get(&path))
        .zip(view.lineno.zip(view.colno))
        .and_then(|(map, (line, col))| resolve_frame(map, line, col));
    let mut call_site_name = None;
    if let Some(r) = resolved {
        call_site_name = r.function;
        view.minified = Some(Location {
            filename: view.filename.take().unwrap_or_default(),
            lineno: view.lineno,
            colno: view.colno,
        });
        view.filename = Some(r.filename);
        view.lineno = Some(r.lineno);
        view.colno = Some(r.colno);
        view.pre_context = r.pre_context;
        view.context_line = r.context_line;
        view.post_context = r.post_context;
        view.resolved = true;
    }
    (view, call_site_name)
}

/// Exceptions (la plus récente d'abord) avec leurs frames (le plus récent d'abord).
pub fn build_view(payload: &Value, maps: &MapSet) -> Vec<ExceptionView> {
    let Some(exc) = payload.get("exception") else { return Vec::new() };
    let values = exc.get("values").unwrap_or(exc).as_array().cloned().unwrap_or_default();
    values
        .iter()
        .rev()
        .map(|ex| {
            let raw = ex.get("stacktrace").and_then(|s| s.get("frames")).and_then(Value::as_array);
            // Du plus ancien au plus récent : le nom lu à la position d'un frame nomme le frame suivant.
            let mut caller_name: Option<String> = None;
            let mut frames: Vec<FrameView> = raw
                .map(|frames| {
                    frames
                        .iter()
                        .map(|f| {
                            let (mut view, call_site_name) = frame_view(f, maps);
                            if view.resolved && caller_name.is_some() {
                                view.function = caller_name.take();
                            }
                            caller_name = call_site_name;
                            view
                        })
                        .collect()
                })
                .unwrap_or_default();
            frames.reverse();
            ExceptionView { ty: opt_str(ex, "type"), value: opt_str(ex, "value"), frames }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sourcemaps::MapSet;
    use serde_json::json;

    const MAP: &str = r#"{"version":3,"sources":["src/app.ts"],"names":["handleClick"],
        "sourcesContent":["line1\nline2\nfunction handleClick() {\n  throw 1\n}\nline6"],"mappings":"SAEIA"}"#;

    fn payload() -> serde_json::Value {
        json!({"exception": {"values": [
            {"type": "Error", "value": "cause", "stacktrace": {"frames": [
                {"filename": "a.js", "function": "inner", "lineno": 3, "in_app": true}]}},
            {"type": "TypeError", "value": "boom", "stacktrace": {"frames": [
                {"filename": "node_modules/lib/index.js", "function": "lib", "lineno": 1, "colno": 1, "in_app": false},
                {"abs_path": "https://app.example.com/static/app.min.js", "filename": "app.min.js",
                 "function": "t", "lineno": 1, "colno": 10, "in_app": true,
                 "context_line": "minified", "pre_context": ["p"], "post_context": ["n"]}]}}
        ]}})
    }

    fn maps() -> MapSet {
        let mut m = MapSet::default();
        m.insert("https://app.example.com/static/app.min.js", sourcemap::SourceMap::from_slice(MAP.as_bytes()).unwrap());
        m
    }

    #[test]
    fn most_recent_exception_and_frame_come_first() {
        let v = build_view(&payload(), &MapSet::default());
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].ty.as_deref(), Some("TypeError"));
        assert_eq!(v[1].ty.as_deref(), Some("Error"));
        assert_eq!(v[0].frames[0].function.as_deref(), Some("t"));
        assert_eq!(v[0].frames[1].function.as_deref(), Some("lib"));
    }

    #[test]
    fn unresolved_frames_keep_their_own_context() {
        let f = &build_view(&payload(), &MapSet::default())[0].frames[0];
        assert!(!f.resolved);
        assert_eq!(f.filename.as_deref(), Some("app.min.js"));
        assert_eq!(f.context_line.as_deref(), Some("minified"));
        assert_eq!(f.pre_context, vec!["p"]);
        assert!(f.in_app);
    }

    #[test]
    fn resolved_frames_use_original_source_and_keep_minified_location() {
        let f = &build_view(&payload(), &maps())[0].frames[0];
        assert!(f.resolved);
        assert_eq!(f.filename.as_deref(), Some("src/app.ts"));
        assert_eq!(f.lineno, Some(3));
        assert_eq!(f.colno, Some(5));
        // sans appelant remappé, le nom reste celui du frame d'origine
        assert_eq!(f.function.as_deref(), Some("t"));
        assert_eq!(f.context_line.as_deref(), Some("function handleClick() {"));
        let min = f.minified.as_ref().unwrap();
        assert_eq!((min.filename.as_str(), min.lineno, min.colno), ("app.min.js", Some(1), Some(10)));
    }

    fn chain(names: [&str; 3]) -> serde_json::Value {
        let url = "https://app.example.com/static/app.min.js";
        let frame = |f: &str| json!({"abs_path": url, "filename": url, "function": f, "lineno": 1, "colno": 10, "in_app": true});
        // ordre Sentry : du plus ancien (appelant) au plus récent
        json!({"exception": {"values": [{"type": "E", "stacktrace": {"frames": [frame(names[0]), frame(names[1]), frame(names[2])]}}]}})
    }

    #[test]
    fn function_name_comes_from_the_call_site_in_the_caller_frame() {
        // Le nom d'une map, à une position, est celui de la fonction appelée là : il nomme le frame suivant.
        let frames = &build_view(&chain(["a", "b", "c"]), &maps())[0].frames; // plus récent d'abord
        assert_eq!(frames[0].function.as_deref(), Some("handleClick"), "c nommé par l'appel dans b");
        assert_eq!(frames[1].function.as_deref(), Some("handleClick"), "b nommé par l'appel dans a");
        assert_eq!(frames[2].function.as_deref(), Some("a"), "a n'a pas d'appelant remappé");
    }

    #[test]
    fn unresolved_callee_keeps_its_own_name() {
        let mut p = chain(["a", "b", "c"]);
        p["exception"]["values"][0]["stacktrace"]["frames"][2]["lineno"] = json!(99);
        let frames = &build_view(&p, &maps())[0].frames;
        assert_eq!(frames[0].function.as_deref(), Some("c"));
        assert!(!frames[0].resolved);
    }

    #[test]
    fn frame_not_covered_by_the_map_stays_unresolved() {
        let mut p = payload();
        p["exception"]["values"][1]["stacktrace"]["frames"][1]["lineno"] = json!(99);
        let f = &build_view(&p, &maps())[0].frames[0];
        assert!(!f.resolved);
        assert_eq!(f.lineno, Some(99));
    }

    #[test]
    fn in_app_defaults_to_false_for_node_modules() {
        let p = json!({"exception": {"values": [{"type": "E", "stacktrace": {"frames": [
            {"filename": "node_modules/x.js"}, {"filename": "src/y.js"}]}}]}});
        let frames = &build_view(&p, &MapSet::default())[0].frames;
        assert!(frames[0].in_app, "src/y.js (dernier frame = premier affiché)");
        assert!(!frames[1].in_app);
    }

    #[test]
    fn event_without_exception_gives_empty_view() {
        assert!(build_view(&json!({"message": "hi"}), &MapSet::default()).is_empty());
    }

    #[test]
    fn exception_without_stacktrace_has_no_frames() {
        let v = build_view(&json!({"exception": {"values": [{"type": "E", "value": "v"}]}}), &MapSet::default());
        assert!(v[0].frames.is_empty());
    }
}
