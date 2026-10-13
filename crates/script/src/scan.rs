//! A quick look at the shape of a script before Boa parses it. Boa's parser and compiler recurse
//! once per level of brackets and once per link of a long expression chain (`1+1+…`, `a?b:c?…`,
//! `!!…`, `a.b.c…`), and running out of stack aborts the app: there is no panic to catch. Scripts
//! nested deeper than [`MAX_NESTING`] brackets, or whose expressions get too long, are refused here
//! with a position instead. Measured 2026-10-11 (release build, 1 MiB stack): about 36 levels of
//! brackets or 300 links of `?:` fit; scripts run with at least 32 MiB ([`crate::SCRIPT_STACK`] and
//! the web build's stack), so the limits below keep a margin of about four.

/// Most levels of brackets (`(`, `[`, `{`, `${`) open at once.
pub(crate) const MAX_NESTING: usize = 256;
/// Most operators chained in the expressions open at once (a level of brackets counts as
/// [`BRACKET_COST`] of them).
pub(crate) const MAX_CHAIN: usize = 2560;
const BRACKET_COST: usize = 10;

/// Where and why a script is refused.
#[derive(Debug, PartialEq)]
pub(crate) struct TooDeep {
    pub(crate) message: String,
    /// 1-based.
    pub(crate) line: u32,
    pub(crate) column: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// `(` that groups (commas in it chain a sequence expression).
    Group,
    /// `(` of a call, `[`, `{`: commas separate items.
    List,
    /// `${` inside a template literal: its `}` goes back into the template.
    Template,
}

struct Frame {
    kind: Kind,
    /// Operators chained at this level since its last separator.
    chain: usize,
}

/// Keywords after which `/` starts a regular expression, which also chain what follows.
const PREFIX_WORDS: &[&str] =
    &["return", "typeof", "instanceof", "in", "of", "new", "delete", "void", "throw", "case", "do", "else", "yield", "await"];

struct Scanner<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: u32,
    column: u32,
    frames: Vec<Frame>,
    /// The chain links of every open frame, and the brackets' cost.
    cost: usize,
    /// The last token ended an expression (an operand, `)`, `]`): `/` divides, `(` calls.
    after_operand: bool,
    /// A line break since the last token.
    newline: bool,
}

/// Refuse `source` when it nests or chains too deeply for the parser.
pub(crate) fn check(source: &str) -> Result<(), TooDeep> {
    let mut s = Scanner {
        chars: source.chars().peekable(),
        line: 1,
        column: 0,
        frames: vec![Frame { kind: Kind::List, chain: 0 }],
        cost: 0,
        after_operand: false,
        newline: false,
    };
    s.run()
}

impl Scanner<'_> {
    fn next(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.column = 0;
            self.newline = true;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn refuse(&self, message: String) -> TooDeep {
        TooDeep { message, line: self.line, column: self.column.max(1) }
    }

    /// One more operator in the current expression.
    fn link(&mut self) -> Result<(), TooDeep> {
        if let Some(f) = self.frames.last_mut() {
            f.chain += 1;
        }
        self.cost += 1;
        if self.cost > MAX_CHAIN {
            return Err(self.refuse(format!("an expression here is too long or too deeply nested (more than {MAX_CHAIN} operators)")));
        }
        Ok(())
    }

    /// The current expression ended (`;`, a list's `,`, a new statement on a new line).
    fn separate(&mut self) {
        if let Some(f) = self.frames.last_mut() {
            self.cost -= f.chain;
            f.chain = 0;
        }
    }

    fn open(&mut self, kind: Kind) -> Result<(), TooDeep> {
        self.frames.push(Frame { kind, chain: 0 });
        self.cost += BRACKET_COST;
        // The outermost frame is the script itself.
        if self.frames.len() > MAX_NESTING + 1 {
            return Err(self.refuse(format!("the script is nested too deeply here (more than {MAX_NESTING} levels of brackets)")));
        }
        if self.cost > MAX_CHAIN {
            return Err(self.refuse(format!("an expression here is too long or too deeply nested (more than {MAX_CHAIN} operators)")));
        }
        self.after_operand = false;
        Ok(())
    }

    /// Close the innermost bracket → its kind (none for a stray closer).
    fn close(&mut self) -> Option<Kind> {
        if self.frames.len() < 2 {
            return None;
        }
        let f = self.frames.pop()?;
        self.cost -= f.chain + BRACKET_COST;
        Some(f.kind)
    }

    /// An operand starts: on a new line after a finished expression it starts a new statement.
    fn operand(&mut self) {
        if self.after_operand && self.newline {
            self.separate();
        }
        self.after_operand = true;
    }

    fn run(&mut self) -> Result<(), TooDeep> {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.next();
                continue;
            }
            match c {
                '/' => {
                    self.next();
                    match self.peek() {
                        // Comments keep the line break before them.
                        Some('/') => {
                            self.skip_line();
                            continue;
                        }
                        Some('*') => {
                            self.skip_block_comment();
                            continue;
                        }
                        _ if self.after_operand => {
                            if self.peek() == Some('=') {
                                self.next();
                            }
                            self.after_operand = false;
                            self.link()?;
                        }
                        _ => {
                            self.skip_regex();
                            self.operand();
                        }
                    }
                }
                '\'' | '"' => {
                    self.next();
                    self.skip_string(c);
                    self.operand();
                }
                '`' => {
                    self.next();
                    self.operand();
                    self.template()?;
                }
                '(' | '[' => {
                    self.next();
                    let call = self.after_operand;
                    if call {
                        self.link()?;
                    }
                    self.open(if c == '(' && !call { Kind::Group } else { Kind::List })?;
                }
                '{' => {
                    self.next();
                    if self.after_operand && self.newline {
                        self.separate();
                    }
                    self.open(Kind::List)?;
                }
                ')' | ']' => {
                    self.next();
                    self.close();
                    self.after_operand = true;
                }
                '}' => {
                    self.next();
                    if self.close() == Some(Kind::Template) {
                        self.template()?;
                        self.after_operand = true;
                    } else {
                        // A block ended: what follows starts afresh.
                        self.separate();
                        self.after_operand = false;
                    }
                }
                ';' => {
                    self.next();
                    self.separate();
                    self.after_operand = false;
                }
                ',' => {
                    self.next();
                    if self.frames.last().is_some_and(|f| f.kind == Kind::Group) {
                        self.link()?;
                    } else {
                        self.separate();
                    }
                    self.after_operand = false;
                }
                c if c.is_ascii_digit() || (c == '.' && !self.after_operand) => {
                    self.skip_word();
                    self.operand();
                }
                c if c.is_alphanumeric() || c == '_' || c == '$' || c == '\\' || !c.is_ascii() => {
                    let word = self.word();
                    if PREFIX_WORDS.contains(&word.as_str()) {
                        if self.after_operand && self.newline && !matches!(word.as_str(), "instanceof" | "in" | "of") {
                            self.separate();
                        }
                        self.after_operand = false;
                        self.link()?;
                    } else {
                        self.operand();
                    }
                }
                _ => {
                    // Punctuation: one operator (`+`, `===`, `?.`, `=>`, `.`…); runs of the same
                    // characters such as `!!!` are as many links.
                    self.next();
                    self.after_operand = false;
                    self.link()?;
                }
            }
            self.newline = false;
        }
        Ok(())
    }

    fn word(&mut self) -> String {
        let mut w = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '$' || c == '\\' || !c.is_ascii() && !c.is_whitespace() {
                w.push(c);
                self.next();
            } else {
                break;
            }
        }
        w
    }

    /// A number: digits, letters (hex, exponents, BigInt `n`), `_` and `.`.
    fn skip_word(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                self.next();
            } else {
                break;
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.next();
        }
    }

    fn skip_block_comment(&mut self) {
        self.next();
        let mut star = false;
        while let Some(c) = self.next() {
            if star && c == '/' {
                return;
            }
            star = c == '*';
        }
    }

    fn skip_string(&mut self, quote: char) {
        while let Some(c) = self.next() {
            match c {
                '\\' => {
                    self.next();
                }
                '\n' => return,
                c if c == quote => return,
                _ => {}
            }
        }
    }

    /// The rest of a regular expression after its `/`: its brackets don't count.
    fn skip_regex(&mut self) {
        let mut class = false;
        while let Some(c) = self.next() {
            match c {
                '\\' => {
                    self.next();
                }
                '[' => class = true,
                ']' => class = false,
                '/' if !class => break,
                '\n' => return,
                _ => {}
            }
        }
        // Its flags.
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.next();
        }
    }

    /// The text of a template literal after its `` ` ``, up to its end or a `${`.
    fn template(&mut self) -> Result<(), TooDeep> {
        while let Some(c) = self.next() {
            match c {
                '\\' => {
                    self.next();
                }
                '`' => return Ok(()),
                '$' if self.peek() == Some('{') => {
                    self.next();
                    return self.open(Kind::Template);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Refuse JSON text nested deeper than [`MAX_NESTING`] (Boa's `JSON.parse` recurses per level).
pub(crate) fn check_json(text: &str) -> Result<(), String> {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for c in text.chars() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' | '{' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return Err(format!("JSON nested more than {MAX_NESTING} levels deep is not read"));
                }
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(src: &str) -> Option<(u32, u32, bool)> {
        check(src).err().map(|e| (e.line, e.column, e.message.contains("nested too deeply here")))
    }

    #[test]
    fn ordinary_scripts_pass() {
        for src in [
            "const a = [1, 2, [3, {b: 4}]];\nfunction f(x) { return x ? f(x - 1) : 0; }\nf(3)",
            "for (let i = 0; i < 10; i++) { if (i % 2) { vc.run('x', {a: [i]}); } }",
            "const t = `a ${1 + `b ${2}`} c`; const r = /[(]+\\//g; const d = 4 / 2 / 1;",
            "x = 1\ny = 2\nz = x\n+ y",
            "return /a(b)/.test(s)",
        ] {
            assert_eq!(refused(src), None, "{src}");
        }
        // Long data lists and many statements are fine.
        assert_eq!(refused(&format!("[{}]", vec!["-1"; 100_000].join(","))), None);
        assert_eq!(refused(&"a = a + 1;\n".repeat(50_000)), None);
        assert_eq!(refused(&"a = a + 1\n".repeat(50_000)), None);
        assert_eq!(refused(&format!("f({})", vec!["1 + 1"; 10_000].join(", "))), None);
    }

    #[test]
    fn deep_brackets_are_refused_where_they_get_too_deep() {
        let src = format!("ok;\n  {}{}", "[".repeat(300), "]".repeat(300));
        assert_eq!(refused(&src), Some((2, 2 + MAX_NESTING as u32 + 1, true)));
        assert_eq!(refused(&format!("{}{}", "[".repeat(MAX_NESTING), "]".repeat(MAX_NESTING))), None);
        assert!(refused(&format!("x = `{}1{}`", "${`".repeat(300), "`}".repeat(300))).is_some());
        assert!(refused(&format!("{}1{}", "(()=>".repeat(300), ")".repeat(300))).is_some());
    }

    #[test]
    fn brackets_in_strings_comments_templates_and_regexes_dont_count() {
        let b = "[(".repeat(1000);
        for src in [
            format!("'{b}'"),
            format!("\"{b}\""),
            format!("// {b}"),
            format!("/* {b} */"),
            format!("`{b}`"),
            format!("/[{b}]/"),
            format!("return /\\({}/", "\\(".repeat(1000)),
        ] {
            assert_eq!(refused(&src), None, "{}", &src[..20]);
        }
    }

    #[test]
    fn long_chains_are_refused() {
        for src in [
            format!("1{}", "+1".repeat(5000)),
            format!("{}1", "1?1:".repeat(3000)),
            format!("{}1", "!".repeat(5000)),
            format!("a{}", ".a".repeat(5000)),
            format!("f{}", "()".repeat(5000)),
            format!("({})", vec!["1"; 5000].join(",")),
            format!("{}1", "typeof ".repeat(5000)),
        ] {
            let e = check(&src).unwrap_err();
            assert!(e.message.contains("too long"), "{}: {}", &src[..10], e.message);
        }
        assert_eq!(refused(&format!("1{}", "+1".repeat(2000))), None);
    }

    #[test]
    fn json_depth() {
        assert!(check_json(&format!("{}{}", "[".repeat(300), "]".repeat(300))).is_err());
        assert!(check_json(&format!("[\"{}\"]", "[".repeat(1000))).is_ok());
        assert!(check_json("{\"a\": [1, {\"b\": \"\\\"[\"}]}").is_ok());
    }
}
