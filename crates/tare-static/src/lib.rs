//! `tare-static` — syn-based allocation-site analyzer.
//!
//! Walks Rust source files and flags known allocation **sites**. These are
//! sites, never amounts — heap amounts are runtime values and fundamentally
//! unknowable statically.

use std::collections::BTreeMap;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall, Macro};
use tare_schema::{Entry, FileData, LineData};

/// Detected allocation site on a specific line.
#[derive(Debug, Clone)]
pub struct AllocSite {
    pub line: usize,
    pub construct: String,
    pub amount_hint: Option<String>,
}

/// Analyze a single Rust source file and return the detected allocation sites.
pub fn analyze_file(source: &str) -> Vec<AllocSite> {
    let Ok(file) = syn::parse_file(source) else {
        return Vec::new();
    };
    let mut visitor = AllocVisitor::default();
    visitor.visit_file(&file);
    visitor.sites
}

/// Analyze a source file and return a `FileData` suitable for the schema.
pub fn analyze_to_file_data(source: &str, path: &Path) -> FileData {
    let content_hash = blake3::hash(source.as_bytes()).to_hex().to_string();
    let sites = analyze_file(source);

    let mut lines: BTreeMap<String, LineData> = BTreeMap::new();
    for site in sites {
        let line_data = lines
            .entry(site.line.to_string())
            .or_insert_with(|| LineData {
                entries: Vec::new(),
            });

        // Check if there's already an alloc_site entry for this line —
        // if so, merge the construct into it rather than creating a duplicate.
        let existing = line_data.entries.iter_mut().find(|e| {
            e.source == tare_schema::Source::Static && e.kind == tare_schema::Kind::AllocSite
        });

        if let Some(entry) = existing {
            if let Some(ref mut constructs) = entry.constructs {
                constructs.push(site.construct);
            }
            // If the new site has an amount_hint and the existing one doesn't, add it.
            if entry.amount_hint.is_none() && site.amount_hint.is_some() {
                entry.amount_hint = site.amount_hint;
            }
        } else {
            line_data.entries.push(Entry::static_alloc_site(
                vec![site.construct],
                site.amount_hint,
            ));
        }
    }

    let _ = path; // path available for future use (e.g., type_size entries)

    FileData {
        content_hash,
        lines,
    }
}

/// AST visitor that detects allocation-site patterns.
#[derive(Default)]
struct AllocVisitor {
    sites: Vec<AllocSite>,
}

impl AllocVisitor {
    fn record(&mut self, line: usize, construct: &str, amount_hint: Option<&str>) {
        self.sites.push(AllocSite {
            line,
            construct: construct.to_string(),
            amount_hint: amount_hint.map(|s| s.to_string()),
        });
    }

    fn span_line(span: proc_macro2::Span) -> usize {
        span.start().line
    }
}

impl<'ast> Visit<'ast> for AllocVisitor {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Some(construct) = classify_call(node) {
            let line = Self::span_line(node.func.span());
            self.record(line, &construct.name, construct.hint.as_deref());
        }
        // Continue visiting children.
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if let Some(construct) = classify_method_call(node) {
            let line = Self::span_line(node.method.span());
            self.record(line, &construct.name, construct.hint.as_deref());
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        if let Some(construct) = classify_macro(node) {
            let line = node
                .path
                .segments
                .last()
                .map(|s| Self::span_line(s.ident.span()))
                .unwrap_or(1);
            self.record(line, &construct.name, construct.hint.as_deref());
        }
        syn::visit::visit_macro(self, node);
    }
}

struct Construct {
    name: String,
    hint: Option<String>,
}

impl Construct {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            hint: None,
        }
    }

    fn with_hint(name: &str, hint: &str) -> Self {
        Self {
            name: name.to_string(),
            hint: Some(hint.to_string()),
        }
    }
}

/// Match `Type::function(...)` call patterns.
fn classify_call(call: &ExprCall) -> Option<Construct> {
    let path = match call.func.as_ref() {
        Expr::Path(p) => p,
        _ => return None,
    };

    let segments: Vec<_> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();

    match segments.as_slice() {
        // Box::new(x)
        [ty, func] if ty == "Box" && func == "new" => Some(Construct::new("Box::new")),

        // Vec::new(), Vec::with_capacity(n)
        [ty, func] if ty == "Vec" && func == "new" => Some(Construct::new("Vec::new")),
        [ty, func] if ty == "Vec" && func == "with_capacity" => {
            Some(Construct::with_hint("Vec::with_capacity", "capacity × element size"))
        }

        // String::new(), String::with_capacity(n), String::from(x)
        [ty, func] if ty == "String" && func == "new" => Some(Construct::new("String::new")),
        [ty, func] if ty == "String" && func == "with_capacity" => {
            Some(Construct::with_hint("String::with_capacity", "capacity bytes"))
        }
        [ty, func] if ty == "String" && func == "from" => Some(Construct::new("String::from")),

        // HashMap::new(), HashMap::with_capacity(n)
        [ty, func] if ty == "HashMap" && func == "new" => Some(Construct::new("HashMap::new")),
        [ty, func] if ty == "HashMap" && func == "with_capacity" => {
            Some(Construct::with_hint("HashMap::with_capacity", "capacity × entry size"))
        }

        // HashSet::new(), HashSet::with_capacity(n)
        [ty, func] if ty == "HashSet" && func == "new" => Some(Construct::new("HashSet::new")),
        [ty, func] if ty == "HashSet" && func == "with_capacity" => {
            Some(Construct::with_hint("HashSet::with_capacity", "capacity × entry size"))
        }

        // BTreeMap::new(), BTreeSet::new()
        [ty, func] if ty == "BTreeMap" && func == "new" => Some(Construct::new("BTreeMap::new")),
        [ty, func] if ty == "BTreeSet" && func == "new" => Some(Construct::new("BTreeSet::new")),

        // VecDeque::new(), VecDeque::with_capacity(n)
        [ty, func] if ty == "VecDeque" && func == "new" => Some(Construct::new("VecDeque::new")),
        [ty, func] if ty == "VecDeque" && func == "with_capacity" => {
            Some(Construct::with_hint("VecDeque::with_capacity", "capacity × element size"))
        }

        // Rc::new(x), Arc::new(x)
        [ty, func] if ty == "Rc" && func == "new" => Some(Construct::new("Rc::new")),
        [ty, func] if ty == "Arc" && func == "new" => Some(Construct::new("Arc::new")),

        _ => None,
    }
}

/// Match `.method(...)` call patterns.
fn classify_method_call(call: &ExprMethodCall) -> Option<Construct> {
    let method = call.method.to_string();
    match method.as_str() {
        "clone" => Some(Construct::new(".clone()")),
        "to_owned" => Some(Construct::new(".to_owned()")),
        "to_string" => Some(Construct::new(".to_string()")),
        "to_vec" => Some(Construct::new(".to_vec()")),
        "into_boxed_slice" => Some(Construct::new(".into_boxed_slice()")),
        "collect" => Some(Construct::new(".collect()")),
        _ => None,
    }
}

/// Match macro invocations that allocate.
fn classify_macro(mac: &Macro) -> Option<Construct> {
    let name = mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())?;

    match name.as_str() {
        "vec" => Some(Construct::new("vec!")),
        "format" => Some(Construct::new("format!")),
        "format_args" => None, // does not allocate by itself
        "string" => Some(Construct::new("string!")), // uncommon but possible
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_vec_with_capacity() {
        let src = r#"
fn foo() {
    let v = Vec::with_capacity(10);
}
"#;
        let sites = analyze_file(src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].construct, "Vec::with_capacity");
        assert!(sites[0].amount_hint.is_some());
    }

    #[test]
    fn detects_box_new() {
        let src = r#"
fn foo() {
    let b = Box::new(42);
}
"#;
        let sites = analyze_file(src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].construct, "Box::new");
    }

    #[test]
    fn detects_collect() {
        let src = r#"
fn foo() {
    let v: Vec<i32> = (0..10).collect();
}
"#;
        let sites = analyze_file(src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].construct, ".collect()");
    }

    #[test]
    fn detects_clone() {
        let src = r#"
fn foo(s: &String) {
    let c = s.clone();
}
"#;
        let sites = analyze_file(src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].construct, ".clone()");
    }

    #[test]
    fn detects_format_macro() {
        let src = r#"
fn foo() {
    let s = format!("hello {}", 42);
}
"#;
        let sites = analyze_file(src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].construct, "format!");
    }

    #[test]
    fn detects_vec_macro() {
        let src = r#"
fn foo() {
    let v = vec![1, 2, 3];
}
"#;
        let sites = analyze_file(src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].construct, "vec!");
    }

    #[test]
    fn detects_rc_arc_new() {
        let src = r#"
use std::rc::Rc;
use std::sync::Arc;
fn foo() {
    let r = Rc::new(42);
    let a = Arc::new("hello");
}
"#;
        let sites = analyze_file(src);
        let names: Vec<_> = sites.iter().map(|s| s.construct.as_str()).collect();
        assert!(names.contains(&"Rc::new"));
        assert!(names.contains(&"Arc::new"));
    }

    #[test]
    fn detects_to_owned_to_string() {
        let src = r#"
fn foo(s: &str) {
    let a = s.to_owned();
    let b = s.to_string();
}
"#;
        let sites = analyze_file(src);
        let names: Vec<_> = sites.iter().map(|s| s.construct.as_str()).collect();
        assert!(names.contains(&".to_owned()"));
        assert!(names.contains(&".to_string()"));
    }

    #[test]
    fn detects_hashmap_new() {
        let src = r#"
use std::collections::HashMap;
fn foo() {
    let m = HashMap::new();
}
"#;
        let sites = analyze_file(src);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].construct, "HashMap::new");
    }

    #[test]
    fn merges_constructs_on_same_line() {
        let src = r#"
fn foo(data: &[String]) {
    let v: Vec<String> = data.iter().map(|s| s.clone()).collect();
}
"#;
        let file_data = analyze_to_file_data(src, Path::new("test.rs"));
        // .clone() and .collect() are on the same line → should be merged
        // into one entry with two constructs.
        let line3 = file_data.lines.get("3").expect("should have line 3");
        assert_eq!(line3.entries.len(), 1);
        let constructs = line3.entries[0].constructs.as_ref().unwrap();
        assert_eq!(constructs.len(), 2);
        assert!(constructs.contains(&".clone()".to_string()));
        assert!(constructs.contains(&".collect()".to_string()));
    }

    #[test]
    fn content_hash_is_blake3() {
        let src = "fn main() {}";
        let file_data = analyze_to_file_data(src, Path::new("test.rs"));
        let expected = blake3::hash(src.as_bytes()).to_hex().to_string();
        assert_eq!(file_data.content_hash, expected);
    }

    #[test]
    fn no_false_positives_on_non_alloc_methods() {
        let src = r#"
fn foo() {
    let x = bar.len();
    let y = baz.is_empty();
    let z = qux.iter();
}
"#;
        let sites = analyze_file(src);
        assert!(sites.is_empty(), "got unexpected sites: {:?}", sites);
    }

    #[test]
    fn sample_lib_expected_sites() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("sample/src/lib.rs"),
        )
        .unwrap();

        let sites = analyze_file(&src);
        let by_line: BTreeMap<usize, Vec<String>> = {
            let mut m: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for s in &sites {
                m.entry(s.line)
                    .or_insert_with(Vec::new)
                    .push(s.construct.clone());
            }
            m
        };

        let has = |line: usize, name: &str| -> bool {
            by_line
                .get(&line)
                .map_or(false, |c| c.iter().any(|s| s == name))
        };

        // Line 12: Vec::with_capacity
        assert!(
            has(12, "Vec::with_capacity"),
            "expected Vec::with_capacity on line 12, got: {by_line:?}"
        );

        // Line 25: Box::new
        assert!(
            has(25, "Box::new"),
            "expected Box::new on line 25, got: {by_line:?}"
        );

        // Line 40: .collect()
        assert!(
            has(40, ".collect()"),
            "expected .collect() on line 40, got: {by_line:?}"
        );

        // Line 45: .clone() and .collect()
        assert!(
            has(45, ".clone()"),
            "expected .clone() on line 45, got: {by_line:?}"
        );
        assert!(
            has(45, ".collect()"),
            "expected .collect() on line 45, got: {by_line:?}"
        );

        // Line 50: format!
        assert!(
            has(50, "format!"),
            "expected format! on line 50, got: {by_line:?}"
        );
    }
}
