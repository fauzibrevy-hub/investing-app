//! Formula evaluator for pack metrics (research plan §4.3).
//!
//! Grammar: `+ - * /`, unary minus, parentheses, numbers, identifiers bound in the metric's
//! `elements`, and the functions `avg4q(x)`, `ttm(x)`, `yoy(x)`, `lag(x, n)`. Any expression
//! may sit inside a function. A missing operand or a division by zero yields a gap with its
//! reason, never zero; an unresolvable unit mix (two currencies) is a gap too.
//!
//! Views: a bare identifier is read under the metric's period view (`instant`, `standalone`,
//! `ttm`, `average4q`). `ttm(e)` sums `e` over the four latest standalone quarters; `avg4q(e)`
//! averages `e` over the five latest instants (opening and four closings, plan §4.4);
//! `yoy(e)` is `(e(q) - e(q-4)) / |e(q-4)|` (the payload's YoY rule); `lag(e, n)` is `e` at `q-n`. Element alternates are tried
//! in order per period, so a rename in a later taxonomy still populates earlier years.
//!
//! Pure: the evaluator asks a `Source` for values and knows nothing about SQLite.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;

use super::def::{Binding, Formula, Layer, Pack, PackSet, Period};

// ---- expression tree ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Avg4q,
    Ttm,
    Yoy,
    Lag,
}

impl Func {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "avg4q" => Some(Self::Avg4q),
            "ttm" => Some(Self::Ttm),
            "yoy" => Some(Self::Yoy),
            "lag" => Some(Self::Lag),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Ident(String),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Call(Func, Box<Expr>, Option<u32>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormulaError {
    #[error("unexpected character `{0}` at {1}")]
    Char(char, usize),
    #[error("unexpected end of formula")]
    Eof,
    #[error("unexpected `{0}` at {1}")]
    Token(String, usize),
    #[error("unknown function `{0}`")]
    Function(String),
    #[error("`lag` needs an expression and a whole number of quarters")]
    LagArgs,
    #[error("`{0}` takes one argument")]
    Arity(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, FormulaError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let v = text
                .parse::<f64>()
                .map_err(|_| FormulaError::Token(text.clone(), start))?;
            out.push((Tok::Num(v), start));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            out.push((Tok::Ident(chars[start..i].iter().collect()), start));
        } else {
            let tok = match c {
                '+' | '-' | '*' | '/' => Tok::Op(c),
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                ',' => Tok::Comma,
                other => return Err(FormulaError::Char(other, i)),
            };
            out.push((tok, i));
            i += 1;
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn next(&mut self) -> Option<(Tok, usize)> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expect(&mut self, want: Tok) -> Result<(), FormulaError> {
        match self.next() {
            Some((t, _)) if t == want => Ok(()),
            Some((t, at)) => Err(FormulaError::Token(describe(&t), at)),
            None => Err(FormulaError::Eof),
        }
    }

    fn sum(&mut self) -> Result<Expr, FormulaError> {
        let mut lhs = self.product()?;
        while let Some(Tok::Op(c @ ('+' | '-'))) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.product()?;
            let op = if c == '+' { Op::Add } else { Op::Sub };
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn product(&mut self) -> Result<Expr, FormulaError> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Op(c @ ('*' | '/'))) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.unary()?;
            let op = if c == '*' { Op::Mul } else { Op::Div };
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, FormulaError> {
        if let Some(Tok::Op('-')) = self.peek() {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if let Some(Tok::Op('+')) = self.peek() {
            self.pos += 1;
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, FormulaError> {
        match self.next() {
            None => Err(FormulaError::Eof),
            Some((Tok::Num(v), _)) => Ok(Expr::Num(v)),
            Some((Tok::LParen, _)) => {
                let e = self.sum()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some((Tok::Ident(name), _)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.pos += 1;
                    let func = Func::parse(&name).ok_or(FormulaError::Function(name.clone()))?;
                    let arg = self.sum()?;
                    let n = if func == Func::Lag {
                        self.expect(Tok::Comma).map_err(|_| FormulaError::LagArgs)?;
                        match self.next() {
                            Some((Tok::Num(v), _))
                                if v.fract() == 0.0 && (0.0..=40.0).contains(&v) =>
                            {
                                Some(v as u32)
                            }
                            _ => return Err(FormulaError::LagArgs),
                        }
                    } else {
                        if let Some(Tok::Comma) = self.peek() {
                            return Err(FormulaError::Arity(name));
                        }
                        None
                    };
                    self.expect(Tok::RParen)?;
                    Ok(Expr::Call(func, Box::new(arg), n))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Some((t, at)) => Err(FormulaError::Token(describe(&t), at)),
        }
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(v) => v.to_string(),
        Tok::Ident(s) => s.clone(),
        Tok::Op(c) => c.to_string(),
        Tok::LParen => "(".into(),
        Tok::RParen => ")".into(),
        Tok::Comma => ",".into(),
    }
}

/// Parses a formula into its tree.
pub fn parse(src: &str) -> Result<Expr, FormulaError> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err(FormulaError::Eof);
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.sum()?;
    match p.next() {
        None => Ok(e),
        Some((t, at)) => Err(FormulaError::Token(describe(&t), at)),
    }
}

/// Every identifier the formula reads.
pub fn identifiers(e: &Expr) -> BTreeSet<String> {
    fn walk(e: &Expr, out: &mut BTreeSet<String>) {
        match e {
            Expr::Num(_) => {}
            Expr::Ident(s) => {
                out.insert(s.clone());
            }
            Expr::Neg(x) | Expr::Call(_, x, _) => walk(x, out),
            Expr::Bin(_, a, b) => {
                walk(a, out);
                walk(b, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(e, &mut out);
    out
}

/// Identifiers read as a four-quarter sum: everything inside `ttm(...)`, plus every bare
/// identifier when the metric's default view is TTM. `avg4q` reads instants and `yoy` /
/// `lag` keep the enclosing view. Used to refuse an annual KPI in that position (R3).
pub fn summed_identifiers(e: &Expr, default_ttm: bool) -> BTreeSet<String> {
    fn walk(e: &Expr, summing: bool, out: &mut BTreeSet<String>) {
        match e {
            Expr::Num(_) => {}
            Expr::Ident(id) => {
                if summing {
                    out.insert(id.clone());
                }
            }
            Expr::Neg(x) => walk(x, summing, out),
            Expr::Bin(_, a, b) => {
                walk(a, summing, out);
                walk(b, summing, out);
            }
            Expr::Call(Func::Ttm, x, _) => walk(x, true, out),
            Expr::Call(Func::Avg4q, x, _) => walk(x, false, out),
            Expr::Call(Func::Yoy | Func::Lag, x, _) => walk(x, summing, out),
        }
    }
    let mut out = BTreeSet::new();
    walk(e, default_ttm, &mut out);
    out
}

/// True when the formula divides somewhere (a ratio); `yoy` is a ratio by definition.
pub fn has_division(e: &Expr) -> bool {
    match e {
        Expr::Num(_) | Expr::Ident(_) => false,
        Expr::Call(Func::Yoy, _, _) => true,
        Expr::Neg(x) | Expr::Call(_, x, _) => has_division(x),
        Expr::Bin(Op::Div, _, _) => true,
        Expr::Bin(_, a, b) => has_division(a) || has_division(b),
    }
}

/// Denominator of a ratio formula (`a / b`, `100 * a / b`, `-a / b`): the right side of the
/// outermost division, `None` when the formula does not divide at the top.
pub fn denominator(e: &Expr) -> Option<&Expr> {
    match e {
        Expr::Bin(Op::Div, _, b) => Some(b),
        Expr::Bin(Op::Mul, a, b) => denominator(a).or_else(|| denominator(b)),
        Expr::Neg(x) => denominator(x),
        _ => None,
    }
}

// ---- evaluation ----

/// A fiscal quarter, calendar years (plan §4.4 ASUMSI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Quarter {
    pub fy: i32,
    pub q: u32,
}

impl Quarter {
    pub fn new(fy: i32, q: u32) -> Self {
        Self { fy, q }
    }

    /// `n` quarters earlier.
    pub fn back(self, n: u32) -> Self {
        let idx = self.fy as i64 * 4 + (self.q as i64 - 1) - n as i64;
        Self {
            fy: idx.div_euclid(4) as i32,
            q: (idx.rem_euclid(4) + 1) as u32,
        }
    }

    pub fn label(self) -> String {
        format!("{}Q{}", self.fy, self.q)
    }
}

impl fmt::Display for Quarter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}Q{}", self.fy, self.q)
    }
}

/// How an element is read at a quarter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// Value at the quarter end (instants only).
    Instant,
    /// Standalone flow of the quarter (flows), or the instant value (instants).
    Standalone,
    /// Trailing four quarters (flows only).
    Ttm,
    /// Mean of the five latest instants.
    Avg4q,
}

/// Where values come from. Implemented over the store by the API, by maps in tests.
pub trait Source {
    /// Value of one element (with canonical dims) at `q` under `view`; `None` when the
    /// filing does not carry it or the view does not apply.
    fn element(&self, element: &str, dims: &str, q: Quarter, view: View) -> Option<f64>;
    /// An entered KPI value; an explicit "unknown" is `None` like a missing one.
    fn kpi(&self, pack: &str, metric: &str, q: Quarter) -> Option<f64>;
    /// The unit the facts of `element` carry (`IDR`, `USD`, `pure`), for the unit check.
    fn unit(&self, element: &str) -> Option<String>;
}

/// Why a cell has no value.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Gap {
    /// No alternate of `ident` carried a value at `q` under `view`.
    Missing {
        ident: String,
        q: Quarter,
        view: View,
    },
    DivisionByZero,
    /// Operands reported in different currencies.
    UnitMismatch {
        units: Vec<String>,
    },
    /// A `metric:` chain looped or went too deep.
    Cycle {
        metric: String,
    },
    /// A metric reference that the pack set cannot resolve.
    Unresolved {
        reference: String,
    },
}

impl Gap {
    pub fn describe(&self) -> String {
        match self {
            Self::Missing { ident, q, view } => {
                format!("{ident} tidak ada untuk {q} ({})", view_label(*view))
            }
            Self::DivisionByZero => "pembagi nol".into(),
            Self::UnitMismatch { units } => format!("satuan berbeda: {}", units.join(", ")),
            Self::Cycle { metric } => format!("rujukan metrik berputar: {metric}"),
            Self::Unresolved { reference } => format!("rujukan tidak dikenal: {reference}"),
        }
    }
}

fn view_label(v: View) -> &'static str {
    match v {
        View::Instant => "posisi",
        View::Standalone => "kuartal",
        View::Ttm => "TTM",
        View::Avg4q => "rata-rata 4 kuartal",
    }
}

/// One operand that fed a value: which alternate was used, at which quarter and view.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Used {
    pub ident: String,
    pub source: String,
    pub q: Quarter,
    pub view: View,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Outcome {
    pub value: Option<f64>,
    pub gap: Option<Gap>,
    pub inputs: Vec<Used>,
}

impl Outcome {
    fn gap(gap: Gap, inputs: Vec<Used>) -> Self {
        Self {
            value: None,
            gap: Some(gap),
            inputs,
        }
    }
}

const MAX_DEPTH: u8 = 8;

/// Evaluation context: the source plus the pack set for `metric:`/`kpi:` references.
pub struct Ctx<'a> {
    pub source: &'a dyn Source,
    pub packs: &'a PackSet,
}

struct Run<'a> {
    ctx: &'a Ctx<'a>,
    pack: &'a Pack,
    bindings: &'a BTreeMap<String, Vec<Binding>>,
    default: View,
    inputs: Vec<Used>,
    depth: u8,
}

impl Run<'_> {
    fn eval(&mut self, e: &Expr, view: View, q: Quarter) -> Result<f64, Gap> {
        match e {
            Expr::Num(v) => Ok(*v),
            Expr::Ident(name) => self.ident(name, view, q),
            Expr::Neg(x) => Ok(-self.eval(x, view, q)?),
            Expr::Bin(op, a, b) => {
                let l = self.eval(a, view, q)?;
                let r = self.eval(b, view, q)?;
                Ok(match op {
                    Op::Add => l + r,
                    Op::Sub => l - r,
                    Op::Mul => l * r,
                    Op::Div => {
                        if r == 0.0 {
                            return Err(Gap::DivisionByZero);
                        }
                        l / r
                    }
                })
            }
            Expr::Call(Func::Ttm, x, _) => {
                let mut sum = 0.0;
                for i in 0..4 {
                    sum += self.eval(x, View::Standalone, q.back(i))?;
                }
                Ok(sum)
            }
            Expr::Call(Func::Avg4q, x, _) => {
                let mut sum = 0.0;
                for i in 0..5 {
                    sum += self.eval(x, View::Instant, q.back(i))?;
                }
                Ok(sum / 5.0)
            }
            Expr::Call(Func::Yoy, x, _) => {
                let now = self.eval(x, view, q)?;
                let then = self.eval(x, view, q.back(4))?;
                if then == 0.0 {
                    return Err(Gap::DivisionByZero);
                }
                Ok((now - then) / then.abs())
            }
            Expr::Call(Func::Lag, x, n) => self.eval(x, view, q.back(n.unwrap_or(0))),
        }
    }

    /// Alternates in order; the first one carrying a value at `q` wins.
    fn ident(&mut self, name: &str, view: View, q: Quarter) -> Result<f64, Gap> {
        let bindings = self.bindings;
        let alts: &[Binding] = bindings.get(name).map_or(&[], Vec::as_slice);
        for b in alts {
            let found = match b {
                Binding::Literal(v) => Some(*v),
                Binding::Element { element, dims, abs } => {
                    self.element(element, dims, q, view).map(|v| {
                        // One place for the sign: an expense or outflow tag arrives
                        // negative in some periods and positive in others.
                        if *abs {
                            v.abs()
                        } else {
                            v
                        }
                    })
                }
                Binding::Kpi(id) => self.kpi(id, q, view)?,
                Binding::Metric(id) => self.metric(id, q)?,
            };
            if let Some(v) = found {
                self.inputs.push(Used {
                    ident: name.to_string(),
                    source: b.source(),
                    q,
                    view,
                    value: v,
                });
                return Ok(v);
            }
        }
        Err(Gap::Missing {
            ident: name.to_string(),
            q,
            view,
        })
    }

    fn element(&self, element: &str, dims: &str, q: Quarter, view: View) -> Option<f64> {
        match view {
            View::Avg4q => {
                let mut sum = 0.0;
                for i in 0..5 {
                    sum += self
                        .ctx
                        .source
                        .element(element, dims, q.back(i), View::Instant)?;
                }
                Some(sum / 5.0)
            }
            other => self.ctx.source.element(element, dims, q, other),
        }
    }

    /// KPI values are per quarter: `ttm` sums four, `avg4q` averages five, else the quarter's.
    fn kpi(&self, id: &str, q: Quarter, view: View) -> Result<Option<f64>, Gap> {
        let (pack, metric) = self
            .ctx
            .packs
            .resolve(self.pack, id)
            .filter(|(_, m)| m.layer == Layer::Kpi)
            .ok_or_else(|| Gap::Unresolved {
                reference: format!("kpi:{id}"),
            })?;
        let one = |qq: Quarter| self.ctx.source.kpi(&pack.id, &metric.id, qq);
        Ok(match view {
            View::Ttm => {
                let mut sum = 0.0;
                for i in 0..4 {
                    match one(q.back(i)) {
                        Some(v) => sum += v,
                        None => return Ok(None),
                    }
                }
                Some(sum)
            }
            View::Avg4q => {
                let mut sum = 0.0;
                for i in 0..5 {
                    match one(q.back(i)) {
                        Some(v) => sum += v,
                        None => return Ok(None),
                    }
                }
                Some(sum / 5.0)
            }
            View::Instant | View::Standalone => one(q),
        })
    }

    /// Another formula metric, evaluated with its own period at `q`.
    fn metric(&mut self, id: &str, q: Quarter) -> Result<Option<f64>, Gap> {
        if self.depth >= MAX_DEPTH {
            return Err(Gap::Cycle {
                metric: id.to_string(),
            });
        }
        let (pack, metric) =
            self.ctx
                .packs
                .resolve(self.pack, id)
                .ok_or_else(|| Gap::Unresolved {
                    reference: format!("metric:{id}"),
                })?;
        let (Some(formula), Some(period)) = (&metric.formula, metric.period) else {
            return Err(Gap::Unresolved {
                reference: format!("metric:{id}"),
            });
        };
        let mut inner = Run {
            ctx: self.ctx,
            pack,
            bindings: &formula.bindings,
            default: period.view(),
            inputs: Vec::new(),
            depth: self.depth + 1,
        };
        let out = inner.eval(&formula.expr, inner.default, q);
        self.inputs.append(&mut inner.inputs);
        match out {
            Ok(v) => Ok(Some(v)),
            Err(Gap::Missing { .. }) | Err(Gap::DivisionByZero) => Ok(None),
            Err(other) => Err(other),
        }
    }
}

/// Evaluates `formula` (a metric of `pack`, period `period`) at quarter `q`.
pub fn evaluate(
    ctx: &Ctx<'_>,
    pack: &Pack,
    formula: &Formula,
    period: Period,
    q: Quarter,
) -> Outcome {
    let mut run = Run {
        ctx,
        pack,
        bindings: &formula.bindings,
        default: period.view(),
        inputs: Vec::new(),
        depth: 0,
    };
    let result = run.eval(&formula.expr, run.default, q);
    let inputs = run.inputs;
    match result {
        Err(gap) => Outcome::gap(gap, inputs),
        Ok(v) if !v.is_finite() => Outcome::gap(Gap::DivisionByZero, inputs),
        Ok(v) => {
            // Units consistent: every element operand must be in the same currency.
            let mut units: BTreeSet<String> = BTreeSet::new();
            for u in &inputs {
                let element = u.source.split('[').next().unwrap_or("");
                if !element.contains(':')
                    || element.starts_with("kpi:")
                    || element.starts_with("metric:")
                {
                    continue;
                }
                if let Some(unit) = ctx.source.unit(element) {
                    if is_currency(&unit) {
                        units.insert(unit);
                    }
                }
            }
            if units.len() > 1 {
                return Outcome::gap(
                    Gap::UnitMismatch {
                        units: units.into_iter().collect(),
                    },
                    inputs,
                );
            }
            Outcome {
                value: Some(v),
                gap: None,
                inputs,
            }
        }
    }
}

fn is_currency(unit: &str) -> bool {
    unit.len() == 3 && unit.bytes().all(|b| b.is_ascii_uppercase())
}

/// Percentile rank (0 to 100) of `current` within `history`, or `None` with fewer than two
/// points. Ties count half, so a value equal to every other lands at 50.
pub fn percentile_rank(history: &[f64], current: f64) -> Option<f64> {
    if history.len() < 2 {
        return None;
    }
    let below = history.iter().filter(|v| **v < current).count() as f64;
    let equal = history.iter().filter(|v| **v == current).count() as f64;
    Some((below + 0.5 * equal) / history.len() as f64 * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_precedence_unary_and_functions() {
        let e = parse("-A + B * (C - 2) / lag(D, 3)").unwrap();
        assert_eq!(identifiers(&e).len(), 4);
        assert!(has_division(&e));
        assert!(!has_division(&parse("A + B").unwrap()));
        assert_eq!(parse("ttm(A").unwrap_err(), FormulaError::Eof);
        assert_eq!(
            parse("foo(A)").unwrap_err(),
            FormulaError::Function("foo".into())
        );
        assert_eq!(parse("lag(A)").unwrap_err(), FormulaError::LagArgs);
        assert_eq!(
            parse("ttm(A, 2)").unwrap_err(),
            FormulaError::Arity("ttm".into())
        );
        assert!(matches!(
            parse("A $ B").unwrap_err(),
            FormulaError::Char('$', 2)
        ));
        assert!(matches!(
            parse("A B").unwrap_err(),
            FormulaError::Token(_, 2)
        ));
    }

    #[test]
    fn denominator_is_the_right_side_of_the_outer_division() {
        let den = |src: &str| denominator(&parse(src).unwrap()).cloned();
        assert_eq!(den("A / ttm(B)"), parse("ttm(B)").ok());
        assert_eq!(den("100 * A / B"), parse("B").ok());
        assert_eq!(den("-A / (B - C)"), parse("B - C").ok());
        assert_eq!(den("A + B"), None);
    }

    #[test]
    fn quarter_arithmetic_wraps_years() {
        assert_eq!(Quarter::new(2026, 1).back(1), Quarter::new(2025, 4));
        assert_eq!(Quarter::new(2026, 2).back(4), Quarter::new(2025, 2));
        assert_eq!(Quarter::new(2026, 3).back(0), Quarter::new(2026, 3));
        assert_eq!(Quarter::new(2026, 3).back(11), Quarter::new(2023, 4));
    }

    #[test]
    fn percentile_rank_is_bounded_and_needs_history() {
        assert_eq!(percentile_rank(&[1.0], 1.0), None);
        assert_eq!(percentile_rank(&[1.0, 2.0, 3.0, 4.0], 4.0), Some(87.5));
        assert_eq!(percentile_rank(&[1.0, 2.0, 3.0, 4.0], 0.5), Some(0.0));
        assert_eq!(percentile_rank(&[2.0, 2.0], 2.0), Some(50.0));
    }
}
