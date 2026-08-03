// ── Interpreter-gated integration test ───────────────────────────
//
// Runs the full pipeline
//   lex → parse → semant → typecheck → lower → optimize → lua
// on std/prelude.plc + spec/hello.plc, writes the emitted Lua to a
// temp file, and executes it with a Lua interpreter found on PATH
// (lua5.4, lua5.3, lua, …).  If no interpreter is available the test
// skips cleanly instead of failing.

use std::process::Command;

use parlance_ir::{Ir, IrDef};

/// Find a Lua interpreter on PATH; returns its name or None.
fn find_lua() -> Option<&'static str> {
    for name in ["lua5.4", "lua5.3", "lua", "lua5.2", "lua5.1"] {
        let probe = Command::new(name).arg("-e").arg("print(1)").output();
        if let Ok(out) = probe {
            if out.status.success() {
                return Some(name);
            }
        }
    }
    None
}

const PRELUDE_FALLBACK: &str = r#"infix + 5 = add
infix - 5 = sub
infix * 6 = mul
infix / 6 = div
infix == 4 = eq

native add  : Int -> Int -> Int
native sub  : Int -> Int -> Int
native mul  : Int -> Int -> Int
native div  : Int -> Int -> Int
native print : a -> IO
"#;

const HELLO_FALLBACK: &str = r#"define main =
  var x <- 10       >>=
  var y <- x + 3    >>=
  print "Hello World"
"#;

const TABLE_FALLBACK: &str = r#"native table : Table

define main =
  var f <- table::foo          >>=
  var v <- table::index "cat"  >>=
  print (add f v)
"#;

/// Read a repo file relative to the crate root, falling back to an
/// embedded copy so the test stays hermetic.
fn repo_file(rel: &str, fallback: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|_| fallback.to_string())
}

#[test]
fn hello_plc_full_pipeline_runs_under_lua() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };

    // ── lex ────────────────────────────────────────────────────
    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let hello = repo_file("spec/hello.plc", HELLO_FALLBACK);
    let source = format!("{prelude}\n{hello}");

    let tokens = parlance_lexer::tokenize(&source).expect("lex");
    // ── parse ──────────────────────────────────────────────────
    let (stmts, _prec) = parlance_parser::Parser::new(tokens).parse().expect("parse");
    // ── semant ─────────────────────────────────────────────────
    let analyzed = parlance_semant::analyze(stmts).expect("semant");
    // ── typecheck ──────────────────────────────────────────────
    parlance_typecheck::typecheck(&analyzed).expect("typecheck");
    // ── lower ──────────────────────────────────────────────────
    let ir = parlance_ir::lower_program(&analyzed);
    // ── optimize ───────────────────────────────────────────────
    let optimized = parlance_optimize::optimize(&ir);
    // ── lua ────────────────────────────────────────────────────
    let lua_src = parlance_lua::compile(&optimized, Some("main"));

    // With strictness-aware β-reduction, the bind `var y <- x + 3`
    // survives as a redex `(function(y) ... end)(add(10)(3))` — the
    // pure computation is evaluated and discarded, preserving the
    // strict evaluation order.  The observable output is unchanged.
    assert!(
        lua_src.contains("print(\"Hello World\")"),
        "optimized main missing:\n{lua_src}"
    );

    // ── write + run ────────────────────────────────────────────
    let dir = std::env::temp_dir().join(format!("parlance_lua_it_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = dir.join("hello.lua");
    std::fs::write(&script, &lua_src).expect("write lua script");

    let out = Command::new(lua).arg(&script).output().expect("run lua");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "lua {lua} exited with {}: {stderr}\n--- generated lua ---\n{lua_src}",
        out.status
    );
    assert_eq!(
        stdout, "Hello World\n",
        "lua output mismatch\n--- generated lua ---\n{lua_src}"
    );
}

/// Two-argument index form: `X::index T K` → `T[K]`
/// (user example: `table::index tbl "cat"` → `tbl["cat"]`).
#[test]
fn table_index_two_arg_runs_under_lua() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let defs = vec![
        IrDef::Native {
            name: "print".into(),
            arity: 1,
        },
        IrDef::Bind {
            name: "main".into(),
            expr: Ir::App(
                Box::new(Ir::Var("print".into())),
                Box::new(Ir::App(
                    Box::new(Ir::App(
                        Box::new(Ir::Var("table::index".into())),
                        Box::new(Ir::Var("tbl".into())),
                    )),
                    Box::new(Ir::Str("cat".into())),
                )),
            ),
        },
    ];
    let mut lua_src = parlance_lua::compile(&defs, Some("main"));
    // A host-provided table, as a Lua global the program indexes.
    lua_src = lua_src.replace("-- entry point", "tbl = { cat = 42 }\n-- entry point");

    let dir = std::env::temp_dir().join(format!("parlance_lua_it2_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = dir.join("table_index.lua");
    std::fs::write(&script, &lua_src).expect("write lua script");
    let out = Command::new(lua).arg(&script).output().expect("run lua");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "lua {lua} exited with {}: {stderr}\n--- generated lua ---\n{lua_src}",
        out.status
    );
    assert_eq!(
        stdout, "42\n",
        "lua output mismatch\n--- generated lua ---\n{lua_src}"
    );
}

/// One-argument index form (`X::index K` → `X[K]`) plus `::`-qualified
/// native field population (`native table::foo : Int` → `table.foo`).
#[test]
fn table_index_one_arg_and_qualified_natives_run_under_lua() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let defs = vec![
        IrDef::Native {
            name: "print".into(),
            arity: 1,
        },
        IrDef::Native {
            name: "table::foo".into(),
            arity: 0,
        },
        IrDef::Bind {
            name: "main".into(),
            expr: Ir::App(
                Box::new(Ir::Var("print".into())),
                Box::new(Ir::App(
                    Box::new(Ir::Var("table::index".into())),
                    Box::new(Ir::Str("cat".into())),
                )),
            ),
        },
    ];
    let mut lua_src = parlance_lua::compile(&defs, Some("main"));
    // Extend the preamble the way a host would: implement the native
    // (so `table.foo = Native["table::foo"]()` no longer errors) and
    // provide the indexed key (table["cat"] === table.cat in Lua).
    lua_src = lua_src.replace(
        "Native[\"table::foo\"] = function(...) error(\"unknown native 'table::foo'\") end",
        "Native[\"table::foo\"] = function() return 7 end",
    );
    lua_src = lua_src.replace("-- entry point", "table.cat = 42\n-- entry point");
    assert!(
        lua_src.contains("table.foo = Native[\"table::foo\"]()"),
        "got:\n{lua_src}"
    );
    assert!(
        lua_src.contains("main = function() return print(table[\"cat\"]) end"),
        "got:\n{lua_src}"
    );

    let dir = std::env::temp_dir().join(format!("parlance_lua_it3_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = dir.join("table_index1.lua");
    std::fs::write(&script, &lua_src).expect("write lua script");
    let out = Command::new(lua).arg(&script).output().expect("run lua");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "lua {lua} exited with {}: {stderr}\n--- generated lua ---\n{lua_src}",
        out.status
    );
    assert_eq!(
        stdout, "42\n",
        "lua output mismatch\n--- generated lua ---\n{lua_src}"
    );
}

/// spec/table.plc end-to-end: `::` field/index access on the
/// native-provided `table` factory runs under a real Lua interpreter.
#[test]
fn table_plc_runs_under_lua() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };

    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let table = repo_file("spec/table.plc", TABLE_FALLBACK);
    let source = format!("{prelude}\n{table}");

    let tokens = parlance_lexer::tokenize(&source).expect("lex");
    let (stmts, _prec) = parlance_parser::Parser::new(tokens).parse().expect("parse");
    let analyzed = parlance_semant::analyze(stmts).expect("semant");
    parlance_typecheck::typecheck(&analyzed).expect("typecheck");
    let ir = parlance_ir::lower_program(&analyzed);
    let optimized = parlance_optimize::optimize(&ir);
    let lua_src = parlance_lua::compile(&optimized, Some("main"));

    assert!(
        lua_src.contains("table = Native.table()"),
        "factory native missing:\n{lua_src}"
    );
    assert!(
        !lua_src.contains("unknown native"),
        "unimplemented native in generated lua:\n{lua_src}"
    );

    let dir = std::env::temp_dir().join(format!("parlance_lua_it4_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = dir.join("table.lua");
    std::fs::write(&script, &lua_src).expect("write lua script");
    let out = Command::new(lua).arg(&script).output().expect("run lua");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "lua {lua} exited with {}: {stderr}\n--- generated lua ---\n{lua_src}",
        out.status
    );
    assert_eq!(
        stdout, "49\n",
        "lua output mismatch (expected 42 + 7)\n--- generated lua ---\n{lua_src}"
    );
}

/// Run a source program through the whole pipeline under a real Lua
/// interpreter and return its stdout (or panic with the generated Lua).
fn run_pipeline(lua: &str, source: &str) -> String {
    let tokens = parlance_lexer::tokenize(source).expect("lex");
    let (stmts, _prec) = parlance_parser::Parser::new(tokens).parse().expect("parse");
    let analyzed = parlance_semant::analyze(stmts).expect("semant");
    parlance_typecheck::typecheck(&analyzed).expect("typecheck");
    let ir = parlance_ir::lower_program(&analyzed);
    let optimized = parlance_optimize::optimize(&ir);
    let lua_src = parlance_lua::compile(&optimized, Some("main"));

    let dir = std::env::temp_dir().join(format!(
        "parlance_lua_it_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = dir.join("prog.lua");
    std::fs::write(&script, &lua_src).expect("write lua script");
    let out = Command::new(lua).arg(&script).output().expect("run lua");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "lua {lua} exited with {}: {stderr}\n--- generated lua ---\n{lua_src}",
        out.status
    );
    stdout
}

/// A user-defined function applied at runtime: the thunk-call fix must
/// make `double 21` evaluate the function, not the thunk.
#[test]
fn user_defined_function_application_runs_under_lua() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let src = format!(
        "{prelude}\ndefine double = \\x -> mul x 2\ndefine main = print (double 21)\n"
    );
    assert_eq!(run_pipeline(lua, &src), "42\n");
}

/// Higher-order application through a named combinator: every user
/// function is thunk-called, so `apply2 inc 10` runs.
#[test]
fn higher_order_application_runs_under_lua() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let src = format!(
        "{prelude}\n\
         define inc = \\x -> add x 1\n\
         define apply2 = \\f -> \\x -> f (f x)\n\
         define main = print (apply2 inc 10)\n"
    );
    assert_eq!(run_pipeline(lua, &src), "12\n");
}

/// Strictness fix: an unused bind whose value is effectful must NOT be
/// erased by β-reduction — both prints must appear, in order.
#[test]
fn effectful_unused_bind_is_not_erased() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let src = format!(
        "{prelude}\ndefine main = var p <- print 1 >>= print 42\n"
    );
    assert_eq!(run_pipeline(lua, &src), "1\n42\n");
}

/// Polymorphic natives: `print : a -> IO` used with Str AND Int in one
/// program (each use instantiates its own `a`).
#[test]
fn polymorphic_print_used_with_multiple_types() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let src = format!(
        "{prelude}\ndefine main = var p <- print \"side\" >>= print 42\n"
    );
    assert_eq!(run_pipeline(lua, &src), "side\n42\n");
}

/// Declared `::`-qualified natives whose left segment is the table
/// factory are type-level contracts: the README example must run
/// without load-time errors (42 + 7 = 49).
#[test]
fn declared_qualified_natives_resolve_through_factory() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let src = format!(
        "{prelude}\n\
         native table : Table\n\
         native table::foo : Int\n\
         native table::index : Str -> Int\n\
         define main = print (add table::foo (table::index \"cat\"))\n"
    );
    assert_eq!(run_pipeline(lua, &src), "49\n");
}

/// Line comments (`# ...`) must lex away and not break parsing.
#[test]
fn comments_parse_and_run() {
    let Some(lua) = find_lua() else {
        eprintln!("skipping integration test: no Lua interpreter on PATH");
        return;
    };
    let prelude = repo_file("std/prelude.plc", PRELUDE_FALLBACK);
    let src = format!(
        "{prelude}\n\
         # a full-line comment\n\
         define main = print 7 # trailing comment\n"
    );
    assert_eq!(run_pipeline(lua, &src), "7\n");
}
