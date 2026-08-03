// ── parlance_lua: Parlance IR → self-contained Lua source ────────
//
// CODEGEN THEORY — HOMOMORPHISM:
//
//   parlance_lua translates the λ-calculus IR into a self-contained
//   Lua source file (targeting Lua 5.4, source-compatible with 5.1+).
//   Each IR node maps to exactly one Lua construct:
//
//     Ir::Int(n)    →  Lua integer literal
//     Ir::Float(n)  →  Lua number literal
//     Ir::Str(s)    →  Lua string literal (fully escaped)
//     Ir::Var(name) →  env-renamed Lua name, or `::` table access
//     Ir::Lam{p,b}  →  function(p) return <body> end
//     Ir::App(f,a)  →  <f>(<a>)
//
//   `::` TABLE ACCESS:
//
//     Var("t::foo")              →  t.foo          (a::b::c → a.b.c)
//     App(Var("X::index"), K)        →  X[K]
//     App(App(Var("X::index"), T),K) →  T[K]   (e.g. table::index tbl "cat")
//
//   The left segment of a `::` name is renamed through the SAME env
//   mapping as plain names, so `t` in `t::foo` refers to the same
//   renamed variable as a plain `t`.
//
//   NAME RENAMING (env-stack alpha-renaming):
//
//     Identifiers containing `'` and names that collide with Lua
//     keywords (local, end, function, …) or with preamble globals
//     are mapped to fresh, valid Lua names.  Every `Lam` pushes a
//     fresh binding for its parameter, so shadowing is preserved
//     exactly (plain names keep their spelling and Lua's own lexical
//     scoping rules then mirror λ-shadowing).
//
//   TOP-LEVEL DEFINITIONS:
//
//     IrDef::Bind{name, expr} →  name = function() return <expr> end
//     (every definition is a thunk — mirroring GraftVM, where each
//     definition is a callable function and the entry point calls it)
//     with ALL definition names pre-declared once at the top
//     (`local a, b, …`) so forward references resolve.
//     IrDef::Infix is skipped (it is parser-level sugar).
//
//     `::`-QUALIFIED DEFINITIONS (e.g. `native table::foo : Int`):
//     the definition lives at a table field of its left segment.
//     The left segment becomes a local table (`table = {}`) when
//     nothing else defines it, and the native is populated onto it
//     (`table.foo = Native["table::foo"]()` / a curried wrapper) —
//     the field a `Var("table::foo")` access reads.
//
//   NATIVE FUNCTIONS:
//
//     IrDef::Native{name, arity} → a curried Lua wrapper delegating
//     to a runtime `Native` table emitted in the preamble.  The
//     preamble implements the prelude natives add/sub/mul/div
//     (integer division) and print (io.write, returns its argument,
//     mirroring GraftVM main.rs); any other native name gets a
//     runtime-error stub (extendable, e.g. for a `table` factory).
//     A zero-arity native is a pure value and is evaluated once at
//     load (`random = Native.random()`), which lets a factory bind a
//     Lua table (or value) directly to its name.
//
//   ENTRY POINT:
//
//     compile(defs, Some("main")) appends a final `main()` call;
//     Some(other) appends that name's call; None omits it.  When the
//     entry name is not defined, no call is emitted (mirroring the
//     GraftVM codegen's behaviour).

use std::collections::{HashMap, HashSet};

use parlance_ir::{Ir, IrDef};

// ── Lexical facts ────────────────────────────────────────────────

/// Prelude natives implemented directly in the preamble `Native` table.
const PRELUDE_NATIVES: [&str; 5] = ["add", "sub", "mul", "div", "print"];

/// The native table factory implemented by the preamble.  A zero-arity
/// native named `table` is a pure *value*: `table = Native.table()`
/// binds a fresh Lua table whose fields/indices the `::` forms read
/// (`table::foo`, `table::index "cat"`).
const TABLE_FACTORY: &str = "table";

/// Lua reserved words — never usable as bare identifiers.
const LUA_KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Globals referenced by the generated preamble.  User definitions are
/// renamed away from these so they can never shadow the runtime.
const RESERVED_GLOBALS: [&str; 5] = ["Native", "io", "math", "tostring", "error"];

fn is_lua_keyword(s: &str) -> bool {
    LUA_KEYWORDS.contains(&s)
}

/// A name that can be emitted verbatim as a Lua identifier.
fn is_valid_lua_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Escape a string as a Lua string literal (works on Lua 5.1+).
fn lua_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            // 3-digit decimal escapes: unambiguous even when a digit
            // follows (Lua reads at most three escape digits).
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                out.push_str(&format!("\\{:03}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Format a float as a Lua number literal.
fn lua_float(f: f64) -> String {
    if f.is_nan() {
        "(0/0)".into()
    } else if f.is_infinite() {
        if f > 0.0 {
            "math.huge".into()
        } else {
            "-math.huge".into()
        }
    } else {
        let mut s = format!("{f:?}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            s.push_str(".0");
        }
        s
    }
}

// ── Environment (env-stack alpha-renaming) ───────────────────────

/// A stack of scopes mapping original Parlance names to the Lua names
/// they are emitted under.  The bottom scope holds top-level
/// definitions; each `Lam` pushes one scope for its parameter.
struct Env {
    scopes: Vec<HashMap<String, String>>,
    /// Every Lua name already handed out (guarantees uniqueness).
    used: HashSet<String>,
    /// Counter for mangled names.
    counter: usize,
}

impl Env {
    fn new() -> Self {
        Env {
            scopes: vec![HashMap::new()],
            used: HashSet::new(),
            counter: 0,
        }
    }

    /// Register a top-level definition name; returns its Lua name.
    fn define(&mut self, name: &str) -> String {
        let lua = self.fresh_name(name);
        self.scopes[0].insert(name.to_string(), lua.clone());
        lua
    }

    /// Push a scope binding `name` (a lambda parameter); returns its Lua name.
    fn push_bind(&mut self, name: &str) -> String {
        let lua = self.fresh_name(name);
        let mut scope = HashMap::new();
        scope.insert(name.to_string(), lua.clone());
        self.scopes.push(scope);
        lua
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Look up the Lua name for a Parlance name (innermost scope wins).
    fn lookup(&self, name: &str) -> Option<&str> {
        for scope in self.scopes.iter().rev() {
            if let Some(lua) = scope.get(name) {
                return Some(lua.as_str());
            }
        }
        None
    }

    /// Produce a fresh valid Lua name for `name`.
    ///
    /// Valid non-reserved names keep their spelling — Lua's own
    /// lexical scoping then mirrors λ-shadowing exactly, and reusing
    /// the same plain name in nested scopes shadows correctly.  They
    /// are still recorded so mangled names never collide with them.
    /// Everything else (`'`-containing names, Lua keywords, preamble
    /// globals) is mangled with a unique suffix.
    fn fresh_name(&mut self, name: &str) -> String {
        if is_valid_lua_name(name) && !is_lua_keyword(name) && !RESERVED_GLOBALS.contains(&name) {
            self.used.insert(name.to_string());
            return name.to_string();
        }
        let base: String = name
            .chars()
            .map(|c| if c == '\'' { 'p' } else { c })
            .collect();
        let base = if base.is_empty() || !base.chars().next().unwrap().is_ascii_alphabetic() {
            format!("v_{base}")
        } else if is_lua_keyword(&base) {
            format!("k_{base}")
        } else {
            base
        };
        loop {
            let candidate = format!("{base}_{}", self.counter);
            self.counter += 1;
            if !self.used.contains(&candidate) {
                self.used.insert(candidate.clone());
                return candidate;
            }
        }
    }
}

// ── `::` qualified access ────────────────────────────────────────

/// If `v` is exactly `X::index` (with non-empty X), return X.
/// This is the special table-index form handled at application sites.
fn index_namespace(v: &str) -> Option<&str> {
    let (left, right) = v.split_once("::")?;
    if left.is_empty() || right != "index" {
        return None;
    }
    Some(left)
}

/// Emit the field-access chain for the segments after the first `::`
/// of a qualified name: "b::c" → ".b.c".  Segments that are not valid
/// Lua names (keywords, `'`-containing) use the bracket form `["…"]`.
fn field_access(right: &str) -> Result<String, String> {
    let mut out = String::new();
    for seg in right.split("::") {
        if seg.is_empty() {
            return Err(format!("empty segment in '::{right}'"));
        }
        if is_valid_lua_name(seg) && !is_lua_keyword(seg) {
            out.push('.');
            out.push_str(seg);
        } else {
            out.push('[');
            out.push_str(&lua_string(seg));
            out.push(']');
        }
    }
    Ok(out)
}

// ── Code generator ───────────────────────────────────────────────

/// Compile a list of IR definitions into self-contained Lua source.
///
/// `entry_point` names the definition invoked by the final call
/// (`Some("main")` for a typical program, or any `--entry` name).
/// Pass `None` to emit definitions only.
///
/// Malformed `::` segments (empty sides, bare `X::index`) are
/// compile errors and panic with a descriptive message, matching the
/// panic-on-invalid-IR convention of `parlance_ir::lower`.
pub fn compile(defs: &[IrDef], entry_point: Option<&str>) -> String {
    let mut gen = LuaGen::new();
    gen.compile_program(defs, entry_point)
}

struct LuaGen {
    env: Env,
    /// Ordered Lua names of all top-level definitions (predeclared once).
    def_names: Vec<String>,
    /// Left segments of `::`-qualified definitions that nothing else
    /// defines — predeclared and initialized as empty tables so the
    /// qualified natives can populate their fields.
    table_init: Vec<String>,
    /// (full name, left-segment Lua name, field-access chain) for each
    /// `::`-qualified definition (e.g. `native table::foo : Int` →
    /// ("table::foo", "table", ".foo")).
    qualified: Vec<(String, String, String)>,
}

impl LuaGen {
    fn new() -> Self {
        LuaGen {
            env: Env::new(),
            def_names: Vec::new(),
            table_init: Vec::new(),
            qualified: Vec::new(),
        }
    }

    fn qualified_for(&self, name: &str) -> &(String, String, String) {
        self.qualified
            .iter()
            .find(|(n, _, _)| n == name)
            .unwrap_or_else(|| panic!("parlance_lua: internal: qualified def '{name}' missing"))
    }

    fn compile_program(&mut self, defs: &[IrDef], entry_point: Option<&str>) -> String {
        // ── Pass A: predeclare every definition name (forward refs) ──
        // Plain names become locals (`local a, b, …`).  `::`-qualified
        // definitions live at a table field of their left segment; the
        // left segment itself becomes a local table when nothing else
        // defines it.
        let mut seen: HashSet<String> = HashSet::new();
        for def in defs {
            let name = match def {
                IrDef::Bind { name, .. } | IrDef::Native { name, .. } => name,
                IrDef::Infix { .. } => continue,
            };
            if let Some((left, right)) = name.split_once("::") {
                if left.is_empty() || right.is_empty() {
                    panic!("parlance_lua: compile error: malformed definition name '{name}'");
                }
                let left_was_known = seen.contains(left);
                let left_lua = if left_was_known {
                    self.env.lookup(left).expect("left registered").to_string()
                } else {
                    seen.insert(left.to_string());
                    let lua = self.env.define(left);
                    self.def_names.push(lua.clone());
                    lua
                };
                seen.insert(name.clone());
                if !left_was_known {
                    self.table_init.push(left_lua.clone());
                }
                let access = field_access(right).unwrap_or_else(|e| {
                    panic!("parlance_lua: compile error: malformed definition name '{name}': {e}")
                });
                self.qualified.push((name.clone(), left_lua, access));
                continue;
            }
            if seen.insert(name.clone()) {
                let lua_name = self.env.define(name);
                self.def_names.push(lua_name);
            }
        }

        let mut out = String::new();
        out.push_str("-- Generated by parlance_lua (Parlance IR -> Lua).\n");
        out.push_str(&preamble());

        if !self.def_names.is_empty() {
            out.push_str("-- forward declarations\nlocal ");
            out.push_str(&self.def_names.join(", "));
            out.push('\n');
        }
        for t in &self.table_init {
            out.push_str(&format!(
                "-- table for ::-qualified definitions\n{t} = {{}}\n"
            ));
        }

        // ── Pass B: definitions ─────────────────────────────────
        for def in defs {
            match def {
                IrDef::Bind { name, expr } if name.contains("::") => {
                    let (_, left_lua, access) = self.qualified_for(name).clone();
                    let body = self.compile_expr(expr);
                    out.push_str(&format!(
                        "{left_lua}{access} = function() return {body} end\n"
                    ));
                }
                IrDef::Bind { name, expr } => {
                    let lua_name = self.env.lookup(name).expect("def predeclared").to_string();
                    // Top-level binds are thunks: `name = function() return <expr> end`.
                    // This is what lets the entry call `main()` perform the program's
                    // effect — mirroring GraftVM, where every definition compiles to a
                    // callable function and the entry point is a call to it.
                    let body = self.compile_expr(expr);
                    out.push_str(&format!("{lua_name} = function() return {body} end\n"));
                }
                IrDef::Native { name, arity } if name.contains("::") => {
                    let (_, left_lua, access) = self.qualified_for(name).clone();
                    if !PRELUDE_NATIVES.contains(&name.as_str()) {
                        out.push_str(&native_stub(name));
                    }
                    out.push_str(&format!(
                        "{left_lua}{access} = {}\n",
                        native_body(name, *arity)
                    ));
                }
                IrDef::Native { name, arity } => {
                    let lua_name = self.env.lookup(name).expect("def predeclared").to_string();
                    if !PRELUDE_NATIVES.contains(&name.as_str()) && name != TABLE_FACTORY {
                        // Unknown native → runtime-error stub.  The
                        // wrapper below still delegates to the Native
                        // table, so a host can extend the preamble
                        // (e.g. with a `table` factory) later.
                        out.push_str(&native_stub(name));
                    }
                    out.push_str(&native_wrapper(&lua_name, name, *arity));
                }
                IrDef::Infix { .. } => {
                    // Parser-level sugar — nothing to emit.
                }
            }
        }

        // ── Entry call ──────────────────────────────────────────
        if let Some(entry) = entry_point {
            if let Some(lua) = self.env.lookup(entry) {
                out.push_str(&format!("-- entry point\n{lua}()\n"));
            }
            // Unknown entry name: silently omit (GraftVM codegen does
            // the same).
        }

        out
    }

    // ── Expression compilation ─────────────────────────────────

    fn compile_expr(&mut self, ir: &Ir) -> String {
        match ir {
            Ir::Int(n) => n.to_string(),
            Ir::Float(f) => lua_float(*f),
            Ir::Str(s) => lua_string(s),
            Ir::Var(v) => self.compile_var(v),
            Ir::Lam { param, body } => {
                let lua_param = self.env.push_bind(param);
                let body_lua = self.compile_expr(body);
                self.env.pop_scope();
                format!("function({lua_param}) return {body_lua} end")
            }
            Ir::App(f, a) => self.compile_app(f, a),
        }
    }

    fn compile_var(&mut self, v: &str) -> String {
        if let Some((left, right)) = v.split_once("::") {
            // `::` table access.
            if left.is_empty() {
                panic!("parlance_lua: compile error: malformed variable '{v}': empty left segment");
            }
            if right.is_empty() {
                panic!(
                    "parlance_lua: compile error: malformed variable '{v}': empty right segment"
                );
            }
            if right == "index" {
                panic!(
                    "parlance_lua: compile error: bare '{v}' cannot be emitted as a raw Lua \
                     name — use it applied: X::index K or X::index T K"
                );
            }
            let access = field_access(right).unwrap_or_else(|e| {
                panic!("parlance_lua: compile error: malformed variable '{v}': {e}")
            });
            let left_lua = self.env_name(left);
            return format!("{left_lua}{access}");
        }
        self.env_name(v)
    }

    fn compile_app(&mut self, f: &Ir, a: &Ir) -> String {
        // Two-argument index form: App(App(Var("X::index"), T), K) → T[K]
        if let Ir::App(inner, t) = f {
            if let Ir::Var(v) = inner.as_ref() {
                if let Some(_namespace) = index_namespace(v) {
                    let t_lua = self.compile_expr(t);
                    let k_lua = self.compile_expr(a);
                    let t_lua = self.bracket_expr(t_lua, t);
                    let k_lua = self.bracket_expr(k_lua, a);
                    return format!("{t_lua}[{k_lua}]");
                }
            }
        }
        // One-argument index form: App(Var("X::index"), K) → X[K]
        if let Ir::Var(v) = f {
            if let Some(namespace) = index_namespace(v) {
                let x = self.env_name(namespace);
                let k_lua = self.compile_expr(a);
                let k_lua = self.bracket_expr(k_lua, a);
                return format!("{x}[{k_lua}]");
            }
        }
        // Generic application: <f>(<a>)
        let f_lua = self.compile_expr(f);
        let a_lua = self.compile_expr(a);
        let f_lua = self.call_expr(f_lua, f);
        format!("{f_lua}({a_lua})")
    }

    /// Lua name for a Parlance name: env lookup, else (tolerantly)
    /// the name itself as a Lua global when it is a valid identifier.
    fn env_name(&self, name: &str) -> String {
        if let Some(lua) = self.env.lookup(name) {
            return lua.to_string();
        }
        if is_valid_lua_name(name) && !is_lua_keyword(name) && !RESERVED_GLOBALS.contains(&name) {
            name.to_string()
        } else {
            panic!(
                "parlance_lua: compile error: unbound variable '{name}' cannot be emitted as a \
                 Lua name"
            );
        }
    }

    /// Function position in a call: Lua cannot call a lambda or
    /// literal directly, so parenthesize those forms.
    fn call_expr(&self, e: String, kind: &Ir) -> String {
        match kind {
            Ir::Lam { .. } | Ir::Int(_) | Ir::Float(_) | Ir::Str(_) => format!("({e})"),
            _ => e,
        }
    }

    /// Index target or key in a `[...]` access: only a lambda needs
    /// parentheses there.
    fn bracket_expr(&self, e: String, kind: &Ir) -> String {
        if matches!(kind, Ir::Lam { .. }) {
            format!("({e})")
        } else {
            e
        }
    }
}

// ── Native runtime preamble ──────────────────────────────────────

/// Emit the self-contained runtime: the `Native` table with the
/// prelude natives implemented (mirroring GraftVM main.rs).
fn preamble() -> String {
    let mut s = String::new();
    s.push_str("-- Native runtime: prelude natives (mirrors GraftVM main.rs)\n");
    s.push_str("local Native = {}\n");
    s.push_str("Native.add = function(a, b) return a + b end\n");
    s.push_str("Native.sub = function(a, b) return a - b end\n");
    s.push_str("Native.mul = function(a, b) return a * b end\n");
    s.push_str("Native.div = function(a, b) return math.floor(a / b) end\n");
    s.push_str("Native.print = function(a) io.write(tostring(a), \"\\n\") return a end\n");
    // The table factory: a zero-arity native producing a fresh Lua
    // table whose fields/indices the `::` forms read (spec/table.plc).
    s.push_str("Native.table = function() return { foo = 42, cat = 7 } end\n");
    s
}

/// A curried wrapper delegating to the runtime `Native` table:
///
///   native add : Int -> Int -> Int
///   →  add = function(a1) return function(a2) return Native.add(a1, a2) end end
/// The `Native` table reference for a native name: dot form when the
/// name is a valid Lua identifier, bracket form otherwise.
fn native_call_target(name: &str) -> String {
    if is_valid_lua_name(name) && !is_lua_keyword(name) {
        format!("Native.{name}")
    } else {
        format!("Native[{}]", lua_string(name))
    }
}

/// The value expression delegating to the runtime `Native` table:
///
///   native add : Int -> Int -> Int
///   →  function(a1) return function(a2) return Native.add(a1, a2) end end
///
/// A zero-arity native is a pure *value* (e.g. `native random : Int`),
/// so it evaluates once at load: `Native.random()`.  This is what lets
/// a native factory (e.g. the `table` example) bind a Lua table
/// directly to its name.
fn native_body(name: &str, arity: u32) -> String {
    let target = native_call_target(name);
    let params: Vec<String> = (1..=arity).map(|i| format!("a{i}")).collect();
    let call = format!("{}({})", target, params.join(", "));
    if arity == 0 {
        call
    } else {
        let mut body = call;
        for p in params.iter().rev() {
            body = format!("function({p}) return {body} end");
        }
        body
    }
}

/// A runtime-error stub for an unknown native: the `Native` table entry
/// errors when called, so a host can later extend the preamble.
fn native_stub(name: &str) -> String {
    format!(
        "Native[{}] = function(...) error({}) end\n",
        lua_string(name),
        lua_string(&format!("unknown native '{name}'")),
    )
}

/// Bind a plain (unqualified) native name to its wrapper/value:
/// `add = <native_body>`.
fn native_wrapper(lua_name: &str, original: &str, arity: u32) -> String {
    format!("{lua_name} = {}\n", native_body(original, arity))
}

// ── Self-test ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn bind(name: &str, expr: Ir) -> IrDef {
        IrDef::Bind {
            name: name.into(),
            expr,
        }
    }

    fn native(name: &str, arity: u32) -> IrDef {
        IrDef::Native {
            name: name.into(),
            arity,
        }
    }

    fn lam(param: &str, body: Ir) -> Ir {
        Ir::Lam {
            param: param.into(),
            body: Box::new(body),
        }
    }

    fn app(f: Ir, a: Ir) -> Ir {
        Ir::App(Box::new(f), Box::new(a))
    }

    /// Compile a single-expression definition and return the RHS text
    /// (the expression inside the top-level thunk).
    fn expr_lua(expr: Ir) -> String {
        let src = compile(&[bind("main", expr)], Some("main"));
        let line = src
            .lines()
            .find(|l| l.starts_with("main = function() return "))
            .expect("main definition");
        let inner = line.trim_start_matches("main = function() return ");
        inner.strip_suffix(" end").expect("thunk close").to_string()
    }

    // ── Per-IR-node homomorphism ────────────────────────────────

    #[test]
    fn int_literal() {
        assert_eq!(expr_lua(Ir::Int(42)), "42");
        assert_eq!(expr_lua(Ir::Int(-7)), "-7");
    }

    #[test]
    fn float_literal() {
        assert_eq!(expr_lua(Ir::Float(3.14)), "3.14");
        assert_eq!(expr_lua(Ir::Float(1.0)), "1.0");
        assert_eq!(expr_lua(Ir::Float(f64::NAN)), "(0/0)");
        assert_eq!(expr_lua(Ir::Float(f64::INFINITY)), "math.huge");
        assert_eq!(expr_lua(Ir::Float(f64::NEG_INFINITY)), "-math.huge");
    }

    #[test]
    fn str_literal() {
        assert_eq!(expr_lua(Ir::Str("hi".into())), "\"hi\"");
        // Escaping: quotes, backslashes, newline, tab, control chars.
        let src = expr_lua(Ir::Str("a\"b\\c\nd\te\u{1}".into()));
        assert_eq!(src, "\"a\\\"b\\\\c\\nd\\te\\001\"");
    }

    #[test]
    fn var_reference() {
        let src = compile(
            &[bind("x", Ir::Int(1)), bind("main", Ir::Var("x".into()))],
            Some("main"),
        );
        assert!(
            src.contains("main = function() return x end"),
            "got:\n{src}"
        );
    }

    #[test]
    fn lambda_encoding() {
        let src = expr_lua(lam("x", Ir::Var("x".into())));
        assert_eq!(src, "function(x) return x end");
    }

    #[test]
    fn application_encoding() {
        let src = expr_lua(app(Ir::Var("f".into()), Ir::Int(1)));
        assert_eq!(src, "f(1)");
    }

    #[test]
    fn application_of_lambda_is_parenthesized() {
        // Lua requires (function() … end)(arg), not function() … end(arg).
        let src = expr_lua(app(lam("x", Ir::Var("x".into())), Ir::Int(1)));
        assert!(
            src.starts_with("(function(x) return x end)(1)"),
            "got: {src}"
        );
    }

    // ── `::` table access ───────────────────────────────────────

    #[test]
    fn table_field_access() {
        let src = compile(
            &[
                bind("t", Ir::Int(1)),
                bind("main", Ir::Var("t::foo".into())),
            ],
            Some("main"),
        );
        // t::foo → t.foo — the left segment renames exactly like plain `t`.
        assert!(src.contains("return t.foo"), "got:\n{src}");
        assert!(
            !src.contains("t()"),
            "left segment must not call the thunk:\n{src}"
        );
    }

    #[test]
    fn nested_table_access() {
        let src = expr_lua(Ir::Var("a::b::c".into()));
        assert_eq!(src, "a.b.c");
    }

    #[test]
    fn table_index_one_arg() {
        // X::index K → X[K]
        let src = expr_lua(app(Ir::Var("table::index".into()), Ir::Str("cat".into())));
        assert_eq!(src, "table[\"cat\"]");
    }

    #[test]
    fn table_index_two_args() {
        // X::index T K → T[K]  (user example: table::index tbl "cat")
        let src = expr_lua(app(
            app(Ir::Var("table::index".into()), Ir::Var("tbl".into())),
            Ir::Str("cat".into()),
        ));
        assert_eq!(src, "tbl[\"cat\"]");
    }

    #[test]
    fn table_index_key_expression() {
        let src = expr_lua(app(
            app(Ir::Var("table::index".into()), Ir::Var("t".into())),
            Ir::Var("k".into()),
        ));
        assert_eq!(src, "t[k]");
    }

    #[test]
    fn table_qualified_call() {
        // t::foo arg → t.foo(arg)
        let src = expr_lua(app(Ir::Var("t::foo".into()), Ir::Int(1)));
        assert_eq!(src, "t.foo(1)");
    }

    #[test]
    #[should_panic(expected = "bare")]
    fn bare_index_is_a_compile_error() {
        expr_lua(Ir::Var("table::index".into()));
    }

    #[test]
    #[should_panic(expected = "empty left segment")]
    fn empty_left_segment_is_a_compile_error() {
        expr_lua(Ir::Var("::foo".into()));
    }

    #[test]
    #[should_panic(expected = "empty right segment")]
    fn empty_right_segment_is_a_compile_error() {
        expr_lua(Ir::Var("t::".into()));
    }

    #[test]
    #[should_panic(expected = "empty segment")]
    fn empty_middle_segment_is_a_compile_error() {
        expr_lua(Ir::Var("t::::foo".into()));
    }

    // ── Env-stack alpha-renaming ────────────────────────────────

    #[test]
    fn prime_param_is_renamed_to_valid_lua() {
        // `x'` is a legal Parlance identifier but not a Lua one.
        let src = expr_lua(lam("x'", Ir::Var("x'".into())));
        assert!(src.starts_with("function(xp_0)"), "got: {src}");
        assert!(src.contains("return xp_0"), "got: {src}");
    }

    #[test]
    fn lua_keyword_def_is_renamed() {
        // `local` is not a Parlance keyword but IS a Lua keyword.
        let src = compile(
            &[
                bind("local", Ir::Int(5)),
                bind("main", Ir::Var("local".into())),
            ],
            Some("main"),
        );
        // No bare `local` may appear as an identifier.
        for line in src.lines() {
            assert!(
                !line.starts_with("local =") && !line.starts_with("local("),
                "bare keyword leaked:\n{src}"
            );
        }
        assert!(
            src.contains("main = function() return k_local_0 end"),
            "got:\n{src}"
        );
    }

    #[test]
    fn keyword_param_shadows_safely() {
        let src = expr_lua(lam("end", Ir::Var("end".into())));
        assert!(src.starts_with("function(k_end_"), "got: {src}");
    }

    #[test]
    fn shadowing_plain_names_keeps_lua_scoping() {
        // \x -> (\x -> x): both params map to `x`, Lua scoping is
        // exactly λ-shadowing, so the output stays valid and correct.
        let src = expr_lua(lam("x", lam("x", Ir::Var("x".into()))));
        assert_eq!(src, "function(x) return function(x) return x end end");
    }

    #[test]
    fn preamble_global_names_are_renamed() {
        // A user definition named `io` must not shadow the `io` used
        // by the Native.print implementation.
        let src = compile(&[bind("io", Ir::Int(1))], None);
        assert!(
            src.contains("io_0 = function() return 1 end"),
            "got:\n{src}"
        );
        assert!(!src.contains("io = function"), "got:\n{src}");
        assert!(src.contains("Native.print"), "got:\n{src}");
    }

    // ── Top-level definitions ───────────────────────────────────

    #[test]
    fn all_def_names_are_predeclared_for_forward_refs() {
        let src = compile(
            &[
                bind(
                    "main",
                    Ir::App(Box::new(Ir::Var("helper".into())), Box::new(Ir::Int(1))),
                ),
                bind("helper", lam("x", Ir::Var("x".into()))),
            ],
            Some("main"),
        );
        // forward reference: main refers to helper declared later
        assert!(src.contains("local main, helper"), "got:\n{src}");
        assert!(
            src.contains("main = function() return helper(1) end"),
            "got:\n{src}"
        );
        assert!(
            src.contains("helper = function() return function(x) return x end end"),
            "got:\n{src}"
        );
    }

    #[test]
    fn infix_defs_are_skipped() {
        let src = compile(
            &[
                IrDef::Infix {
                    op: "+".into(),
                    strength: 5,
                    func: Ir::Var("add".into()),
                },
                bind("main", Ir::Int(1)),
            ],
            Some("main"),
        );
        assert!(
            src.contains("main = function() return 1 end"),
            "got:\n{src}"
        );
        assert!(!src.contains("infix"), "got:\n{src}");
    }

    #[test]
    fn qualified_native_populates_table_field() {
        // `native table::foo : Int` → `table.foo = Native["table::foo"]()`
        let src = compile(&[native("table::foo", 0)], None);
        assert!(src.contains("local table"), "got:\n{src}");
        assert!(src.contains("table = {}"), "got:\n{src}");
        assert!(
            src.contains("table.foo = Native[\"table::foo\"]()"),
            "got:\n{src}"
        );
        // No bare `table.foo` raw-name emission: the field is populated
        // from the runtime Native table (extendable by a host).
        assert!(
            src.contains(
                "Native[\"table::foo\"] = function(...) error(\"unknown native 'table::foo'\") end"
            ),
            "got:\n{src}"
        );
    }

    #[test]
    fn qualified_native_function_field() {
        // `native table::index : Str -> Int` → `table.index = function(a1) … end`
        let src = compile(&[native("table::index", 1)], None);
        assert!(
            src.contains("table.index = function(a1) return Native[\"table::index\"](a1) end"),
            "got:\n{src}"
        );
    }

    #[test]
    fn qualified_names_share_left_segment_table() {
        let src = compile(&[native("table::foo", 0), native("table::index", 1)], None);
        // one `local table`, one `table = {}`, two field assignments
        assert_eq!(src.matches("local table").count(), 1, "got:\n{src}");
        assert_eq!(src.matches("table = {}").count(), 1, "got:\n{src}");
        assert!(src.contains("table.foo ="), "got:\n{src}");
        assert!(src.contains("table.index ="), "got:\n{src}");
    }

    #[test]
    #[should_panic(expected = "malformed definition name")]
    fn empty_qualified_def_segment_is_a_compile_error() {
        compile(&[native("table::", 0)], None);
    }

    #[test]
    fn qualified_bind_populates_table_field_thunk() {
        let src = compile(&[bind("tbl::foo", Ir::Int(7))], None);
        assert!(src.contains("tbl = {}"), "got:\n{src}");
        assert!(
            src.contains("tbl.foo = function() return 7 end"),
            "got:\n{src}"
        );
    }

    // ── Native wrappers ─────────────────────────────────────────

    #[test]
    fn native_wrapper_is_curried() {
        let src = compile(&[native("add", 2)], None);
        assert!(
            src.contains(
                "add = function(a1) return function(a2) return Native.add(a1, a2) end end"
            ),
            "got:\n{src}"
        );
    }

    #[test]
    fn native_unary_wrapper() {
        let src = compile(&[native("print", 1)], None);
        assert!(
            src.contains("print = function(a1) return Native.print(a1) end"),
            "got:\n{src}"
        );
    }

    #[test]
    fn native_zero_arity_wrapper() {
        // A zero-arity native is a pure value — evaluated once at load.
        let src = compile(&[native("random", 0)], None);
        assert!(src.contains("random = Native.random()"), "got:\n{src}");
    }

    #[test]
    fn unknown_native_gets_runtime_error_stub() {
        let src = compile(&[native("foo", 2)], None);
        assert!(
            src.contains("Native[\"foo\"] = function(...) error(\"unknown native 'foo'\") end"),
            "got:\n{src}"
        );
        // Still curried so partial application compiles.
        assert!(
            src.contains(
                "foo = function(a1) return function(a2) return Native.foo(a1, a2) end end"
            ),
            "got:\n{src}"
        );
    }

    #[test]
    fn table_factory_native_has_no_stub() {
        // `native table : Table` (zero-arity) is implemented by the
        // preamble factory — no runtime-error stub is emitted.
        let src = compile(&[native("table", 0)], None);
        assert!(src.contains("table = Native.table()"), "got:\n{src}");
        assert!(
            !src.contains("unknown native 'table'"),
            "stub must be suppressed:\n{src}"
        );
    }

    #[test]
    fn preamble_implements_table_factory() {
        let src = compile(&[], None);
        assert!(
            src.contains("Native.table = function() return { foo = 42, cat = 7 } end"),
            "got:\n{src}"
        );
    }

    #[test]
    fn preamble_implements_prelude_natives() {
        let src = compile(&[], None);
        assert!(src.contains("Native.add = function(a, b) return a + b end"));
        assert!(src.contains("Native.sub = function(a, b) return a - b end"));
        assert!(src.contains("Native.mul = function(a, b) return a * b end"));
        assert!(src.contains("Native.div = function(a, b) return math.floor(a / b) end"));
        assert!(
            src.contains("Native.print = function(a) io.write(tostring(a), \"\\n\") return a end")
        );
    }

    // ── Entry emission ──────────────────────────────────────────

    #[test]
    fn entry_call_is_emitted_last() {
        let src = compile(&[bind("main", Ir::Int(1))], Some("main"));
        assert!(src.trim_end().ends_with("main()"), "got:\n{src}");
        // The call must come after the definition.
        let def_pos = src.find("main = function() return 1 end").unwrap();
        let call_pos = src.rfind("main()").unwrap();
        assert!(call_pos > def_pos);
    }

    #[test]
    fn custom_entry_name() {
        let src = compile(&[bind("start", Ir::Int(1))], Some("start"));
        assert!(src.trim_end().ends_with("start()"), "got:\n{src}");
    }

    #[test]
    fn no_entry_emits_definitions_only() {
        let src = compile(&[bind("main", Ir::Int(1))], None);
        assert!(src.contains("main = function() return 1 end"));
        assert!(!src.contains("main()"), "got:\n{src}");
    }

    #[test]
    fn unknown_entry_is_silently_omitted() {
        let src = compile(&[bind("main", Ir::Int(1))], Some("nope"));
        assert!(!src.contains("nope()"), "got:\n{src}");
    }

    #[test]
    fn renamed_entry_is_called_under_its_lua_name() {
        let src = compile(&[bind("main", Ir::Int(1))], Some("main"));
        assert!(src.contains("main()"));
    }

    // ── End-to-end shape checks ─────────────────────────────────

    #[test]
    fn hello_program_shape() {
        // The optimized hello.plc program: main = print "Hello World"
        let defs = vec![
            native("add", 2),
            native("sub", 2),
            native("mul", 2),
            native("div", 2),
            native("print", 1),
            bind(
                "main",
                app(Ir::Var("print".into()), Ir::Str("Hello World".into())),
            ),
        ];
        let src = compile(&defs, Some("main"));
        assert!(
            src.contains("local add, sub, mul, div, print, main"),
            "got:\n{src}"
        );
        assert!(
            src.contains("main = function() return print(\"Hello World\") end"),
            "got:\n{src}"
        );
        assert!(src.trim_end().ends_with("main()"), "got:\n{src}");
    }
}
