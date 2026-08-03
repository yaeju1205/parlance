# Parlance

함수형 언어 컴파일러 — GraftVM을 위한 프론트엔드

Parlance는 순수 함수형 프로그래밍 언어로, λ-계산법을 기반으로 설계되었습니다. 소스 코드를 GraftVM 바이트코드로 컴파일하고 실행합니다.

## 빠른 시작

```bash
# 컴파일 + 실행
cargo run -p parlance -- examples/hello.plc --run

# 바이트코드 확인
cargo run -p parlance -- examples/hello.plc --bytecode

# 최적화된 IR 확인
cargo run -p parlance -- examples/hello.plc --opt

# 모든 테스트 실행
cargo test
```

## 예제

```plc
define id = \x -> x        # 항등 함수
define result = id 42      # 함수 적용

infix + 5 = add            # 중위 연산자 선언
infix * 6 = mul            # BP: binding power (클수록 강함)

define expr = 1 + 2 * 3    # 1 + (2 * 3) — 우선순위 적용

# bind-chain (do-notation)
define demo = var x <- 1 >>= var y <- 2 >>= x + y

# `::` 필드/인덱스 접근 (이중 콜론은 식별자를 하나로 이어붙임)
native table::foo : Int
native table::index : Str -> Int
define main = table::index "cat"    # 인덱스 접근 (X::index K)
define x = table::foo               # 필드 접근 (table::foo)
```

## 사용법

```bash
parlance file.plc                     # 바이트코드 출력
parlance file.plc --tokens            # 토큰 스트림
parlance file.plc --ast               # AST
parlance file.plc --ir                # IR
parlance file.plc --opt               # 최적화된 IR
parlance file.plc --bytecode          # 바이트코드
parlance file.plc --run               # 컴파일 + GraftVM 실행
parlance file.plc -o out.bc           # 바이트코드 저장
```

## 설치
```bash
cargo install --path .
```

## 파이프라인

```
소스 (.plc)
  → [lexer]     DFA 스캐너              →  토큰 스트림
  → [parser]    재귀하강 + Pratt        →  AST (Expr, Stmt)
  → [import]    모듈 해석               →  import → 정의 병합
  → [semant]    표기적 의미론 desugar   →  순수 λ-term (Infix/Bind/Seq 제거)
  → [ir]        λ-계산법 IR             →  Int | Float | Str | Var | Lam | App
  → [opt]       η-reduction, β-reduction, inlining, DCE
  → [codegen]   GraftVM bytecode        →  Opcode 리스트
  → [run]       GraftVM 인터프리터      →  실행
```

## 이론

| 단계 | 이론 | 설명 |
|------|------|------|
| 스캐너 | **DFA** (Deterministic Finite Automaton) | 문자 클래스 분할 → 상태 전이 → 최장 일치 |
| 파서 | **재귀 하강** + **Pratt** (Top-Down Operator Precedence) | LL(1) lookahead + 동적 바인딩 파워 |
| 의미 분석 | **표기적 의미론** (Denotational Semantics) | 고수준 구문 → 순수 λ-계산법 매크로 확장 |
| 이름 분석 | **Lexical Scoping** | 정적 스코프, λ-바인더 그림자 규칙 |
| 최적화 | **λ-계산법 변환** | η-reduction, β-reduction, inlining, dead code elimination |
| 코드 생성 | **동형 사상** (Homomorphism) | 각 IR 노드 → Opcode 시퀀스 |

## `::` 이중 콜론 접근

`::` 쌍은 식별자를 이어붙여 **하나의 이름**으로 토큰화합니다 — 필드/인덱스
접근을 위한 문법입니다. 단일 `:` 는 변함없이 타입 어노테이션 전용입니다.

```plc
native table::foo : Int            # 네이티브로 제공되는 이름 등록
native table::index : Str -> Int

define main = table::index "cat"   # 인덱스 접근 (X::index K)
define x = table::foo              # 필드 접근 (table::foo)
```

- 필드 접근: `table::foo`
- 인덱스 접근: `X::index K` (키 1개), `X::index T K` (키 + 값 2개)
- `::` 이름이 환경에 없으면 타입체커는 제약 없는 타입 변수를 반환하여
  `Expr::Apply` 규칙을 통해 통일됩니다 (unbound-variable 오류 대신).

## 프로젝트 구조

```
parlance/
├── Cargo.toml                  # 워크스페이스
├── README.md
├── SPEC.md                     # 완전한 언어 명세 (EBNF, 이론, 예제)
├── SYNTAX.md                   # 문법 레퍼런스
├── examples/
│   ├── hello.plc               # 항등 함수
│   ├── prelude.plc             # 기초 연산자
│   ├── infix.plc               # 중위 연산자 우선순위
│   ├── bind.plc                # bind-chain (>>=)
│   └── let_expr.plc            # let 스타일
└── crates/
    ├── parlance-core/          # Token, AST, ImportSpec, Module, Export
    ├── parlance-lexer/         # DFA 스캐너
    ├── parlance-parser/        # 재귀하강 + Pratt 파서
    ├── parlance-import/        # 모듈 해석기 (사이클 감지, Only/Hiding 필터)
    ├── parlance-semant/        # Denotational desugar + lexical scope
    ├── parlance-ir/            # λ-calculus IR + substitution + folding
    ├── parlance-opt/           # η-reduction, β-reduction, inlining, DCE
    ├── parlance-codegen/       # IR → GraftVM bytecode
    └── parlc/                  # 바이너리 (parlc 명령어)
```

## 테스트

```bash
cargo test           # 71개 테스트 전부 실행
cargo test -p parlance-parser    # 파서 테스트 (13)
cargo test -p parlance-import    # import 테스트 (5)
cargo test -p parlance-semant    # 의미 분석 테스트 (7)
cargo test -p parlance-ir        # IR 테스트 (15)
cargo test -p parlance-opt       # 최적화 테스트 (23)
cargo test -p parlance-codegen   # 코드 생성 테스트 (8)
```

## 라이선스

MIT
