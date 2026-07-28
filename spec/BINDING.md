# Parlance 바인딩 시스템 (NFI)

Parlance는 **바인딩용 언어**로 설계되었습니다. 핵심 아이디어는 간단합니다:

> **컴파일러는 함수 이름을 전혀 모릅니다.**  
> "이 함수는 네이티브 함수다"라는 선언만 알고, 실제 구현은 호스트가 런타임에 주입합니다.

이 문서는 Parlance의 네이티브 함수 시스템을 사용하는 방법을 설명합니다.

---

## 1. 개념

Parlance 프로그램은 세 종류의 정의로 구성됩니다:

| 종류 | 문법 | 구현 위치 |
|------|------|-----------|
| `define` | `define name = expr` | Parlance 자체 (λ-계산법) |
| `native` | `native name : type` | 호스트 (Rust, 다른 언어) |
| `infix` | `infix op bp = func` | 연산자 우선순위 선언 |

`native` 함수는 **Parlance 외부에서 구현**됩니다. 컴파일러는 `CallNative` opcode를 생성하고, VM이 실행 시점에 호스트가 등록한 함수를 이름으로 찾아 호출합니다.

---

## 2. Parlance 측 — `native` 선언

### 2.1 기본 문법

```plc
native 함수이름 : 타입시그니처
```

타입 규칙:
- **대문자**로 시작 → 타입 생성자 (`Int`, `Float`, `IO` 등)
- **소문자**로 시작 → 타입 변수 (`a`, `b` 등, 제네릭)
- `->` 는 오른쪽 결합 (curried form)

### 2.2 예제

```plc
// 기본 타입
native print : a -> IO           // 모든 타입 a를 받아 IO 반환

// 정수 연산
native add : Int -> Int -> Int
native sub : Int -> Int -> Int
native mul : Int -> Int -> Int
native div : Int -> Int -> Int

// 불리언 연산
native eq : Int -> Int -> Bool

// 파일 입출력 (IO 필요)
native readFile  : Str -> IO     // 파일 읽기
native writeFile : Str -> Str -> IO

// 난수 생성
native random : Int              // IO가 없으므로 순수 함수로 간주
```

### 2.3 `define`과의 차이점

```plc
// Parlance 함수 — 컴파일러가 바디를 λ-계산법으로 컴파일
define double : Int -> Int = \x -> x + x

// 네이티브 함수 — 호스트가 구현 제공
native sqrt : Int -> Int
```

---

## 3. 호스트 측 — `native` 구현 등록

### 3.1 `VM::register_native()`

VM의 네이티브 함수 시그니처:

```rust
pub type NativeFn = fn(&[Liternal]) -> Result<Liternal, String>;
```

`Liternal`은 GraftVM의 값 타입:

```rust
pub enum Liternal {
    Int(Int),     // Int: Int8 | Int16 | Int32 | Int64
    UInt(UInt),   // UInt: UInt8 | ... | UInt64
    Float(Float), // Float: Float32 | Float64
    String(String),
    Bool(bool),
}
```

### 3.2 기본 구현 예제 (main.rs)

```rust
use graftvm_interpreter::vm::VM;
use graftvm_liternal::Liternal;

let mut vm = VM::new(bytecode);

// 단항 함수 등록
vm.register_native("print", |args| {
    println!("{}", args[0]);     // stdout에 출력
    Ok(args[0].clone())          // 인자를 그대로 반환
});

// 이항 함수 등록 (i64 연산)
vm.register_native("add", |args| {
    let x = args[0].expect_int()?.expect_i64()?;
    let y = args[1].expect_int()?.expect_i64()?;
    Ok(Liternal::from(x + y))
});

// 문자열 처리
vm.register_native("concat", |args| {
    let a = args[0].expect_string()?;
    let b = args[1].expect_string()?;
    Ok(Liternal::from(format!("{}{}", a, b)))
});

vm.run()?;
```

### 3.3 완전한 런타임 예제

다음은 `add`, `print` 네이티브 함수를 직접 등록해서 Parlance 프로그램을 실행하는 완전한 Rust 바이너리 예제입니다.

```rust
// Parlance 런타임 예제 — myapp/src/main.rs
//
// 이 바이너리는 Parlance 컴파일러가 생성한 바이트코드를
// 직접 실행합니다. 네이티브 함수는 호스트가 직접 등록합니다.

use graftvm_interpreter::vm::VM;
use graftvm_liternal::Liternal;

fn main() -> Result<(), String> {
    // Parlance 컴파일러가 생성한 바이트코드 (예: "test.bc")
    let bytecode = vec![
        // ... parlance가 생성한 Opcode들 ...
    ];

    // ── VM 생성 ───────────────────────────────────────────────
    let mut vm = VM::new(bytecode);

    // ── 네이티브 함수 등록 ─────────────────────────────────────

    // print : a -> IO
    vm.register_native("print", |args| {
        let val = &args[0];
        match val {
            Liternal::Int(v) => println!("{}", v),
            Liternal::Str(s) => println!("{}", s),
            _ => println!("{:?}", val),
        }
        Ok(val.clone())
    });

    // add : Int -> Int -> Int
    vm.register_native("add", |args| {
        let x = args[0].expect_int()?.expect_i64()?;
        let y = args[1].expect_int()?.expect_i64()?;
        Ok(Liternal::from(x + y))
    });

    // sub : Int -> Int -> Int
    vm.register_native("sub", |args| {
        let x = args[0].expect_int()?.expect_i64()?;
        let y = args[1].expect_int()?.expect_i64()?;
        Ok(Liternal::from(x - y))
    });

    // mul : Int -> Int -> Int
    vm.register_native("mul", |args| {
        let x = args[0].expect_int()?.expect_i64()?;
        let y = args[1].expect_int()?.expect_i64()?;
        Ok(Liternal::from(x * y))
    });

    // div : Int -> Int -> Int
    vm.register_native("div", |args| {
        let x = args[0].expect_int()?.expect_i64()?;
        let y = args[1].expect_int()?.expect_i64()?;
        Ok(Liternal::from(x / y))
    });

    // eq : Int -> Int -> Bool
    vm.register_native("eq", |args| {
        let x = args[0].expect_int()?.expect_i64()?;
        let y = args[1].expect_int()?.expect_i64()?;
        Ok(Liternal::from(x == y))
    });

    // ── 실행 ──────────────────────────────────────────────────
    vm.run()?;
    println!(";; 프로그램 종료");
    Ok(())
}
```

### 3.4 샌드박스 런타임 예제

출력 기능을 제외한 **순수 계산만 허용**하는 샌드박스:

```rust
fn run_sandboxed(bytecode: Vec<Opcode>) -> Result<(), String> {
    let mut vm = VM::new(bytecode);

    // 순수 계산 함수만 등록
    vm.register_native("add", arithmetic_fn!(add));
    vm.register_native("sub", arithmetic_fn!(sub));
    vm.register_native("mul", arithmetic_fn!(mul));
    vm.register_native("div", arithmetic_fn!(div));

    // print, readFile 등은 등록하지 않음
    // → 프로그램에서 print를 호출하면 런타임 에러

    vm.run()
}

// 도우미 매크로
macro_rules! arithmetic_fn {
    ($op:ident) => {
        |args: &[Liternal]| -> Result<Liternal, String> {
            let x = args[0].expect_int()?.expect_i64()?;
            let y = args[1].expect_int()?.expect_i64()?;
            Ok(Liternal::from(x.$op(y)))
        }
    };
}
```

### 3.5 Cargo.toml 예제 (런타임 바이너리)

```toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2021"

[dependencies]
# parlance 컴파일러는 별도, 바이트코드만 실행
graftvm_interpreter = { git = "https://github.com/yaeju1205/graftvm" }
graftvm_liternal = { git = "https://github.com/yaeju1205/graftvm" }
graftvm_bytecode = { git = "https://github.com/yaeju1205/graftvm" }
```

### 3.6 컴파일 + 실행 통합 예제

Parlance 소스 파일을 컴파일하고 바로 실행하는 완전한 예제:

```rust
use std::process::Command;

fn build_and_run(plc_source: &str) -> Result<(), String> {
    // 1. Parlance 컴파일러 호출 → 바이트코드 파일 생성
    let output = Command::new("parlance")
        .arg(plc_source)
        .arg("-o")
        .arg("out.bc")
        .output()
        .map_err(|e| format!("컴파일 실패: {}", e))?;

    if !output.status.success() {
        return Err(format!("컴파일 에러: {}", String::from_utf8_lossy(&output.stderr)));
    }

    // 2. 바이트코드 로드
    let bytecode = load_bytecode("out.bc")?;

    // 3. VM + 바인딩 + 실행
    let mut vm = VM::new(bytecode);
    vm.register_native("add", /* ... */);
    vm.register_native("print", /* ... */);
    vm.run()
}
```

---

## 4. 샌드박싱

가장 중요한 특징: **호스트가 네이티브 함수를 등록하지 않으면 Parlance에서 사용할 수 없습니다.**

```rust
fn sandboxed_vm(bytecode: Vec<Opcode>) {
    let mut vm = VM::new(bytecode);

    // 순수 계산만 허용 (I/O 없음)
    vm.register_native("add", /* ... */);
    vm.register_native("sub", /* ... */);
    // print, readFile 등은 등록하지 않음
    // → 실행 시 "unknown native 'print'" 런타임 에러

    vm.run()?;
}
```

샌드박싱 레벨 예시:

| 레벨 | 등록하는 함수 | 사용 예 |
|------|---------------|---------|
| **순수 계산** | `add`, `sub`, `mul`, `div` | 스마트 컨트랙트 로직 |
| **표준 출력** | 위 + `print` | REPL, 디버깅 |
| **파일 시스템** | 위 + `readFile`, `writeFile` | 스크립팅 언어 |
| **네트워크** | 위 + `httpGet`, `sendMsg` | 자동화 |

---

## 5. IO 타입과 효과 시스템

`IO` 타입은 **효과(effect)를 나타내는 타입 생성자**입니다.

```plc
// IO가 없음 → 순수 함수, 같은 입력에 항상 같은 출력
native add : Int -> Int -> Int

// IO가 있음 → 효과를 가짐, 호출마다 상태가 달라질 수 있음
native print   : a -> IO
native getLine : IO
native readFile : Str -> IO
```

타입 체커는 `IO`를 일반 타입으로 취급하지만, 의미적으로는:

- `IO`를 반환하는 함수는 **호출 순서가 중요**
- `IO`가 없는 함수는 **언제나 안전하게 인라인/재배치 가능**
- 호스트는 `IO` 함수를 등록할지 말지로 **권한 제어**

---

## 6. 전체 흐름

```
Parlance 소스 (.plc)
  │
  ├─ native add : Int -> Int -> Int    ← "이건 외부 함수야"
  ├─ define x = add 1 2                ← "add를 호출"
  └─ define main = print x
        │
        ▼
[컴파일러]
  │
  ├─ 타입 체커: add는 Int->Int->Int, print는 a->IO
  ├─ IR: CallNative { name: "add", arity: 2 }
  └─ IR: CallNative { name: "print", arity: 1 }
        │
        ▼
[VM 실행]
  │
  ├─ vm.register_native("add", |args| { ... })
  ├─ vm.register_native("print", |args| { ... })
  └─ CallNative → VM이 native_table에서 lookup → 실행
```

---

## 7. 주의사항

1. **arity 일치**: `native add : Int -> Int -> Int`는 arity=2. 호스트 함수는 `&[args]`에서 2개를 받을 것이라 기대.
2. **타입 안전성**: 파서/타입체커는 타입 시그니처를 검증하지만, **런타임 타입 불일치는 호스트 함수 내에서 처리**해야 함.
3. **함수 이름 충돌**: `define`과 `native`는 같은 이름 공간을 공유. 나중에 선언된 것이 이전 것을 덮어씀.
4. **모듈 시스템과의 통합**: `native` 선언은 모듈 시스템을 통해 가져올 수 있음 (`import "prelude"`).
