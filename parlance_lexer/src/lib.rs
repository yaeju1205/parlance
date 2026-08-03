// ── DFA-based scanner (lexer) ────────────────────────────────────
//
// SCANNER THEORY:
//
//   The lexer implements a Deterministic Finite Automaton (DFA)
//   over the input character stream.  The alphabet Σ is partitioned
//   into disjoint character classes:
//
//     CLASS          CHARS                    REGEX
//     ──────────────────────────────────────────────────────
//     whitespace     ' ', '\t', '\n'          [ \t\n]
//     digit          '0'..'9'                 [0-9]
//     ident_start    [a-zA-Z_]                [a-zA-Z_]
//     ident_cont     [a-zA-Z0-9_']            [a-zA-Z0-9_']
//     symbol         !#$%&*+-./<=>?@^|~=      see is_symbol()
//     quote          "                        "
//     lparen         (                        (
//     rparen         )                        )
//     backslash      \                        \\
//
//   DFA STATE TRANSITION DIAGRAM (one token per run from START):
//
//                    ┌──────── digit ────────┐
//                    │                       ▼
//       START ──digit──→  INT  ◄── digit ────┘
//                         │
//                         │   '.' + digit  →  FLOAT  ◄── digit ──┐
//                         │                                      │
//                         │ any other      →  ACCEPT (emit Int)  │
//                         │                                      │
//       START ──'.' + digit ──→  FLOAT ────── digit ─────────────┘
//                                      │
//                                      │ any non-digit → ACCEPT (emit Float)
//                                      │
//                    ┌─── ident_cont ───────┐
//                    │                      ▼
//       START ──ident──→  IDENT ◄── ident_cont ─┘
//                         │
//                         │   "::" pair  →  continue as IDENT
//                         │   (table::foo lexes as one Ident)
//                         │
//                         │ any other  →  ACCEPT (emit Ident/keyword)
//
//   NOTE ON ':' :
//     A SINGLE ':' is NOT part of the IDENT alphabet — it always
//     lexes as Token::Colon (used by `define x : Int = ...` type
//     annotations).  Only the two-character sequence `::` continues
//     an identifier, and that rule lives in the scanner loop so the
//     two cases (`x :: y` vs `x : y`) never collide.
//
//                    ┌─── (any non-quote, non-backslash) ──┐
//                    │                                     │
//                    ▼                                     │
//       START ──quote──→  STRING ──→  STRING_ESC ──→ ──────┘
//                         │              │
//                         │ quote        │ any
//                         ▼              ▼
//                       ACCEPT          ACCEPT (after ESC)
//
//                    ┌─── symbol ────────┐
//                    │                   ▼
//       START ──symbol──→  SYMBOL ◄── symbol ──┘
//                         │
//                         │ any non-symbol  →  ACCEPT (emit Op/punctuation)
//
//       START ──\──→  ACCEPT (emit Backslash)
//       START ──(──→  ACCEPT (emit LParen)
//       START ──)──→  ACCEPT (emit RParen)
//       START ──other──→  ERROR
//
//   POST-ACCEPT RESOLUTION:
//     After the DFA accepts an IDENT or SYMBOL token, the raw string
//     is checked against a fixed table for keywords and multi-character
//     punctuation (=>, <-, >>=, =).  This is equivalent to having a
//     SECONDARY classifier on the DFA's output.
//
//     IDENT →  lookup: "import"|"define"|"infix"|"var"  → keyword
//                      otherwise                        → Ident(String)
//
//     SYMBOL → lookup: "->"|"="|"<-"|">>="              → punctuation
//                      otherwise                        → Op(String)
//
//   MAXIMAL MUNCH:
//     Both the IDENT and SYMBOL loops consume the longest possible
//     run of their character class before accepting.  This prevents
//     `>>=` from being split into `>` `> `=`.
//
//   STRING ESCAPE HANDLING:
//     Inside a string, `\` transitions the DFA to a temporary escape
//     state that consumes exactly one more character (the escape code)
//     and returns to STRING.  The recognised escapes are: \n, \t,
//     \", \\.

mod token;

pub use token::*;

// ── Character-class predicates ───────────────────────────────────
//  These partition Σ into the DFA's alphabet categories.

/// FIRST(ident) = { [a-zA-Z_] }
#[inline]
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Continuation chars for identifiers.  Accepts the single quote
/// so that names like `x'` are valid (common in lambda-calculus
/// style renamed variables).
#[inline]
fn is_ident_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '\''
}

/// Symbol characters that can form user-defined operators.
/// `->`, `<-`, `>>=` are tokenised as symbol runs first, then resolved.
#[inline]
fn is_symbol(c: char) -> bool {
    matches!(
        c,
        '!' | '#'
            | '$'
            | '%'
            | '&'
            | '*'
            | '+'
            | '-'
            | '.'
            | '/'
            | '<'
            | '>'
            | '?'
            | '@'
            | '^'
            | '|'
            | '~'
            | '='
    )
}

// ── Scanner (DFA implementation) ─────────────────────────────────

/// Run the DFA over the source text and produce a flat token stream.
///
/// The function implements the state machine described at the top of
/// this file.  `pos` is the DFA's current position in `chars`;
/// `line`/`col` track source location for error reporting.
pub fn tokenize(src: &str) -> Result<Vec<Spanned>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut pos = 0;
    let mut line = 1;
    let mut col = 1;
    let mut tokens = Vec::new();

    loop {
        // ── DFA: START state ─────────────────────────────────────
        //  Consume all whitespace (ε-transition from START on whitespace).
        while pos < chars.len() && chars[pos].is_whitespace() {
            if chars[pos] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            pos += 1;
        }

        if pos >= chars.len() {
            break; // End of input — emit Eof below.
        }

        let start_line = line;
        let start_col = col;
        let c = chars[pos];

        let token = if c.is_ascii_digit() {
            // ── DFA: INT → FLOAT ──────────────────────────────────
            //  δ(START, digit) → INT
            //  δ(INT,  digit) → INT   (self-loop)
            //  δ(INT,  '.' + digit) → FLOAT
            //  δ(FLOAT, digit) → FLOAT (self-loop)
            //  δ(INT,  other) → ACCEPT (emit Int)
            //  δ(FLOAT, other) → ACCEPT (emit Float)
            let start = pos;
            pos += 1;
            col += 1;
            while pos < chars.len() && chars[pos].is_ascii_digit() {
                pos += 1;
                col += 1;
            }
            // Peek for fractional part: if '.' followed by digit → FLOAT
            if pos < chars.len()
                && chars[pos] == '.'
                && pos + 1 < chars.len()
                && chars[pos + 1].is_ascii_digit()
            {
                pos += 1; // consume '.'
                col += 1;
                while pos < chars.len() && chars[pos].is_ascii_digit() {
                    pos += 1;
                    col += 1;
                }
                let s: String = chars[start..pos].iter().collect();
                Token::Float(s.parse::<f64>().unwrap_or(0.0))
            } else {
                let s: String = chars[start..pos].iter().collect();
                Token::Int(s.parse().unwrap_or(0))
            }
        } else if is_ident_start(c) {
            // ── DFA: IDENT state ─────────────────────────────────
            //  δ(START, ident_start) → IDENT
            //  δ(IDENT, ident_cont)  → IDENT   (self-loop)
            //  δ(IDENT, "::")        → IDENT   (double-colon self-loop)
            //  δ(IDENT, other)       → ACCEPT
            //
            //  A `::` PAIR continues an identifier, so `table::foo`
            //  and `tbl::index` lex as ONE Ident token.  This is
            //  implemented in the scanner loop (NOT by adding ':'
            //  to is_ident_cont): a single ':' must still lex as
            //  Token::Colon for `define x : Int = ...` annotations.
            let start = pos;
            loop {
                // Consume the ordinary ident_cont run.
                while pos < chars.len() && is_ident_cont(chars[pos]) {
                    pos += 1;
                    col += 1;
                }
                // If a `::` pair follows, consume both colons and keep
                // scanning as the same identifier; otherwise accept.
                if pos + 1 < chars.len() && chars[pos] == ':' && chars[pos + 1] == ':' {
                    pos += 2;
                    col += 2;
                } else {
                    break;
                }
            }
            let s: String = chars[start..pos].iter().collect();
            // Post-accept keyword resolution table
            match s.as_str() {
                "import" => Token::Import,
                "define" => Token::Define,
                "infix" => Token::Infix,
                "var" => Token::Var,
                "native" => Token::Native,
                _ => Token::Ident(s),
            }
        } else if c == '"' {
            // ── DFA: STRING state ────────────────────────────────
            //  δ(START, '"') → STRING
            //  δ(STRING, any but '"' or '\') → STRING
            //  δ(STRING, '"') → ACCEPT
            //  δ(STRING, '\') → STRING_ESC
            //  δ(STRING_ESC, any) → STRING
            pos += 1;
            col += 1;
            let mut s = String::new();
            loop {
                match chars.get(pos) {
                    None => {
                        return Err(format!(
                            "{}:{}: unterminated string literal",
                            start_line, start_col
                        ));
                    }
                    Some(&'"') => {
                        pos += 1;
                        col += 1;
                        break;
                    }
                    Some(&'\\') => {
                        pos += 1;
                        col += 1;
                        match chars.get(pos) {
                            None => {
                                return Err(format!(
                                    "{}:{}: unterminated escape sequence",
                                    start_line, start_col
                                ));
                            }
                            Some(&'n') => s.push('\n'),
                            Some(&'t') => s.push('\t'),
                            Some(&'"') => s.push('"'),
                            Some(&'\\') => s.push('\\'),
                            Some(other) => s.push(*other),
                        }
                        pos += 1;
                        col += 1;
                    }
                    Some(&ch) => {
                        s.push(ch);
                        pos += 1;
                        col += 1;
                    }
                }
            }
            Token::Str(s)
        } else if is_symbol(c) {
            // ── DFA: SYMBOL state ────────────────────────────────
            //  δ(START, symbol) → SYMBOL
            //  δ(SYMBOL, symbol) → SYMBOL   (self-loop, maximal munch)
            //  δ(SYMBOL, other)  → ACCEPT
            let start = pos;
            while pos < chars.len() && is_symbol(chars[pos]) {
                pos += 1;
                col += 1;
            }
            let s: String = chars[start..pos].iter().collect();
            // Post-accept multi-char punctuation resolution
            match s.as_str() {
                "->" => Token::Arrow,
                "=" => Token::Equal,
                "<-" => Token::Bind,
                ">>=" => Token::BindChain,
                _ => Token::Op(s),
            }
        } else if c == '\\' {
            // ── DFA: BACKSLASH (single-character accept) ────────
            pos += 1;
            col += 1;
            Token::Backslash
        } else if c == '(' {
            pos += 1;
            col += 1;
            Token::LParen
        } else if c == ')' {
            pos += 1;
            col += 1;
            Token::RParen
        } else if c == ',' {
            pos += 1;
            col += 1;
            Token::Comma
        } else if c == ':' {
            pos += 1;
            col += 1;
            Token::Colon
        } else {
            // ── DFA: ERROR state (no transition for this char) ──
            return Err(format!(
                "{}:{}: unexpected character '{}'",
                start_line, start_col, c
            ));
        };

        tokens.push(Spanned {
            token,
            line: start_line,
            col: start_col,
        });
    }

    // ── DFA: accept EOF sentinel ──────────────────────────────
    tokens.push(Spanned {
        token: Token::Eof,
        line,
        col,
    });

    Ok(tokens)
}

// ── Self-tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Tokenize and drop the Eof sentinel.
    fn toks(src: &str) -> Vec<Token> {
        let mut v: Vec<Token> = tokenize(src)
            .unwrap()
            .into_iter()
            .map(|s| s.token)
            .collect();
        assert_eq!(v.pop(), Some(Token::Eof));
        v
    }

    #[test]
    fn double_colon_lexes_as_one_ident() {
        assert_eq!(toks("table::foo"), vec![Token::Ident("table::foo".into())]);
        assert_eq!(toks("tbl::index"), vec![Token::Ident("tbl::index".into())]);
        // Multiple :: segments stay a single identifier.
        assert_eq!(
            toks("a::b::c"),
            vec![Token::Ident("a::b::c".into())]
        );
        // :: pairs may interleave with ordinary ident chars.
        assert_eq!(
            toks("x'::y"),
            vec![Token::Ident("x'::y".into())]
        );
    }

    #[test]
    fn double_colon_inside_larger_expression() {
        assert_eq!(
            toks("table::index \"cat\""),
            vec![
                Token::Ident("table::index".into()),
                Token::Str("cat".into()),
            ]
        );
        assert_eq!(
            toks("define x = table::foo"),
            vec![
                Token::Define,
                Token::Ident("x".into()),
                Token::Equal,
                Token::Ident("table::foo".into()),
            ]
        );
    }

    #[test]
    fn single_colon_still_lexes_as_colon() {
        // Type annotation with spaces: `x : Int`
        assert_eq!(
            toks("x : Int"),
            vec![
                Token::Ident("x".into()),
                Token::Colon,
                Token::Ident("Int".into()),
            ]
        );
        // Type annotation WITHOUT spaces: `x:Int` must not become `x:Int`
        // as one identifier — the lone ':' stays Token::Colon.
        assert_eq!(
            toks("x:Int"),
            vec![
                Token::Ident("x".into()),
                Token::Colon,
                Token::Ident("Int".into()),
            ]
        );
        // define x : Int = 42 end-to-end.
        assert_eq!(
            toks("define x : Int = 42"),
            vec![
                Token::Define,
                Token::Ident("x".into()),
                Token::Colon,
                Token::Ident("Int".into()),
                Token::Equal,
                Token::Int(42),
            ]
        );
        // A single colon after an identifier must NOT glue the next
        // identifier onto it.
        assert_eq!(
            toks("foo:bar"),
            vec![
                Token::Ident("foo".into()),
                Token::Colon,
                Token::Ident("bar".into()),
            ]
        );
    }
}
