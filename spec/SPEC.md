# Parlance: 언어 명세 (Language Specification)

## 1. 이론적 기반 (Theoretical Foundation)

### 1.1 스캐너 — DFA (Deterministic Finite Automaton)

```
문자 클래스 분할:
  digit       [0-9]
  ident_start [a-zA-Z_]
  ident_cont  [a-zA-Z0-9_']
  symbol      ! # $ % & * + - . / < = > ? @ ^ | ~
  quote       "

DFA 상태 전이도:

           ┌──────── digit ────────┐
           │                       ▼
 START ──digit──→  INT  ◄── digit ────┘
                   │
                   │ '.' + digit → FLOAT ◄── digit ──┐
                   │                                  │
      ┌──── '.' + digit ────┐                         │
      │                     ▼                         │
 START ──'.' + digit ──→  FLOAT ──────── digit ───────┘

           ┌─── ident_cont ───────┐
           │                      ▼
 START ──ident──→  IDENT ◄── ident_cont ─┘
                   │
                   │ "::" 쌍 → IDENT (table::foo 는 하나의 Ident)
                   │
                   │ 그 외 → ACCEPT

           ┌─── (any non-quote, non-backslash) ──┐
           │                                      │
           ▼                                      │
 START ──quote──→  STRING ──→  STRING_ESC ──→ ────┘
                   │              │
                   │ quote        │ any
                   ▼              ▼
                 ACCEPT          ACCEPT (after ESC)

           ┌─── symbol ────────┐
           │                   ▼
 START ──symbol──→  SYMBOL ◄── symbol ──┘

 START ──\──→  ACCEPT (emit Backslash)
 START ──(──→  ACCEPT (emit LParen)
 START ──)──→  ACCEPT (emit RParen)
 START ──,──→  ACCEPT (emit Comma)
 START ──other──→  ERROR

키워드 해석 (IDENT 수락 후): "import"|"define"|"infix"|"var"|"hiding"
심볼 해석 (SYMBOL 수락 후): "->"|"="|"<-"|">>=" → punctuation, 나머지 → Op(String)
최장 일치 (maximal munch): 모든 상태에서 가능한 가장 긴 입력을 소비

':' (콜론) 규칙:
  단일 ':' 는 IDENT 알파벳이 아님 — 항상 Token::Colon (타입 어노테이션용)
  이중 '::' 만 식별자를 계속 이어붙임 (field/index 접근):
    table::foo, tbl::index → 하나의 Ident 토큰
    define x : Int = ...    → Ident("x"), Colon, Ident("Int") — 그대로 동작
```

### 1.2 파서 — 재귀 하강 (Recursive Descent) + Pratt

```
LL(1) 재귀 하강: 각 논터미널이 하나의 함수, 1-토큰 lookahead로 FIRST 집합 디스패치
Pratt 파싱: 동적 바인딩 파워로 중위 연산자 우선순위 해결 (left-assoc)

BNF 계층 (loosest → tightest):
  expr   ::= pratt (">>=" pratt)*                  [Seq: bind-chain]
  pratt  ::= bind (OP bind)*                       [Infix: Pratt loop]
  bind   ::= "var" IDENT "<-" expr | apply         [Bind: local variable]
  apply  ::= atom+                                  [Apply: juxtaposition]
  atom   ::= INT | FLOAT | STR | IDENT
           | "(" expr ")"
           | "\" IDENT "->" expr                    [Lambda]
```

### 1.3 의미 분석 — 표기적 의미론 (Denotational Semantics)

```
각 고수준 구문의 "의미(denotation)"를 순수 λ-계산법으로 번역:

  Infix(l, op, r)  ⇒  Apply(Apply(func_of(op), l), r)
  Bind{name, val} 단독  ⇒  ERROR (continuation 없음)
  Seq([Bind(x1,v1), ..., Bind(xn,vn), body])
    ⇒  Apply(Lambda{x1, ... Apply(Lambda{xn, body}, vn)...}, v1)

이름 분석 (Lexical Scoping):
  모든 Var(name)은 lambda param, global define, 또는 import에서 온 이름이어야 함
```

### 1.4 최적화 — λ-계산법 변환

```
η-변환 (η-reduction):   λx. (f x)  →  f   (when x ∉ FV(f))
β-축약 (β-reduction):  (λx. body) arg  →  body[x↦arg]
인라인화 (Inlining):    define x = (literal/var) → 모든 Var(x)를 body로 치환
DCE:                   참조되지 않는 정의 제거
```

---

## 2. 완전한 EBNF 문법

```ebnf
(* ── 최상위 ── *)
program       = { stmt } EOF ;

(* ── 문장 ── *)
stmt          = import_stmt | define_stmt | infix_stmt ;

import_stmt   = "import" STRING
                [ "(" IDENT { "," IDENT } ")"
                | "hiding" "(" IDENT { "," IDENT } ")"
                ] ;

define_stmt   = "define" IDENT "=" expr ;

infix_stmt    = "infix" OP INT "=" expr ;

(* ── 표현식 ── *)
expr          = pratt { ">>=" pratt } ;     (* bind-chain, left-assoc *)

pratt         = bind { OP bind } ;           (* infix, Pratt-parsed *)

bind          = "var" IDENT "<-" expr        (* local binding *)
              | apply ;

apply         = atom { atom } ;              (* juxtaposition, left-assoc *)

atom          = INT
              | FLOAT
              | STR
              | IDENT
              | "(" expr ")"
              | "\" IDENT "->" expr ;        (* lambda *)

(* ── 터미널 ── *)
INT           = digit { digit } ;
FLOAT         = digit { digit } "." digit { digit }
              | "." digit { digit } ;
STR           = "\"" { char | escape } "\"" ;
IDENT         = ident_start { ident_cont | "::" } ;
OP            = symbol { symbol } ;          (* user-declared operator *)
```

---

## 3. FIRST 집합

```
FIRST(import_stmt)  = { "import" }
FIRST(define_stmt)  = { "define" }
FIRST(infix_stmt)   = { "infix" }
FIRST(stmt)         = { "import", "define", "infix" }
FIRST(expr)         = FIRST(pratt)
FIRST(pratt)        = FIRST(bind)
FIRST(bind)         = { "var" } ∪ FIRST(apply)
FIRST(apply)        = FIRST(atom)
FIRST(atom)         = { INT, FLOAT, STR, IDENT, "(", "\" }
FOLLOW(expr)        = { EOF, ")" }
```

---

## 4. 문법 예시

### 4.1 hello.plc — 항등 함수와 application

```plc
define id = \x -> x
define result = id 42
```

파이프라인 결과:
```
--tokens: define / id / = / \ / x / -> / x / define / result / = / id / 42
--ast:    define id = \x -> x
           define result = (id 42)
--ir:     define id = (\x -> x)
           define result = (id 42)
--opt:    define result = 42         (* id가 인라인 + β-reduction *)
--bytecode: Jump, Enter, StoreData, LoadData, Copy, PushArg, Exit, Ret
```

### 4.2 prelude.plc — 기초 연산자

```plc
infix * 6 = mul
infix + 5 = add

define mul = \x -> \y -> x
define add = \x -> \y -> x
```

### 4.3 infix.plc — 중위 연산자 우선순위

```plc
infix + 5 = add
infix * 6 = mul              (* BP=6: tighter than + *)

define add = \x -> \y -> x
define mul = \x -> \y -> x

define expr = 1 + 2 * 3      (* 1 + (2 * 3) *)
```

파싱 결과: `Infix(Int(1), "+", Infix(Int(2), "*", Int(3)))`
Desugar 후: `Apply(Apply(add, 1), Apply(Apply(mul, 2), 3))`

### 4.4 bind.plc — bind-chain (>>=)

```plc
infix + 5 = add
define add = \x -> \y -> x

define demo = var x <- 1 >>= var y <- 2 >>= x + y
```

Desugar 과정:
```
파싱:   Seq(Seq(Bind("x", 1), Bind("y", 2)), Infix(x, +, y))
flat:   [Bind("x",1), Bind("y",2), Infix(x,+,y)]
desugar: Apply(Lambda{x, Apply(Lambda{y, add(x,y)}, 2)}, 1)
```

### 4.5 let_expr.plc — let 표현식 (bind-chain 활용)

```plc
define add = \x -> \y -> x

define result =
  var a <- 5 >>=
  var b <- 3 >>=
  a + b
```

### 4.6 실수 리터럴

```plc
define pi = 3.14               (* Float(3.14) *)
define half = .5               (* Float(0.5)  *)
define integer = 42            (* Int(42)     — 점 없음 *)
```

### 4.7 import 예시

```plc
import "prelude"                   (* 모두 가져오기 *)
import "prelude" (add, mul)        (* 선택적 가져오기 *)
import "prelude" hiding (internal) (* 제외하고 가져오기 *)
```

### 4.8 `::` 필드/인덱스 접근 (table::foo, X::index K)

`::` 쌍은 식별자를 하나로 이어붙입니다. 즉 `table::foo` 와 `tbl::index`
는 각각 **하나의** `Ident` 토큰으로 어휘 분석됩니다. 단일 `:` 는 그대로
`Token::Colon` (타입 어노테이션 `define x : Int = ...`) 입니다.

네이티브로 제공되는 이름은 `native` 선언으로 등록하고, 적용(application)
으로 사용합니다:

```plc
native table::foo : Int
native table::index : Str -> Int

define main = table::index "cat"    (* 인덱스 접근: X::index K *)
define x = table::foo               (* 필드 접근:   table::foo *)
```

- 필드 접근: `table::foo` — 테이블/레코드의 필드 이름 하나로 취급
- 인덱스 접근: `X::index K` — `index` 에 키 `K` 를 적용
- 인덱스 접근 (키 + 값): `X::index T K` — `index` 에 `T`, `K` 두 인자를 적용

파이프라인 하위 단계(semant, IR, GraftVM codegen)는 이름을 불투명한
문자열로 취급하므로 변경이 필요 없습니다. 타입체커는 `::` 이름이 환경에
없으면(선언되지 않은 field/index 접근) 오류 대신 제약 없는 신선한 타입
변수를 반환하여 `Expr::Apply` 규칙을 통해 통일(unify)되게 합니다.

---

## 5. 전체 파이프라인

```
소스 텍스트 (.plc)
    │
    ▼
[parlance_lexer]     DFA (Deterministic Finite Automaton)
    │  문자 → 토큰 스트림 [Token]
    ▼
[parlance_parser]    Recursive Descent + Pratt (Top-Down Operator Precedence)
    │  토큰 → AST [Expr, Stmt]
    ▼
[parlance_import]    Module Resolution (파일 찾기 → 파싱 → export 수집 → 필터)
    │  Import → synthetic Define/Infix
    ▼
[parlance_semant]    Denotational Semantics + Lexical Scope
    │  Infix/Bind/Seq → 순수 Apply/Lambda/Var
    ▼
[parlance_ir]        λ-calculus IR
    │  Ir = Int | Float | Str | Var | Lam | App
    ▼
[parlance_optimize]       η-reduction → β-reduction → Inlining → DCE (fixpoint)
    │  동일한 의미, 더 간단한 IR
    ▼
[parlance_codegen]   IrBuilder → GraftVM bytecode (Opcode)
    │  StoreData, LoadData, Enter, PushArg, Call, Exit, Ret ...
    ▼
GraftVM 바이트코드 (.bc)
```

---

## 6. 컴파일러 사용법

```bash
parlance file.plc                     # AST + bytecode 출력
parlance file.plc --tokens            # 토큰 스트림 표시
parlance file.plc --ast               # AST 표시
parlance file.plc --ir                # IR 표시
parlance file.plc --opt               # 최적화된 IR 표시
parlance file.plc --bytecode          # 바이트코드 표시
parlance file.plc -o out.bc           # 바이트코드 파일 저장
```

---

## 7. AST 노드와 대응되는 이론

| AST 노드 | 이론 | 생산 규칙 | Desugar 후 |
|----------|------|-----------|------------|
| `Int(i64)` | 리터럴 | `atom ::= INT` | `Int` |
| `Float(f64)` | 리터럴 | `atom ::= FLOAT` | `Float` |
| `Str(String)` | 리터럴 | `atom ::= STR` | `Str` |
| `Var(String)` | 참조 | `atom ::= IDENT` | `Var` |
| `Lambda{param, body}` | λ-추상 | `atom ::= "\" IDENT "->" expr` | `Lam` |
| `Apply(f, a)` | 함수 적용 | `apply ::= atom+` | `App` |
| `Infix(l, op, r)` | Pratt infix | `pratt ::= bind OP bind` | `App(App(func, l), r)` |
| `Bind{name, value}` | 지역 변수 | `bind ::= "var" IDENT "<-" expr` | ERROR (단독) |
| `Seq(a, b)` | bind-chain | `expr ::= pratt ">>=" pratt` | 중첩 `App(Lam{...}, ...)` |

---

## 8. 크레이트 의존성 그래프

```
parlc (binary)
  └─ parlance_codegen
       ├─ parlance_ir
       │   └─ parlance_core
       ├─ parlance_optimize
       │   └─ parlance_ir
       └─ graftvm_ir
            ├─ graftvm_bytecode
            └─ graftvm_liternal
  ├─ parlance_semant
  │   ├─ parlance_core
  │   └─ parlance_import
  │       ├─ parlance_core
  │       ├─ parlance_lexer
  │       │   └─ parlance_core
  │       └─ parlance_parser
  │           ├─ parlance_core
  │           └─ parlance_lexer
  └─ parlance_core
```
