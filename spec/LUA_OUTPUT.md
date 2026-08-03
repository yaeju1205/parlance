# Lua backend — observed outputs

Commands: `cargo run -- spec/<file>.plc --lua -o /tmp/out.lua` then `lua5.4 /tmp/out.lua`.
Lua interpreter: lua5.4 (5.4.4, Debian bookworm). Runner: `target/debug/parlance`.

| spec            | `--lua` → `lua5.4` output        | `--run` (GraftVM) output        | notes |
|-----------------|----------------------------------|---------------------------------|-------|
| spec/hello.plc  | `Hello World`                    | `Hello World`                   | matches GraftVM |
| spec/infix.plc  | *(no output)*                    | *(no output)*                   | no `main`; `expr = 1 + 2 * 3` with the spec's own `add`/`mul` (which return their first argument) constant-folds to `1` — emitted `expr = function() return 1 end` |
| spec/bind.plc   | *(no output)*                    | *(no output)*                   | no `main`; `demo = var x <- 1 >>= var y <- 2 >>= x + y` → `add 1 2` → `1` (spec's `add` returns first arg) — emitted `demo = function() return 1 end` |
| spec/table.plc  | `49`                             | *(runtime error: GraftVM codegen has no `::` field/index semantics — `::` names are treated as plain function names; the Lua backend is where `::` access is real)* | `table::foo` → `table.foo` = 42, `table::index "cat"` → `table["cat"]` = 7, sum printed = 49 |

Precedence/arithmetic sanity check with the prelude natives (not a repo spec):

```plc
define main =
  var x <- 1 + 2 * 3  >>=
  print x
```
→ `lua5.4` prints `7` (infix precedence + curried prelude `add`/`mul` through the Lua backend).

## table.plc semantics

`native table : Table` is a zero-arity native — a pure *value*, evaluated once at
load (`table = Native.table()`). The parlance_lua preamble implements it as a
factory returning a fresh Lua table `{ foo = 42, cat = 7 }`. The `::` forms read
it:

- `table::foo`         →  `table.foo`        (field access)
- `table::index "cat"` →  `table["cat"]`     (index access)

so the program computes and prints `add 42 7 = 49`.
