//! `.xbrl` → contexts, units, numeric facts (research plan §4.2).
//!
//! The IDX instance documents put the XBRL instance elements (`context`, `unit`, `period`)
//! in the *default* namespace and the facts under `idx-cor:` / `idx-dei:` prefixes, so the
//! parser resolves namespaces and canonicalizes prefixes instead of matching text. Every
//! element with a `contextRef` whose text parses as a finite number becomes a `Fact`; text
//! blocks, nil elements and non-numeric values are counted and skipped. Dimensions from
//! `scenario` or `segment` become the canonical string `axis=member;axis=member`, sorted by
//! axis. `decimals` is kept as declared (`INF` → `None`).

use std::collections::{BTreeMap, HashMap};

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{Namespace, QName, ResolveResult};
use quick_xml::NsReader;

const NS_INSTANCE: &str = "http://www.xbrl.org/2003/instance";
const NS_LINKBASE: &str = "http://www.xbrl.org/2003/linkbase";
const NS_XBRLDI: &str = "http://xbrl.org/2006/xbrldi";
const NS_XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("not an XBRL instance: no contexts and no facts")]
    NotXbrl,
    #[error("fact {element} references unknown context {context}")]
    UnknownContext { element: String, context: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CtxKind {
    Duration,
    Instant,
}

impl CtxKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Duration => "duration",
            Self::Instant => "instant",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "duration" => Some(Self::Duration),
            "instant" => Some(Self::Instant),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub id: String,
    pub kind: CtxKind,
    /// `None` for instants.
    pub start: Option<String>,
    pub end: String,
    /// Canonical `axis=member;axis=member`, `""` when undimensioned.
    pub dims: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: String,
    /// `IDR`, `USD`, `IDR/shares`, `shares`, `pure`.
    pub measure: String,
}

/// One numeric fact with its context already resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    pub element: String,
    pub ctx_kind: CtxKind,
    pub start: Option<String>,
    pub end: String,
    pub dims: String,
    pub value: f64,
    pub unit: String,
    pub decimals: Option<i32>,
    /// Where a PDF-sourced fact was read (`xbrl/pdf.rs`); `None` for every XBRL fact.
    pub cite: Option<Cite>,
}

/// The page of the original PDF a fact was read from and the label as printed there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cite {
    /// 1-based PDF page, as a viewer numbers it (not the printed page number).
    pub page: u32,
    pub label: String,
}

/// `(element, ctx_kind, start, end, dims)`: the store's primary key within a filing.
pub type FactKey = (String, CtxKind, Option<String>, String, String);

impl Fact {
    pub fn key(&self) -> FactKey {
        (
            self.element.clone(),
            self.ctx_kind,
            self.start.clone(),
            self.end.clone(),
            self.dims.clone(),
        )
    }
}

/// Everything read from one instance document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub contexts: Vec<Context>,
    pub units: Vec<Unit>,
    pub facts: Vec<Fact>,
    /// `idx-dei` elements by local name, first occurrence, text as written.
    pub dei: BTreeMap<String, String>,
    /// Elements with a `contextRef` that were not numeric (text blocks, dates, names).
    pub skipped_non_numeric: usize,
    /// Elements with `xsi:nil="true"`: the taxonomy's unused placeholders.
    pub skipped_nil: usize,
    /// Same key seen twice with a different value; the first value is kept.
    pub conflicts: usize,
}

#[derive(Default)]
struct ContextBuilder {
    id: String,
    instant: Option<String>,
    start: Option<String>,
    end: Option<String>,
    dims: Vec<(String, String)>,
    /// Axis of the member element currently open.
    axis: Option<String>,
    typed: bool,
}

impl ContextBuilder {
    fn finish(mut self) -> Option<Context> {
        self.dims.sort();
        let dims = self
            .dims
            .iter()
            .map(|(a, m)| format!("{a}={m}"))
            .collect::<Vec<_>>()
            .join(";");
        match (self.instant, self.start, self.end) {
            (Some(i), _, _) => Some(Context {
                id: self.id,
                kind: CtxKind::Instant,
                start: None,
                end: i,
                dims,
            }),
            (None, Some(s), Some(e)) => Some(Context {
                id: self.id,
                kind: CtxKind::Duration,
                start: Some(s),
                end: e,
                dims,
            }),
            _ => None,
        }
    }
}

#[derive(Default)]
struct UnitBuilder {
    id: String,
    numerators: Vec<String>,
    denominators: Vec<String>,
    in_denominator: bool,
}

impl UnitBuilder {
    fn finish(self) -> Unit {
        let measure = match (self.numerators.is_empty(), self.denominators.is_empty()) {
            (true, _) => "pure".to_string(),
            (false, true) => self.numerators.join("*"),
            (false, false) => format!(
                "{}/{}",
                self.numerators.join("*"),
                self.denominators.join("*")
            ),
        };
        Unit {
            id: self.id,
            measure,
        }
    }
}

struct FactBuilder {
    element: String,
    is_dei: bool,
    context_ref: String,
    unit_ref: Option<String>,
    decimals: Option<i32>,
    nil: bool,
    text: String,
    /// Open nested elements (markup inside a text block).
    depth: usize,
    children: usize,
}

struct RawFact {
    element: String,
    context_ref: String,
    unit_ref: Option<String>,
    decimals: Option<i32>,
    value: f64,
}

enum Frame {
    Context(ContextBuilder),
    Unit(UnitBuilder),
    Fact(FactBuilder),
    Other,
}

struct Parser {
    doc: Document,
    raw: Vec<RawFact>,
    stack: Vec<Frame>,
    /// Local-name path inside the open context or unit, e.g. `["period", "startDate"]`.
    path: Vec<String>,
}

/// Namespace URI of a resolved name, owned so the reader can be borrowed again.
fn ns_uri(ns: &ResolveResult<'_>) -> Option<Vec<u8>> {
    match ns {
        ResolveResult::Bound(Namespace(u)) => Some(u.to_vec()),
        _ => None,
    }
}

fn is_ns(ns: Option<&[u8]>, uri: &str) -> bool {
    ns == Some(uri.as_bytes())
}

/// Canonical prefix for a namespace, so files that bind `idx-cor` to another prefix or use
/// the default namespace for the instance elements still yield the same element names.
fn canonical_prefix(ns: Option<&[u8]>, written: Option<&[u8]>) -> String {
    let written = || {
        written
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .unwrap_or_default()
    };
    let Some(uri) = ns else {
        return written();
    };
    let uri = String::from_utf8_lossy(uri);
    let trimmed = uri.trim_end_matches('/');
    match &*uri {
        NS_INSTANCE => "xbrli".into(),
        NS_LINKBASE => "link".into(),
        NS_XBRLDI => "xbrldi".into(),
        NS_XSI => "xsi".into(),
        _ if uri.contains("idx.co.id") && trimmed.ends_with("/dei") => "idx-dei".into(),
        _ if uri.contains("idx.co.id") && trimmed.ends_with("/cor") => "idx-cor".into(),
        _ => {
            let w = written();
            if w.is_empty() {
                "ns".into()
            } else {
                w
            }
        }
    }
}

fn local(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).into_owned()
}

fn written_prefix(e: &BytesStart<'_>) -> Option<Vec<u8>> {
    e.name().prefix().map(|p| p.into_inner().to_vec())
}

/// Canonical `prefix:Local` of a QName written in the document's own prefixes.
fn qname_of(reader: &NsReader<&[u8]>, raw: &str) -> String {
    let raw = raw.trim();
    let q = QName(raw.as_bytes());
    let (ns, local) = reader.resolve_element(q);
    let ns = ns_uri(&ns);
    let prefix = canonical_prefix(
        ns.as_deref(),
        q.prefix().map(|p| p.into_inner().to_vec()).as_deref(),
    );
    let local = String::from_utf8_lossy(local.as_ref());
    if prefix.is_empty() {
        local.into_owned()
    } else {
        format!("{prefix}:{local}")
    }
}

fn attr_value(a: &Attribute<'_>) -> String {
    a.unescape_value()
        .map(|v| v.into_owned())
        .unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned())
}

fn attr(e: &BytesStart<'_>, local: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == local)
        .map(|a| attr_value(&a))
}

/// `INF` → `None` (exact). Clamped to ±30 so a nonsense value cannot overflow `10^(−d)`.
fn parse_decimals(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("INF") {
        return None;
    }
    s.parse::<i32>().ok().map(|d| d.clamp(-30, 30))
}

/// Accepts plain decimal numbers (with an optional exponent); rejects words like `NaN`.
pub fn parse_number(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || !s.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E'))
    {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn measure_name(text: &str) -> String {
    let t = text.trim();
    t.rsplit(':').next().unwrap_or(t).to_string()
}

impl Parser {
    fn open(&mut self, reader: &NsReader<&[u8]>, ns: Option<&[u8]>, e: &BytesStart<'_>) {
        if let Some(Frame::Fact(f)) = self.stack.last_mut() {
            // Markup inside a fact: a text block, never a number.
            f.depth += 1;
            f.children += 1;
            return;
        }
        let name = local(e);
        let instance = is_ns(ns, NS_INSTANCE);
        match self.stack.last_mut() {
            Some(Frame::Context(c)) => {
                self.path.push(name.clone());
                if is_ns(ns, NS_XBRLDI) && (name == "explicitMember" || name == "typedMember") {
                    c.axis = attr(e, b"dimension").map(|a| qname_of(reader, &a));
                    c.typed = name == "typedMember";
                }
            }
            Some(Frame::Unit(u)) => {
                self.path.push(name.clone());
                if name == "unitDenominator" {
                    u.in_denominator = true;
                }
            }
            _ if instance && name == "context" => {
                self.path.clear();
                self.stack.push(Frame::Context(ContextBuilder {
                    id: attr(e, b"id").unwrap_or_default(),
                    ..Default::default()
                }));
            }
            _ if instance && name == "unit" => {
                self.path.clear();
                self.stack.push(Frame::Unit(UnitBuilder {
                    id: attr(e, b"id").unwrap_or_default(),
                    ..Default::default()
                }));
            }
            _ => {
                let structural = instance || is_ns(ns, NS_LINKBASE) || is_ns(ns, NS_XBRLDI);
                match attr(e, b"contextRef") {
                    Some(context_ref) if !structural => {
                        let prefix = canonical_prefix(ns, written_prefix(e).as_deref());
                        let element = if prefix.is_empty() {
                            name
                        } else {
                            format!("{prefix}:{name}")
                        };
                        self.stack.push(Frame::Fact(FactBuilder {
                            is_dei: prefix == "idx-dei",
                            element,
                            context_ref,
                            unit_ref: attr(e, b"unitRef"),
                            decimals: attr(e, b"decimals").and_then(|d| parse_decimals(&d)),
                            nil: attr(e, b"nil")
                                .is_some_and(|v| v.trim().eq_ignore_ascii_case("true")),
                            text: String::new(),
                            depth: 0,
                            children: 0,
                        }));
                    }
                    _ => self.stack.push(Frame::Other),
                }
            }
        }
    }

    fn text(&mut self, reader: &NsReader<&[u8]>, text: &str) {
        match self.stack.last_mut() {
            Some(Frame::Fact(f)) => {
                if f.depth == 0 {
                    f.text.push_str(text);
                }
            }
            Some(Frame::Context(c)) => match self.path.last().map(String::as_str) {
                Some("instant") => c.instant = Some(text.trim().to_string()),
                Some("startDate") => c.start = Some(text.trim().to_string()),
                Some("endDate") => c.end = Some(text.trim().to_string()),
                Some(_) => {
                    if let Some(axis) = c.axis.take() {
                        let member = if c.typed {
                            format!("typed:{}", text.trim())
                        } else {
                            qname_of(reader, text)
                        };
                        c.dims.push((axis, member));
                    }
                }
                None => {}
            },
            Some(Frame::Unit(u)) if self.path.last().map(String::as_str) == Some("measure") => {
                let m = measure_name(text);
                if u.in_denominator {
                    u.denominators.push(m);
                } else {
                    u.numerators.push(m);
                }
            }
            _ => {}
        }
    }

    fn end(&mut self) {
        match self.stack.last_mut() {
            Some(Frame::Fact(f)) if f.depth > 0 => {
                f.depth -= 1;
                return;
            }
            Some(Frame::Context(c)) if !self.path.is_empty() => {
                let left = self.path.pop();
                if left.as_deref() == Some("explicitMember")
                    || left.as_deref() == Some("typedMember")
                {
                    c.axis = None;
                }
                return;
            }
            Some(Frame::Unit(u)) if !self.path.is_empty() => {
                if self.path.pop().as_deref() == Some("unitDenominator") {
                    u.in_denominator = false;
                }
                return;
            }
            _ => {}
        }
        match self.stack.pop() {
            Some(Frame::Context(c)) => {
                self.path.clear();
                if let Some(ctx) = c.finish() {
                    self.doc.contexts.push(ctx);
                }
            }
            Some(Frame::Unit(u)) => {
                self.path.clear();
                self.doc.units.push(u.finish());
            }
            Some(Frame::Fact(f)) => {
                if f.nil {
                    self.doc.skipped_nil += 1;
                    return;
                }
                if f.is_dei {
                    let key = f
                        .element
                        .rsplit(':')
                        .next()
                        .unwrap_or(&f.element)
                        .to_string();
                    self.doc
                        .dei
                        .entry(key)
                        .or_insert_with(|| f.text.trim().to_string());
                }
                if f.children > 0 {
                    self.doc.skipped_non_numeric += 1;
                    return;
                }
                match parse_number(&f.text) {
                    Some(value) => self.raw.push(RawFact {
                        element: f.element,
                        context_ref: f.context_ref,
                        unit_ref: f.unit_ref,
                        decimals: f.decimals,
                        value,
                    }),
                    None => self.doc.skipped_non_numeric += 1,
                }
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Result<Document, ParseError> {
        if self.doc.contexts.is_empty() && self.raw.is_empty() {
            return Err(ParseError::NotXbrl);
        }
        let contexts: HashMap<&str, &Context> = self
            .doc
            .contexts
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect();
        let units: HashMap<&str, &str> = self
            .doc
            .units
            .iter()
            .map(|u| (u.id.as_str(), u.measure.as_str()))
            .collect();
        let mut seen: HashMap<FactKey, f64> = HashMap::new();
        let mut facts = Vec::with_capacity(self.raw.len());
        let mut conflicts = 0;
        for r in self.raw.drain(..) {
            let ctx =
                contexts
                    .get(r.context_ref.as_str())
                    .ok_or_else(|| ParseError::UnknownContext {
                        element: r.element.clone(),
                        context: r.context_ref.clone(),
                    })?;
            let unit = r
                .unit_ref
                .as_deref()
                .and_then(|u| units.get(u).copied())
                .unwrap_or("pure")
                .to_string();
            let fact = Fact {
                element: r.element,
                ctx_kind: ctx.kind,
                start: ctx.start.clone(),
                end: ctx.end.clone(),
                dims: ctx.dims.clone(),
                value: r.value,
                unit,
                decimals: r.decimals,
                cite: None,
            };
            match seen.get(&fact.key()) {
                Some(v) if *v == fact.value => {}
                Some(_) => conflicts += 1,
                None => {
                    seen.insert(fact.key(), fact.value);
                    facts.push(fact);
                }
            }
        }
        self.doc.facts = facts;
        self.doc.conflicts = conflicts;
        Ok(self.doc)
    }
}

/// Parses one instance document held in memory.
pub fn parse_str(xml: &str) -> Result<Document, ParseError> {
    let mut reader = NsReader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut p = Parser {
        doc: Document::default(),
        raw: Vec::new(),
        stack: Vec::new(),
        path: Vec::new(),
    };
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let ns = ns_uri(&ns);
        match event {
            Event::Start(e) => p.open(&reader, ns.as_deref(), &e),
            Event::Empty(e) => {
                p.open(&reader, ns.as_deref(), &e);
                p.end();
            }
            Event::Text(t) => {
                let text = t.unescape().map(|c| c.into_owned()).unwrap_or_default();
                p.text(&reader, &text);
            }
            Event::CData(c) => p.text(&reader, &String::from_utf8_lossy(c.as_ref())),
            Event::End(_) => p.end(),
            Event::Eof => break,
            _ => {}
        }
    }
    p.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<?xml version="1.0"?>
<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:link="http://www.xbrl.org/2003/linkbase"
  xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:iso4217="http://www.xbrl.org/2003/iso4217"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xbrldi="http://xbrl.org/2006/xbrldi"
  xmlns:cor="http://www.idx.co.id/xbrl/taxonomy/2020-01-01/cor"
  xmlns:dei="http://www.idx.co.id/xbrl/taxonomy/2020-01-01/dei">
<link:schemaRef xlink:type="simple" xlink:href="Taxonomy.xsd"/>
<unit id="IDR"><measure>iso4217:IDR</measure></unit>
<unit id="IDRPerShares"><divide><unitNumerator><measure>iso4217:IDR</measure></unitNumerator><unitDenominator><measure>shares</measure></unitDenominator></divide></unit>
<unit id="Shares"><measure>shares</measure></unit>
<context id="CurrentYearInstant"><entity><identifier scheme="s">x</identifier></entity><period><instant>2025-06-30</instant></period></context>
<context id="CurrentYearDuration"><entity><identifier scheme="s">x</identifier></entity><period><startDate>2025-01-01</startDate><endDate>2025-06-30</endDate></period></context>
<context id="Dim"><entity><identifier scheme="s">x</identifier></entity><period><instant>2025-06-30</instant></period>
  <scenario><xbrldi:explicitMember dimension="cor:ZAxis">cor:BMember</xbrldi:explicitMember><xbrldi:explicitMember dimension="cor:AAxis">cor:CMember</xbrldi:explicitMember></scenario></context>
<dei:EntityCode contextRef="CurrentYearInstant">AAAA</dei:EntityCode>
<dei:NumberOfYearsServedAsAuditSigningPartner contextRef="CurrentYearInstant">3</dei:NumberOfYearsServedAsAuditSigningPartner>
<cor:Cash id="a" decimals="-6" contextRef="CurrentYearInstant" unitRef="IDR">125000000</cor:Cash>
<cor:Cash id="b" decimals="-6" contextRef="CurrentYearInstant" unitRef="IDR">125000000</cor:Cash>
<cor:Cash id="c" decimals="-6" contextRef="CurrentYearInstant" unitRef="IDR">125000001</cor:Cash>
<cor:Equity decimals="-6" contextRef="Dim" unitRef="IDR">-5000000</cor:Equity>
<cor:SalesAndRevenue decimals="INF" contextRef="CurrentYearDuration" unitRef="IDR">7000000</cor:SalesAndRevenue>
<cor:BasicEarningsLossPerShare decimals="2" contextRef="CurrentYearDuration" unitRef="IDRPerShares">12.5</cor:BasicEarningsLossPerShare>
<cor:NumberOfShares decimals="0" contextRef="CurrentYearInstant" unitRef="Shares">1000</cor:NumberOfShares>
<cor:NilThing xsi:nil="true" contextRef="CurrentYearInstant" unitRef="IDR"/>
<cor:NotesTextBlock contextRef="CurrentYearDuration">&lt;p&gt;Segment &lt;b&gt;text&lt;/b&gt; 436&lt;/p&gt;</cor:NotesTextBlock>
<cor:PartyName contextRef="CurrentYearDuration">PT Contoh</cor:PartyName>
<cor:Markup contextRef="CurrentYearDuration"><b>12</b></cor:Markup>
</xbrl>"#;

    #[test]
    fn contexts_units_and_dimensions_are_canonical() {
        let d = parse_str(DOC).unwrap();
        assert_eq!(d.contexts.len(), 3);
        let dim = d.contexts.iter().find(|c| c.id == "Dim").unwrap();
        assert_eq!(dim.kind, CtxKind::Instant);
        assert_eq!(
            dim.dims,
            "idx-cor:AAxis=idx-cor:CMember;idx-cor:ZAxis=idx-cor:BMember"
        );
        let dur = d
            .contexts
            .iter()
            .find(|c| c.id == "CurrentYearDuration")
            .unwrap();
        assert_eq!(dur.start.as_deref(), Some("2025-01-01"));
        assert_eq!(dur.end, "2025-06-30");
        let units: Vec<_> = d.units.iter().map(|u| u.measure.as_str()).collect();
        assert_eq!(units, ["IDR", "IDR/shares", "shares"]);
    }

    #[test]
    fn numeric_facts_only_with_canonical_prefix_and_decimals() {
        let d = parse_str(DOC).unwrap();
        let names: Vec<_> = d.facts.iter().map(|f| f.element.as_str()).collect();
        assert_eq!(
            names,
            [
                "idx-dei:NumberOfYearsServedAsAuditSigningPartner",
                "idx-cor:Cash",
                "idx-cor:Equity",
                "idx-cor:SalesAndRevenue",
                "idx-cor:BasicEarningsLossPerShare",
                "idx-cor:NumberOfShares",
            ]
        );
        let cash = &d.facts[1];
        assert_eq!(cash.value, 125_000_000.0);
        assert_eq!(cash.decimals, Some(-6));
        assert_eq!(cash.unit, "IDR");
        assert_eq!(cash.ctx_kind, CtxKind::Instant);
        assert_eq!(cash.end, "2025-06-30");
        let rev = &d.facts[3];
        assert_eq!(rev.decimals, None, "INF decimals are kept as None");
        assert_eq!(rev.start.as_deref(), Some("2025-01-01"));
        let eps = &d.facts[4];
        assert_eq!(
            (eps.unit.as_str(), eps.value, eps.decimals),
            ("IDR/shares", 12.5, Some(2))
        );
        assert_eq!(d.facts[0].unit, "pure");
        assert_eq!(
            d.facts[2].dims,
            "idx-cor:AAxis=idx-cor:CMember;idx-cor:ZAxis=idx-cor:BMember"
        );
        // One exact duplicate (dropped silently) and one conflicting duplicate (counted).
        assert_eq!(d.conflicts, 1);
        // text block, name, markup → skipped and counted; dei text too; nil counted apart.
        assert_eq!(d.skipped_non_numeric, 4);
        assert_eq!(d.skipped_nil, 1);
    }

    #[test]
    fn dei_header_is_collected_as_text() {
        let d = parse_str(DOC).unwrap();
        assert_eq!(d.dei.get("EntityCode").map(String::as_str), Some("AAAA"));
        assert_eq!(
            d.dei
                .get("NumberOfYearsServedAsAuditSigningPartner")
                .map(String::as_str),
            Some("3")
        );
    }

    #[test]
    fn prefixed_instance_elements_and_segment_dimensions_also_parse() {
        let xml = r#"<xbrli:xbrl xmlns:xbrli="http://www.xbrl.org/2003/instance" xmlns:xbrldi="http://xbrl.org/2006/xbrldi" xmlns:idx-cor="http://www.idx.co.id/xbrl/taxonomy/2020-01-01/cor" xmlns:iso4217="http://www.xbrl.org/2003/iso4217">
<xbrli:unit id="u"><xbrli:measure>iso4217:USD</xbrli:measure></xbrli:unit>
<xbrli:context id="c"><xbrli:entity><xbrli:identifier scheme="s">x</xbrli:identifier>
<xbrli:segment><xbrldi:explicitMember dimension="idx-cor:Axis">idx-cor:M</xbrldi:explicitMember></xbrli:segment></xbrli:entity>
<xbrli:period><xbrli:startDate>2024-01-01</xbrli:startDate><xbrli:endDate>2024-12-31</xbrli:endDate></xbrli:period></xbrli:context>
<idx-cor:Revenue contextRef="c" unitRef="u" decimals="0">42</idx-cor:Revenue>
</xbrli:xbrl>"#;
        let d = parse_str(xml).unwrap();
        assert_eq!(d.contexts[0].dims, "idx-cor:Axis=idx-cor:M");
        assert_eq!(d.facts[0].element, "idx-cor:Revenue");
        assert_eq!(d.facts[0].unit, "USD");
        assert_eq!(d.facts[0].ctx_kind, CtxKind::Duration);
    }

    #[test]
    fn unknown_context_and_non_xbrl_are_errors() {
        let xml = r#"<xbrl xmlns="http://www.xbrl.org/2003/instance" xmlns:c="http://www.idx.co.id/xbrl/taxonomy/2020-01-01/cor">
<context id="c"><entity><identifier scheme="s">x</identifier></entity><period><instant>2024-12-31</instant></period></context>
<c:Cash contextRef="nope" decimals="0">1</c:Cash></xbrl>"#;
        assert!(matches!(
            parse_str(xml),
            Err(ParseError::UnknownContext { .. })
        ));
        assert!(matches!(
            parse_str("<html><body>not xbrl</body></html>"),
            Err(ParseError::NotXbrl)
        ));
        assert!(matches!(
            parse_str("<xbrl><a></b></xbrl>"),
            Err(ParseError::Xml(_))
        ));
    }

    #[test]
    fn number_parsing_is_strict() {
        assert_eq!(parse_number(" -12.5 "), Some(-12.5));
        assert_eq!(parse_number("1e3"), Some(1000.0));
        assert_eq!(parse_number("NaN"), None);
        assert_eq!(parse_number("12,5"), None);
        assert_eq!(parse_number("2024-01-01"), None);
        assert_eq!(parse_number(""), None);
    }
}
