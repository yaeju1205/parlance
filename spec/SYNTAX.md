# Parlance Syntax Reference

## 프로그램 구조

```plc
import "prelude"                    # 파일 전체 import
import "prelude" (add, mul)         # 선택적 import
import "prelude" hiding (internal)  # 제외하고 import

define id = \x => x                 # 정의 (최상위)
infix + 5 = add                     # 연산자 우선순위 선언
```

## 리터럴

| 표현 | 타입 | 예시 |
|------|------|------|
| 정수 | `Int` | `42`, `0`, `-5` |
| 실수 | `Float` | `3.14`, `.5` |
| 문자열 | `Str` | `"hello"`, `"esc\n\t"` |

## 표현식 계층 (loosest → tightest)

```
>>=  (Seq, bind-chain)
OP   (infix, Pratt-parsed)
var  (Bind, local variable binding)
f x  (Apply, juxtaposition, left-assoc)
atom (Int, Float, Str, Var, (expr), \param => expr)
```

## 람다

```plc
define id = \x => x                 # 항등 함수
define const = \x => \y => x        # 커링된 2-인자 함수
define apply = \f => \x => f x      # 고차 함수
```

## 애플리케이션 (함수 호출)

```plc
define call = f x                   # juxtaposition = 함수 호출
define nested = f g x               # ((f g) x), left-assoc
```

## 중위 연산자 (Pratt 파싱)

```plc
infix + 5 = add                     # + 는 BP=5 (looser)
infix * 6 = mul                     # * 는 BP=6 (tighter)

define expr = 1 + 2 * 3             # 1 + (2 * 3)
```

## Bind-chain (do-notation)

```plc
define demo = var x <- 1 >>= var y <- 2 >>= x + y
# desugars to:  Apply(Lambda{x, Apply(Lambda{y, x+y}, 2)}, 1)
```

## 그룹핑

```plc
define x = (1 + 2) * 3              # 괄호로 우선순위 변경
```

## `::` 필드/인덱스 접근

`::` (이중 콜론) 쌍은 식별자를 하나로 이어붙입니다. `table::foo`,
`tbl::index` 는 각각 **하나의 식별자**로 토큰화되며, 필드/인덱스 접근에
사용합니다.

```plc
native table::foo : Int
native table::index : Str -> Int

define main = table::index "cat"    # 인덱스 접근 (X::index K)
define x = table::foo               # 필드 접근 (table::foo)
define y = table::index "cat" 1     # 키+값 인덱스 접근 (X::index T K)
```

주의: 단일 `:` 는 여전히 타입 어노테이션 전용 토큰입니다.
`define x : Int = 42` 는 그대로 동작하며 `x:Int` (공백 없음)도
`x`, `:`, `Int` 세 토큰으로 분리됩니다.

## 컴파일러 사용법

```bash
parlc file.plc                      # AST + bytecode 출력
parlc file.plc --tokens             # 토큰 스트림
parlc file.plc --ast                # AST 표시
parlc file.plc --ir                 # IR 표시
parlc file.plc --opt                # 최적화된 IR 표시
parlc file.plc --bytecode           # 바이트코드 표시
parlc file.plc --run                # 컴파일 후 GraftVM으로 실행
parlc file.plc -o out.bc            # 바이트코드 파일 저장
```

## 실행 예시

```bash
# hello.plc: id 42 → 42 (상수 폴딩)
$ cargo run -p parlc -- examples/hello.plc --run
;; running bytecode (2 instructions)...
  [  0] StoreData { index: 0, data: Int64(42) }
  [  1] LoadData { dst: Addr { window: 0, slot: 0 }, index: 0 }
;; execution finished

# infix.plc: 1 + 2 * 3 → 1 (상수 폴딩)
$ cargo run -p parlc -- examples/infix.plc --run
;; running bytecode (2 instructions)...
  [  0] StoreData { index: 0, data: Int64(1) }
  [  1] LoadData { dst: Addr { window: 0, slot: 0 }, index: 0 }
;; execution finished
```
