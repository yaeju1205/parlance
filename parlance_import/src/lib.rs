// ── Import Resolution (module system) ────────────────────────────
//
// MODULE SYSTEM THEORY:
//
//   IMPORT RESOLUTION is the process of transforming an `import`
//   statement into a concrete set of bindings from another source
//   file.  It is a form of STATIC LINKING: names are resolved at
//   compile time, before semantic analysis.
//
//   ALGORITHM (recursive-descent style):
//
//     1. Parse the import statement → (path, spec)
//     2. Locate the source file on disk
//     3. Lex and parse the file → Vec<Stmt>
//     4. Collect all exported names:
//          - each `define name = ...` → Export::Define { name, expr }
//          - each `infix op bp = ...` → Export::Infix { op, bp, func }
//     5. Apply the import spec filter:
//          - ImportSpec::All        → keep all
//          - ImportSpec::Only(list) → keep only names in list
//     6. Return the resolved Module
//
//   CYCLE DETECTION:
//     A set of paths currently being resolved is threaded through
//     the resolver.  If a file is encountered again while it is
//     still on the call stack, a cycle error is reported.
//
//     Parsing an imported file may itself contain imports — those
//     are resolved recursively through the same resolver function.
//
//   RESOLUTION ORDER:
//     Files are resolved depth-first.  Imports within a file are
//     processed left-to-right.  Later definitions shadow earlier
//     ones (including imports from different files).
//
//   SEARCH PATH:
//     The resolver searches for files in:
//       1. The directory of the importing file
//       2. Any paths in the search_paths list
//     The file extension `.plc` is appended if not present.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use parlance_parser::ast::{Export, ImportSpec, Module, Stmt};

/// Global resolver state for cycle detection across recursive calls.
struct Resolver {
    /// Set of canonical file paths currently on the resolution call stack.
    visiting: HashSet<PathBuf>,
    /// Directories to search when resolving import paths.
    search_paths: Vec<PathBuf>,
}

// Thread-safe singleton for the resolver state.
// We use a mutex so the resolver can be called from multiple contexts.
// In practice, a compiler pass calls it sequentially.
static RESOLVER: LazyLock<Mutex<Option<Resolver>>> = LazyLock::new(|| Mutex::new(None));

fn with_resolver<F, T>(f: F) -> T
where
    F: FnOnce(&mut Resolver) -> T,
{
    let mut guard = RESOLVER.lock().unwrap();
    let resolver = guard.get_or_insert_with(|| Resolver {
        visiting: HashSet::new(),
        search_paths: vec![],
    });
    f(resolver)
}

/// Set the search paths for import resolution.
/// Call this once before resolving any imports.
pub fn set_search_paths(paths: Vec<PathBuf>) {
    with_resolver(|r| {
        r.search_paths = paths;
    });
}

/// Resolve a single import statement to its Module.
///
/// Returns the resolved module containing only the exports
/// that match the import spec.
pub fn resolve_import(import_stmt: &Stmt, source_dir: &Path) -> Result<Module, String> {
    match import_stmt {
        Stmt::Import { path, spec } => {
            let resolved_path = locate_file(path, source_dir)?;
            let canon = resolved_path
                .canonicalize()
                .map_err(|e| format!("cannot canonicalize '{}': {e}", resolved_path.display()))?;

            // Cycle detection
            with_resolver(|r| {
                if r.visiting.contains(&canon) {
                    return Err(format!("cyclic import detected: '{}'", canon.display()));
                }
                r.visiting.insert(canon.clone());
                Ok(())
            })?;

            // Parse the file
            let source = std::fs::read_to_string(&resolved_path)
                .map_err(|e| format!("cannot read '{}': {e}", resolved_path.display()))?;
            let mut stmts = parlance_parser::parse_program(&source)
                .map(|(s, _)| s)
                .map_err(|e| format!("in '{}': {e}", resolved_path.display()))?;

            // Recursively resolve imports within the file
            let mut resolved_stmts = Vec::new();
            for stmt in stmts.drain(..) {
                match &stmt {
                    Stmt::Import { .. } => {
                        // Resolve nested imports, extract their exports as
                        // Define statements for the current scope.
                        let sub_dir = resolved_path.parent().unwrap_or(Path::new("."));
                        let sub_mod = resolve_import(&stmt, sub_dir)?;
                        for export in sub_mod.exports {
                            match export {
                                Export::Define { name, expr } => {
                                    resolved_stmts.push(Stmt::Define { name, expr, type_sig: None });
                                }
                                Export::Infix { op, strength, func } => {
                                    resolved_stmts.push(Stmt::Infix { op, strength, func });
                                }
                            }
                        }
                    }
                    _ => {
                        resolved_stmts.push(stmt);
                    }
                }
            }

            // Collect exports
            let all_exports = collect_exports(&resolved_stmts);

            // Remove from visiting set
            with_resolver(|r| {
                r.visiting.remove(&canon);
            });

            // Apply the import spec filter
            let exports = filter_exports(all_exports, spec)?;

            Ok(Module {
                path: path.clone(),
                exports,
            })
        }
        other => Err(format!(
            "resolve_import called on non-import statement: {other}"
        )),
    }
}

/// Collect all exports from a list of statements.
/// Every `define` and `infix` at the top level is an export.
pub fn collect_exports(stmts: &[Stmt]) -> Vec<Export> {
    let mut exports = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Define { name, expr, .. } => {
                exports.push(Export::Define {
                    name: name.clone(),
                    expr: expr.clone(),
                });
            }
            Stmt::Infix { op, strength, func } => {
                exports.push(Export::Infix {
                    op: op.clone(),
                    strength: *strength,
                    func: func.clone(),
                });
            }
            Stmt::Import { .. } => {
                // Already resolved above; skip.
            }
            Stmt::Native { .. } => {
                // Native declarations are not exported.
            }
        }
    }
    exports
}

/// Apply an import spec filter to a list of exports.
fn filter_exports(exports: Vec<Export>, spec: &ImportSpec) -> Result<Vec<Export>, String> {
    match spec {
        ImportSpec::All => Ok(exports),
        ImportSpec::Only(names) => {
            let name_set: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
            let mut filtered = Vec::new();
            for export in exports {
                let export_name = match &export {
                    Export::Define { name, .. } => name.as_str(),
                    Export::Infix { op, .. } => op.as_str(),
                };
                if name_set.contains(export_name) {
                    filtered.push(export);
                }
            }
            // Check if any requested names were not found
            let found_names: HashSet<&str> = filtered
                .iter()
                .map(|e| match e {
                    Export::Define { name, .. } => name.as_str(),
                    Export::Infix { op, .. } => op.as_str(),
                })
                .collect();
            for name in names {
                if !found_names.contains(name.as_str()) {
                    return Err(format!("import '{}' not found in module", name));
                }
            }
            Ok(filtered)
        }
    }
}

/// Locate a source file by its import path.
///
/// Search order:
///   1. source_dir / path.plc
///   2. source_dir / path  (if it has an extension)
///   3. each search_path / path.plc
fn locate_file(path: &str, source_dir: &Path) -> Result<PathBuf, String> {
    let _file_path = Path::new(path);

    // Direct file path with or without extension
    let candidates = with_resolver(|r| {
        let mut v = Vec::new();
        // Try in the source directory first
        v.push(source_dir.join(path).with_extension("plc"));
        v.push(source_dir.join(path));
        // Then in each search path
        for sp in &r.search_paths {
            v.push(sp.join(path).with_extension("plc"));
            v.push(sp.join(path));
        }
        v
    });

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(format!(
        "cannot find import '{}' (searched in {:?})",
        path, source_dir
    ))
}

// ── Self-test ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use parlance_parser::ast::*;

    #[test]
    fn test_collect_exports() {
        let stmts = vec![
            Stmt::Define {
                name: "x".into(),
                expr: Expr::Int(42),
                type_sig: None,
            },
            Stmt::Infix {
                op: "+".into(),
                strength: 5,
                func: Expr::Var("add".into()),
            },
        ];
        let exports = collect_exports(&stmts);
        assert_eq!(exports.len(), 2);
    }

    #[test]
    fn test_filter_exports_all() {
        let exports = vec![
            Export::Define {
                name: "x".into(),
                expr: Expr::Int(1),
            },
            Export::Define {
                name: "y".into(),
                expr: Expr::Int(2),
            },
        ];
        let filtered = filter_exports(exports, &ImportSpec::All).unwrap();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_exports_only() {
        let exports = vec![
            Export::Define {
                name: "x".into(),
                expr: Expr::Int(1),
            },
            Export::Define {
                name: "y".into(),
                expr: Expr::Int(2),
            },
        ];
        let filtered = filter_exports(exports, &ImportSpec::Only(vec!["x".into()])).unwrap();
        assert_eq!(filtered.len(), 1);
        match &filtered[0] {
            Export::Define { name, .. } => assert_eq!(name, "x"),
            _ => panic!("expected define"),
        }
    }

    #[test]
    fn test_filter_exports_only_missing() {
        let exports = vec![Export::Define {
            name: "x".into(),
            expr: Expr::Int(1),
        }];
        let result = filter_exports(exports, &ImportSpec::Only(vec!["z".into()]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
