//! Pack definitions (research plan §4.3, D6): the TOML schema, parsing and validation.
//!
//! A pack is versioned data: id, version, label, the IDX subindustry codes it applies to by
//! default, dimensions with metrics, and the external driver series it wants. Every metric
//! declares its layer (`xbrl` formula over facts, `kpi` entered by hand, `driver` external
//! series), unit, direction, an Indonesian `why`, and for formulas the period view and the
//! bindings of each identifier to element alternates, KPI values, other metrics or a literal.
//!
//! Validation happens at load (`Pack::parse`), never at request time: every identifier bound,
//! units from the vocabulary and consistent with the formula shape, `why` present, the five
//! dimension kinds present (industry packs), no cycle through `metric:` bindings.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

use super::eval::{self, Expr};
use crate::research::prices::drivers as drivers_catalogue;

/// Source layer of a metric (design.md §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Xbrl,
    Kpi,
    Driver,
}

impl Layer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xbrl => "xbrl",
            Self::Kpi => "kpi",
            Self::Driver => "driver",
        }
    }
}

/// Which way "better" points; `neutral` for mixes and levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Higher,
    Lower,
    Neutral,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Higher => "higher",
            Self::Lower => "lower",
            Self::Neutral => "neutral",
        }
    }
}

/// How a bare identifier in a formula is read (plan §4.3 `period`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    Instant,
    Standalone,
    Ttm,
    Average4q,
}

impl Period {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instant => "instant",
            Self::Standalone => "standalone",
            Self::Ttm => "ttm",
            Self::Average4q => "average4q",
        }
    }

    pub fn view(self) -> eval::View {
        match self {
            Self::Instant => eval::View::Instant,
            Self::Standalone => eval::View::Standalone,
            Self::Ttm => eval::View::Ttm,
            Self::Average4q => eval::View::Avg4q,
        }
    }
}

/// How often a KPI is expected (plan §4.3 `cadence`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cadence {
    Quarterly,
    Annual,
}

impl Cadence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quarterly => "quarterly",
            Self::Annual => "annual",
        }
    }
}

/// The five content dimensions every industry pack must cover (prd.md §6.1 item 3), plus
/// `core` for the pack that is always applied. ASUMSI (R2.1): the canon names the five as
/// "volume or unit economics, pricing, cost structure, asset quality or efficiency,
/// balance-sheet specifics"; each `[[dimension]]` declares which one it serves, so a pack
/// may have more than five dimensions as long as all five kinds appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DimensionKind {
    Volume,
    Pricing,
    Cost,
    Quality,
    Balance,
    Core,
}

impl DimensionKind {
    pub const REQUIRED: [DimensionKind; 5] = [
        Self::Volume,
        Self::Pricing,
        Self::Cost,
        Self::Quality,
        Self::Balance,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Volume => "volume",
            Self::Pricing => "pricing",
            Self::Cost => "cost",
            Self::Quality => "quality",
            Self::Balance => "balance",
            Self::Core => "core",
        }
    }
}

/// Units a metric may declare. `ccy` is the filing's presentation currency; `pp` is
/// percentage points (the difference of two percentages).
/// Limits of `extract_aliases` per metric (R8).
const MAX_EXTRACT_ALIASES: usize = 8;
const MAX_ALIAS_CHARS: usize = 60;

pub const UNITS: &[&str] = &[
    "%",
    "pp",
    "x",
    "ccy",
    "IDR miliar",
    "USD juta",
    "USD/t",
    "USD/oz",
    "USD/hari",
    "Mt",
    "Mbcm",
    "juta DWT",
    "bcm/t",
    "oz",
    "unit",
    "kapal",
    "tahun",
    "hari",
];

/// Where one identifier of a formula gets its value, tried in order per period.
#[derive(Clone, Debug, PartialEq)]
pub enum Binding {
    /// A tagged fact: element name plus canonical `axis=member;axis=member` dims (`""` for
    /// undimensioned facts). `abs` (written `abs:idx-cor:Name`) reads the fact as a magnitude,
    /// for expense and outflow tags that filings report with either sign.
    Element {
        element: String,
        dims: String,
        abs: bool,
    },
    /// A KPI metric of the same pack (or core).
    Kpi(String),
    /// Another formula metric of the same pack (or core), evaluated with its own period.
    Metric(String),
    /// A declared constant, visible in the pack file ("treat as zero when not reported").
    Literal(f64),
}

impl Binding {
    /// The element or reference text without the `abs:` marker, which is a reading rule and
    /// not part of the identity (payloads and unit checks key on this).
    pub fn source(&self) -> String {
        match self {
            Self::Element { element, dims, .. } if dims.is_empty() => element.clone(),
            Self::Element { element, dims, .. } => format!("{element}[{dims}]"),
            Self::Kpi(id) => format!("kpi:{id}"),
            Self::Metric(id) => format!("metric:{id}"),
            Self::Literal(v) => v.to_string(),
        }
    }

    /// Parses one alternate. Element dims are sorted by axis so they match the store's
    /// canonical form regardless of the order written.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty binding".into());
        }
        if let Ok(v) = s.parse::<f64>() {
            return v
                .is_finite()
                .then_some(Self::Literal(v))
                .ok_or_else(|| format!("literal `{s}` is not finite"));
        }
        let (abs, s) = match s.strip_prefix("abs:") {
            Some(rest) => (true, rest.trim()),
            None => (false, s),
        };
        if abs && (s.starts_with("kpi:") || s.starts_with("metric:")) {
            return Err(format!("`abs:` applies to tagged facts only, not `{s}`"));
        }
        if let Some(id) = s.strip_prefix("kpi:") {
            return reference(id).map(|_| Self::Kpi(id.to_string()));
        }
        if let Some(id) = s.strip_prefix("metric:") {
            return reference(id).map(|_| Self::Metric(id.to_string()));
        }
        let (element, dims) = match s.split_once('[') {
            Some((e, rest)) => {
                let inner = rest
                    .strip_suffix(']')
                    .ok_or_else(|| format!("`{s}`: unterminated dims"))?;
                let mut pairs: Vec<(String, String)> = Vec::new();
                for p in inner.split(';').map(str::trim).filter(|p| !p.is_empty()) {
                    let (axis, member) = p
                        .split_once('=')
                        .ok_or_else(|| format!("`{s}`: dims must be axis=member"))?;
                    pairs.push((axis.trim().to_string(), member.trim().to_string()));
                }
                pairs.sort();
                let dims = pairs
                    .iter()
                    .map(|(a, m)| format!("{a}={m}"))
                    .collect::<Vec<_>>()
                    .join(";");
                (e.trim(), dims)
            }
            None => (s, String::new()),
        };
        let ok = element.split_once(':').is_some_and(|(p, l)| {
            !p.is_empty() && !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric())
        });
        if !ok {
            return Err(format!("`{element}` is not a prefixed element name"));
        }
        Ok(Self::Element {
            element: element.to_string(),
            dims,
            abs,
        })
    }
}

/// A metric id of the same pack, or `core.<id>` for the core pack.
fn reference(s: &str) -> Result<(), String> {
    slug(s.strip_prefix("core.").unwrap_or(s))
}

fn slug(s: &str) -> Result<(), String> {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        Ok(())
    } else {
        Err(format!("`{s}` is not a lowercase identifier"))
    }
}

/// A formula with its parsed expression and bindings.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub text: String,
    pub expr: Expr,
    pub bindings: BTreeMap<String, Vec<Binding>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    pub id: String,
    pub label: String,
    pub layer: Layer,
    pub unit: String,
    pub direction: Direction,
    pub why: String,
    pub note: Option<String>,
    /// Formula metrics only.
    pub formula: Option<Formula>,
    pub period: Option<Period>,
    /// KPI metrics only.
    pub cadence: Option<Cadence>,
    /// Driver metrics only: the `[[driver]]` id.
    pub driver: Option<String>,
    /// Values outside `[lo, hi]` are flagged, never hidden ("validate scale").
    pub range: Option<(f64, f64)>,
    /// KPI metrics only: other names the document extractor looks for besides `label`
    /// (R8, e.g. the English wording of a presentation). Never shown to the owner.
    pub extract_aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dimension {
    pub id: String,
    pub label: String,
    pub kind: DimensionKind,
    pub metrics: Vec<Metric>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Driver {
    pub id: String,
    pub label: String,
    /// `TBD` when no free source exists (R3.3 spike); otherwise the catalogue entry of the
    /// same id in `prices::drivers` fetches the series.
    pub source: String,
    /// A catalogue series that stands in on the chart when the driver itself has no source
    /// (`brent` for bunker, `bwet` for the charter index); always labelled as a proxy.
    pub proxy: Option<String>,
    /// The pack metric this driver is charted against per standalone quarter (`newcastle`
    /// against `asp_usd_t`); validated at load to name a metric of the same pack.
    pub compare_metric: Option<String>,
}

impl Driver {
    pub fn has_source(&self) -> bool {
        !self.source.trim().is_empty() && !self.source.trim().eq_ignore_ascii_case("tbd")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub id: String,
    pub version: String,
    pub label: String,
    pub industry_codes: Vec<String>,
    pub taxonomies: Vec<String>,
    pub dimensions: Vec<Dimension>,
    pub drivers: Vec<Driver>,
}

/// The id of the pack that is always applied.
pub const CORE_ID: &str = "core";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub Vec<String>);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("; "))
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid pack: {0}")]
    Invalid(#[from] ValidationError),
}

// ---- TOML shapes (private; validated into the public types above) ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    id: String,
    version: String,
    label: String,
    #[serde(default)]
    industry_codes: Vec<String>,
    #[serde(default)]
    taxonomies: Vec<String>,
    #[serde(default, rename = "dimension")]
    dimensions: Vec<DimensionFile>,
    #[serde(default, rename = "driver")]
    drivers: Vec<DriverFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DimensionFile {
    id: String,
    label: String,
    kind: DimensionKind,
    #[serde(default, rename = "metric")]
    metrics: Vec<MetricFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricFile {
    id: String,
    label: String,
    layer: Layer,
    unit: String,
    direction: Direction,
    #[serde(default)]
    why: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    formula: Option<String>,
    #[serde(default)]
    elements: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    period: Option<Period>,
    #[serde(default)]
    cadence: Option<Cadence>,
    #[serde(default)]
    driver: Option<String>,
    #[serde(default)]
    range: Option<[f64; 2]>,
    #[serde(default)]
    extract_aliases: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriverFile {
    id: String,
    label: String,
    #[serde(default = "tbd")]
    source: String,
    #[serde(default)]
    proxy: Option<String>,
    #[serde(default)]
    compare_metric: Option<String>,
}

fn tbd() -> String {
    "TBD".into()
}

impl Pack {
    /// Parses and validates one pack file. `core` is exempt from the five-kind rule.
    pub fn parse(text: &str) -> Result<Self, PackError> {
        let file: PackFile = toml::from_str(text)?;
        let mut errors: Vec<String> = Vec::new();
        let mut err = |m: String| errors.push(m);

        if let Err(e) = slug(&file.id) {
            err(format!("id: {e}"));
        }
        if file.version.trim().is_empty() {
            err("version is empty".into());
        }
        if file.label.trim().is_empty() {
            err("label is empty".into());
        }
        for c in &file.industry_codes {
            if c.is_empty() || !c.bytes().all(|b| b.is_ascii_alphanumeric()) {
                err(format!("industry code `{c}` is not alphanumeric"));
            }
        }
        if file.dimensions.is_empty() {
            err("no dimensions".into());
        }

        let mut drivers = Vec::new();
        let mut driver_ids = BTreeSet::new();
        for d in &file.drivers {
            if let Err(e) = slug(&d.id) {
                err(format!("driver: {e}"));
            }
            if !driver_ids.insert(d.id.clone()) {
                err(format!("driver `{}` declared twice", d.id));
            }
            if d.label.trim().is_empty() {
                err(format!("driver `{}`: label is empty", d.id));
            }
            let driver = Driver {
                id: d.id.clone(),
                label: d.label.clone(),
                source: d.source.clone(),
                proxy: d.proxy.clone(),
                compare_metric: d.compare_metric.clone(),
            };
            if driver.has_source() && drivers_catalogue::spec(&d.id).is_none() {
                err(format!(
                    "driver `{}`: has a source but no adapter in the driver catalogue",
                    d.id
                ));
            }
            if let Some(p) = &d.proxy {
                match drivers_catalogue::spec(p) {
                    Some(s) if s.has_source() => {}
                    _ => err(format!(
                        "driver `{}`: proxy `{p}` is not a sourced catalogue series",
                        d.id
                    )),
                }
            }
            drivers.push(driver);
        }

        let mut dimensions = Vec::new();
        let mut dim_ids = BTreeSet::new();
        let mut metric_ids = BTreeSet::new();
        let mut kinds = BTreeSet::new();
        for d in &file.dimensions {
            if let Err(e) = slug(&d.id) {
                err(format!("dimension: {e}"));
            }
            if !dim_ids.insert(d.id.clone()) {
                err(format!("dimension `{}` declared twice", d.id));
            }
            if d.label.trim().is_empty() {
                err(format!("dimension `{}`: label is empty", d.id));
            }
            if d.metrics.is_empty() {
                err(format!("dimension `{}` has no metrics", d.id));
            }
            kinds.insert(d.kind);
            let mut metrics = Vec::new();
            for m in &d.metrics {
                let at = format!("metric `{}`", m.id);
                if let Err(e) = slug(&m.id) {
                    err(format!("{at}: {e}"));
                }
                if !metric_ids.insert(m.id.clone()) {
                    err(format!("{at} declared twice"));
                }
                if m.label.trim().is_empty() {
                    err(format!("{at}: label is empty"));
                }
                if m.why.trim().is_empty() {
                    err(format!("{at}: `why` is missing"));
                }
                if !UNITS.contains(&m.unit.as_str()) {
                    err(format!("{at}: unit `{}` is not in the vocabulary", m.unit));
                }
                let range = match m.range {
                    Some([lo, hi]) if lo < hi => Some((lo, hi)),
                    Some(_) => {
                        err(format!("{at}: range must be [lo, hi] with lo < hi"));
                        None
                    }
                    None => None,
                };
                let extract_aliases: Vec<String> = m
                    .extract_aliases
                    .iter()
                    .map(|a| a.split_whitespace().collect::<Vec<_>>().join(" "))
                    .collect();
                if !extract_aliases.is_empty() && m.layer != Layer::Kpi {
                    err(format!("{at}: `extract_aliases` belongs to kpi metrics"));
                }
                if extract_aliases.len() > MAX_EXTRACT_ALIASES {
                    err(format!(
                        "{at}: at most {MAX_EXTRACT_ALIASES} extract_aliases"
                    ));
                }
                for a in &extract_aliases {
                    let len = a.chars().count();
                    if !(3..=MAX_ALIAS_CHARS).contains(&len) {
                        err(format!(
                            "{at}: extract alias `{a}` must be 3 to {MAX_ALIAS_CHARS} characters"
                        ));
                    }
                    if a.chars().any(char::is_control) {
                        err(format!("{at}: extract alias `{a}` has a control character"));
                    }
                }
                let mut formula = None;
                match m.layer {
                    Layer::Xbrl => {
                        if m.period.is_none() {
                            err(format!("{at}: formula metrics need a `period`"));
                        }
                        if m.cadence.is_some() {
                            err(format!("{at}: `cadence` belongs to kpi metrics"));
                        }
                        match m.formula.as_deref().map(str::trim) {
                            None | Some("") => err(format!("{at}: formula is missing")),
                            Some(text) => match eval::parse(text) {
                                Err(e) => err(format!("{at}: formula: {e}")),
                                Ok(expr) => {
                                    let mut bindings = BTreeMap::new();
                                    for (ident, alts) in &m.elements {
                                        if alts.is_empty() {
                                            err(format!("{at}: `{ident}` has no alternates"));
                                        }
                                        let mut parsed = Vec::new();
                                        for a in alts {
                                            match Binding::parse(a) {
                                                Ok(b) => parsed.push(b),
                                                Err(e) => err(format!("{at}: `{ident}`: {e}")),
                                            }
                                        }
                                        bindings.insert(ident.clone(), parsed);
                                    }
                                    for ident in eval::identifiers(&expr) {
                                        if !bindings.contains_key(&ident) {
                                            err(format!("{at}: identifier `{ident}` is not bound"));
                                        }
                                    }
                                    for ident in bindings.keys() {
                                        if !eval::identifiers(&expr).contains(ident) {
                                            err(format!("{at}: binding `{ident}` is not used by the formula"));
                                        }
                                    }
                                    // Units consistent with the formula shape: a ratio unit
                                    // needs a division, unless the fact is itself a ratio
                                    // tag and declares the `range` that validates its scale.
                                    let ratio = matches!(m.unit.as_str(), "%" | "x");
                                    if ratio && !eval::has_division(&expr) && range.is_none() {
                                        err(format!("{at}: unit `{}` but the formula is not a ratio (divide, or declare `range` for a ratio tag)", m.unit));
                                    }
                                    formula = Some(Formula {
                                        text: text.to_string(),
                                        expr,
                                        bindings,
                                    });
                                }
                            },
                        }
                    }
                    Layer::Kpi => {
                        if m.cadence.is_none() {
                            err(format!("{at}: kpi metrics need a `cadence`"));
                        }
                        if m.formula.is_some() || !m.elements.is_empty() {
                            err(format!("{at}: kpi metrics carry no formula"));
                        }
                    }
                    Layer::Driver => {
                        match &m.driver {
                            Some(id) if driver_ids.contains(id) => {}
                            Some(id) => err(format!("{at}: driver `{id}` is not declared")),
                            None => err(format!("{at}: driver metrics need `driver`")),
                        }
                        if m.formula.is_some() {
                            err(format!("{at}: driver metrics carry no formula"));
                        }
                    }
                }
                metrics.push(Metric {
                    id: m.id.clone(),
                    label: m.label.clone(),
                    layer: m.layer,
                    unit: m.unit.clone(),
                    direction: m.direction,
                    why: m.why.trim().to_string(),
                    note: m.note.clone(),
                    formula,
                    period: m.period,
                    cadence: m.cadence,
                    driver: m.driver.clone(),
                    range,
                    extract_aliases,
                });
            }
            dimensions.push(Dimension {
                id: d.id.clone(),
                label: d.label.clone(),
                kind: d.kind,
                metrics,
            });
        }

        for d in &drivers {
            if let Some(m) = &d.compare_metric {
                if !metric_ids.contains(m) {
                    err(format!(
                        "driver `{}`: compare_metric `{m}` is not a metric of this pack",
                        d.id
                    ));
                }
            }
        }

        if file.id != CORE_ID {
            for k in DimensionKind::REQUIRED {
                if !kinds.contains(&k) {
                    err(format!("dimension kind `{}` is missing", k.as_str()));
                }
            }
            if kinds.contains(&DimensionKind::Core) {
                err("only the core pack may declare `core` dimensions".into());
            }
        }

        let pack = Pack {
            id: file.id,
            version: file.version,
            label: file.label,
            industry_codes: file.industry_codes,
            taxonomies: file.taxonomies,
            dimensions,
            drivers,
        };
        // The document extractor finds a KPI by label or alias, so a name two KPIs share
        // would make it guess: refused at load.
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for m in pack.metrics().filter(|m| m.layer == Layer::Kpi) {
            let own = std::iter::once(&m.label).chain(m.extract_aliases.iter());
            for name in own {
                let key = name
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_ascii_lowercase();
                if let Some(other) = names.insert(key, m.id.clone()) {
                    err(format!(
                        "metric `{}`: name `{name}` is also a label or extract alias of `{other}`",
                        m.id
                    ));
                }
            }
        }
        // References inside the pack: kpi and metric bindings must exist (core may be
        // referenced from any pack and is checked by `PackSet`), and metric chains must not loop.
        for m in pack.metrics() {
            let Some(f) = &m.formula else { continue };
            for (ident, alts) in &f.bindings {
                for b in alts {
                    match b {
                        Binding::Kpi(id) => {
                            if !pack.metric(id).is_some_and(|t| t.layer == Layer::Kpi)
                                && !id.starts_with("core.")
                            {
                                err(format!("metric `{}`: `{ident}` binds kpi `{id}` which is not a kpi of this pack", m.id));
                            }
                        }
                        Binding::Metric(id) => {
                            if !pack.metric(id).is_some_and(|t| t.layer == Layer::Xbrl)
                                && !id.starts_with("core.")
                            {
                                err(format!("metric `{}`: `{ident}` binds metric `{id}` which is not a formula metric of this pack", m.id));
                            }
                        }
                        Binding::Element { .. } | Binding::Literal(_) => {}
                    }
                }
            }
        }
        // Resolved R3 (plan §4.3 TBD): an annual KPI carries forward within its year, so a
        // four-quarter sum would count it four times. Refused at load; read it as an
        // instant or through `lag` instead. Core KPIs are checked by `PackSet`.
        for m in pack.metrics() {
            let Some(f) = &m.formula else { continue };
            for ident in eval::summed_identifiers(&f.expr, m.period == Some(Period::Ttm)) {
                for b in f.bindings.get(&ident).into_iter().flatten() {
                    if let Binding::Kpi(id) = b {
                        if pack
                            .metric(id)
                            .is_some_and(|t| t.cadence == Some(Cadence::Annual))
                        {
                            err(format!(
                                "metric `{}`: `{ident}` sums annual kpi `{id}` over four quarters; read it as an instant or with lag",
                                m.id
                            ));
                        }
                    }
                }
            }
        }
        if let Some(cycle) = pack.metric_cycle() {
            err(format!(
                "metric bindings form a cycle: {}",
                cycle.join(" -> ")
            ));
        }

        if errors.is_empty() {
            Ok(pack)
        } else {
            Err(ValidationError(errors).into())
        }
    }

    pub fn metrics(&self) -> impl Iterator<Item = &Metric> {
        self.dimensions.iter().flat_map(|d| d.metrics.iter())
    }

    pub fn metric(&self, id: &str) -> Option<&Metric> {
        self.metrics().find(|m| m.id == id)
    }

    pub fn kpi_metrics(&self) -> impl Iterator<Item = &Metric> {
        self.metrics().filter(|m| m.layer == Layer::Kpi)
    }

    /// Drivers listed without a free source ("Pendorong tanpa sumber").
    pub fn missing_drivers(&self) -> impl Iterator<Item = &Driver> {
        self.drivers.iter().filter(|d| !d.has_source())
    }

    /// First cycle found through `metric:` bindings within this pack, if any.
    fn metric_cycle(&self) -> Option<Vec<String>> {
        fn walk<'a>(
            pack: &'a Pack,
            id: &'a str,
            path: &mut Vec<&'a str>,
            done: &mut BTreeSet<&'a str>,
        ) -> Option<Vec<String>> {
            if let Some(pos) = path.iter().position(|p| *p == id) {
                let mut cycle: Vec<String> = path[pos..].iter().map(|s| s.to_string()).collect();
                cycle.push(id.to_string());
                return Some(cycle);
            }
            if done.contains(id) {
                return None;
            }
            path.push(id);
            if let Some(f) = pack.metric(id).and_then(|m| m.formula.as_ref()) {
                for alts in f.bindings.values() {
                    for b in alts {
                        if let Binding::Metric(next) = b {
                            if let Some(c) = walk(pack, next, path, done) {
                                return Some(c);
                            }
                        }
                    }
                }
            }
            path.pop();
            done.insert(id);
            None
        }
        let mut done = BTreeSet::new();
        for m in self.metrics() {
            if let Some(c) = walk(self, &m.id, &mut Vec::new(), &mut done) {
                return Some(c);
            }
        }
        None
    }
}

/// Every loaded pack, keyed by id. `core` is always present in a valid set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackSet {
    packs: BTreeMap<String, Pack>,
}

impl PackSet {
    /// Builds the set and checks cross-pack references (`metric:core.x`, `kpi:core.x`),
    /// a present core pack, and industry codes claimed by one pack only.
    pub fn new(packs: Vec<Pack>) -> Result<Self, ValidationError> {
        let mut errors = Vec::new();
        let mut map: BTreeMap<String, Pack> = BTreeMap::new();
        for p in packs {
            if map.contains_key(&p.id) {
                errors.push(format!("pack `{}` loaded twice", p.id));
            }
            map.insert(p.id.clone(), p);
        }
        let Some(core) = map.get(CORE_ID) else {
            errors.push("core pack is missing".into());
            return Err(ValidationError(errors));
        };
        let mut codes: BTreeMap<&str, &str> = BTreeMap::new();
        for p in map.values() {
            // Core and pack metric ids share one KPI namespace per ticker, so they must differ.
            if p.id != CORE_ID {
                for m in p.metrics() {
                    if core.metric(&m.id).is_some() {
                        errors.push(format!(
                            "pack `{}` metric `{}` reuses a core metric id",
                            p.id, m.id
                        ));
                    }
                }
            }
            for m in p.metrics() {
                let Some(f) = &m.formula else { continue };
                let summed = eval::summed_identifiers(&f.expr, m.period == Some(Period::Ttm));
                for (ident, alts) in &f.bindings {
                    for b in alts {
                        if let Binding::Kpi(id) = b {
                            if let Some(core_id) = id.strip_prefix("core.") {
                                if summed.contains(ident)
                                    && core
                                        .metric(core_id)
                                        .is_some_and(|t| t.cadence == Some(Cadence::Annual))
                                {
                                    errors.push(format!(
                                        "pack `{}` metric `{}`: `{ident}` sums annual kpi `{id}` over four quarters",
                                        p.id, m.id
                                    ));
                                }
                            }
                        }
                        let (id, want) = match b {
                            Binding::Metric(id) => (id, Layer::Xbrl),
                            Binding::Kpi(id) => (id, Layer::Kpi),
                            _ => continue,
                        };
                        if let Some(core_id) = id.strip_prefix("core.") {
                            if p.id == CORE_ID {
                                errors.push(format!(
                                    "core metric `{}` binds `{id}`: inside core use the bare id, so cycles are checked",
                                    m.id
                                ));
                            }
                            if !core.metric(core_id).is_some_and(|t| t.layer == want) {
                                errors.push(format!(
                                    "pack `{}` metric `{}` binds `{id}` which core does not define",
                                    p.id, m.id
                                ));
                            }
                        }
                    }
                }
            }
            for c in &p.industry_codes {
                if let Some(other) = codes.insert(c.as_str(), p.id.as_str()) {
                    if other != p.id {
                        errors.push(format!(
                            "industry code `{c}` is claimed by `{other}` and `{}`",
                            p.id
                        ));
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(Self { packs: map })
        } else {
            Err(ValidationError(errors))
        }
    }

    pub fn core(&self) -> &Pack {
        self.packs
            .get(CORE_ID)
            .expect("PackSet::new guarantees the core pack")
    }

    pub fn get(&self, id: &str) -> Option<&Pack> {
        self.packs.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.packs.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pack> {
        self.packs.values()
    }

    pub fn len(&self) -> usize {
        self.packs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }

    /// The pack whose `industry_codes` claim `code` (the filing-derived default, A2.5).
    pub fn for_industry_code(&self, code: &str) -> Option<&Pack> {
        self.packs
            .values()
            .find(|p| p.id != CORE_ID && p.industry_codes.iter().any(|c| c == code))
    }

    /// Resolves a `metric:`/`kpi:` id written inside `pack`: own pack first, `core.x` for core.
    pub fn resolve<'a>(&'a self, pack: &'a Pack, id: &str) -> Option<(&'a Pack, &'a Metric)> {
        if let Some(core_id) = id.strip_prefix("core.") {
            let core = self.core();
            return core.metric(core_id).map(|m| (core, m));
        }
        pack.metric(id).map(|m| (pack, m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
id = "demo"
version = "2026.10-1"
label = "Demo"
industry_codes = ["Z999"]

[[dimension]]
id = "d1"
label = "Satu"
kind = "volume"
  [[dimension.metric]]
  id = "a"
  label = "A"
  layer = "xbrl"
  unit = "ccy"
  direction = "higher"
  why = "Alasan."
  period = "standalone"
  formula = "X + Y"
  elements.X = ["idx-cor:SalesAndRevenue"]
  elements.Y = ["idx-cor:Other", "0"]
[[dimension]]
id = "d2"
label = "Dua"
kind = "pricing"
  [[dimension.metric]]
  id = "b"
  label = "B"
  layer = "kpi"
  unit = "USD/t"
  direction = "higher"
  why = "Alasan."
  cadence = "quarterly"
[[dimension]]
id = "d3"
label = "Tiga"
kind = "cost"
  [[dimension.metric]]
  id = "c"
  label = "C"
  layer = "xbrl"
  unit = "%"
  direction = "lower"
  why = "Alasan."
  period = "ttm"
  formula = "100 * ttm(A) / ttm(R)"
  elements.A = ["metric:a"]
  elements.R = ["idx-cor:SalesAndRevenue"]
[[dimension]]
id = "d4"
label = "Empat"
kind = "quality"
  [[dimension.metric]]
  id = "d"
  label = "D"
  layer = "xbrl"
  unit = "x"
  direction = "neutral"
  why = "Alasan."
  period = "instant"
  formula = "P / B"
  elements.P = ["idx-cor:PropertyPlantAndEquipment[idx-cor:ClassesAxis=idx-cor:VesselsMember]"]
  elements.B = ["kpi:b"]
[[dimension]]
id = "d5"
label = "Lima"
kind = "balance"
  [[dimension.metric]]
  id = "e"
  label = "E"
  layer = "driver"
  unit = "USD/t"
  direction = "neutral"
  why = "Alasan."
  driver = "coal"
[[driver]]
id = "coal"
label = "Batubara"
"#;

    #[test]
    fn parses_a_valid_pack() {
        let p = Pack::parse(MINIMAL).unwrap();
        assert_eq!(p.id, "demo");
        assert_eq!(p.dimensions.len(), 5);
        let a = p.metric("a").unwrap();
        let f = a.formula.as_ref().unwrap();
        assert_eq!(f.bindings["Y"][1], Binding::Literal(0.0));
        let d = p.metric("d").unwrap();
        let f = d.formula.as_ref().unwrap();
        assert_eq!(
            f.bindings["P"][0],
            Binding::Element {
                element: "idx-cor:PropertyPlantAndEquipment".into(),
                dims: "idx-cor:ClassesAxis=idx-cor:VesselsMember".into(),
                abs: false
            }
        );
        assert!(p.missing_drivers().any(|d| d.id == "coal"));
        let set = PackSet::new(vec![core_pack(), p]).unwrap();
        assert_eq!(set.for_industry_code("Z999").unwrap().id, "demo");
        assert!(set.for_industry_code("A000").is_none());
    }

    fn core_pack() -> Pack {
        Pack::parse(
            r#"
id = "core"
version = "1"
label = "Inti"
[[dimension]]
id = "laba"
label = "Laba"
kind = "core"
  [[dimension.metric]]
  id = "revenue"
  label = "Pendapatan"
  layer = "xbrl"
  unit = "ccy"
  direction = "higher"
  why = "Alasan."
  period = "standalone"
  formula = "R"
  elements.R = ["idx-cor:SalesAndRevenue"]
"#,
        )
        .unwrap()
    }

    fn errors_of(text: &str) -> Vec<String> {
        match Pack::parse(text) {
            Err(PackError::Invalid(ValidationError(e))) => e,
            Err(PackError::Toml(e)) => vec![format!("toml: {e}")],
            Ok(_) => vec![],
        }
    }

    #[test]
    fn refuses_an_annual_kpi_summed_over_four_quarters() {
        // `b` becomes annual and `c` sums it under ttm(): refused. Read as an instant it passes.
        let annual = MINIMAL.replace("cadence = \"quarterly\"", "cadence = \"annual\"");
        let summed = annual.replace("elements.A = [\"metric:a\"]", "elements.A = [\"kpi:b\"]");
        let errors = errors_of(&summed);
        assert!(
            errors.iter().any(|e| e.contains("sums annual kpi `b`")),
            "{errors:?}"
        );
        // A bare identifier in a `period = "ttm"` metric is a four-quarter sum too.
        let bare = summed.replace(
            "formula = \"100 * ttm(A) / ttm(R)\"",
            "formula = \"100 * A / ttm(R)\"",
        );
        assert!(errors_of(&bare)
            .iter()
            .any(|e| e.contains("sums annual kpi `b`")));
        // Inside avg4q, or in an instant metric, the KPI is read per quarter end: allowed.
        let instant = bare.replace(
            "formula = \"100 * A / ttm(R)\"",
            "formula = \"100 * avg4q(A) / ttm(R)\"",
        );
        assert!(
            !errors_of(&instant).iter().any(|e| e.contains("annual kpi")),
            "{:?}",
            errors_of(&instant)
        );
        assert!(
            Pack::parse(&annual).is_ok(),
            "annual KPI read as an instant in `d`"
        );
    }

    #[test]
    fn rejects_unbound_identifier_missing_why_bad_unit_and_missing_kind() {
        let text = MINIMAL
            .replace("formula = \"X + Y\"", "formula = \"X + Z\"")
            .replace(
                "why = \"Alasan.\"\n  period = \"standalone\"",
                "period = \"standalone\"",
            )
            .replace(
                "unit = \"USD/t\"\n  direction = \"higher\"",
                "unit = \"ton\"\n  direction = \"higher\"",
            )
            .replace("kind = \"balance\"", "kind = \"volume\"");
        let errors = errors_of(&text);
        assert!(
            errors.iter().any(|e| e.contains("`Z` is not bound")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("`Y` is not used")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("`why` is missing")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("unit `ton`")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("kind `balance` is missing")),
            "{errors:?}"
        );
    }

    #[test]
    fn ratio_units_need_a_division_and_metric_cycles_are_refused() {
        let text = MINIMAL.replace(
            "formula = \"100 * ttm(A) / ttm(R)\"",
            "formula = \"ttm(A) - ttm(R)\"",
        );
        let errors = errors_of(&text);
        assert!(
            errors.iter().any(|e| e.contains("not a ratio")),
            "{errors:?}"
        );

        let text = MINIMAL.replace(
            "elements.Y = [\"idx-cor:Other\", \"0\"]",
            "elements.Y = [\"metric:c\"]",
        );
        let errors = errors_of(&text);
        assert!(errors.iter().any(|e| e.contains("cycle")), "{errors:?}");
    }

    #[test]
    fn unknown_keys_and_bad_toml_are_errors() {
        assert!(errors_of(&MINIMAL.replace("version = ", "versio = "))[0].starts_with("toml:"));
        assert!(errors_of("id = ")[0].starts_with("toml:"));
    }

    #[test]
    fn binding_parse_sorts_dims_and_rejects_junk() {
        let b = Binding::parse("idx-cor:E[b=2; a=1]").unwrap();
        assert_eq!(b.source(), "idx-cor:E[a=1;b=2]");
        assert!(Binding::parse("NoPrefix").is_err());
        assert!(Binding::parse("idx-cor:E[a]").is_err());
        assert!(Binding::parse("kpi:Bad-Id").is_err());
        assert_eq!(Binding::parse(" 1.5 ").unwrap(), Binding::Literal(1.5));
    }

    #[test]
    fn core_references_are_checked_by_the_set() {
        let text = MINIMAL.replace(
            "elements.A = [\"metric:a\"]",
            "elements.A = [\"metric:core.nope\"]",
        );
        let p = Pack::parse(&text).unwrap();
        let err = PackSet::new(vec![core_pack(), p]).unwrap_err();
        assert!(err.0[0].contains("core does not define"), "{err}");
        assert!(PackSet::new(vec![]).is_err());
    }

    #[test]
    fn extract_aliases_are_optional_kpi_only_and_validated() {
        // None: the default, as every canon pack written before R8.
        assert!(Pack::parse(MINIMAL)
            .unwrap()
            .metric("b")
            .unwrap()
            .extract_aliases
            .is_empty());
        let with = MINIMAL.replace(
            "  cadence = \"quarterly\"\n",
            "  cadence = \"quarterly\"\n  extract_aliases = [\"  average   selling price \", \"ASP\"]\n",
        );
        let p = Pack::parse(&with).unwrap();
        assert_eq!(
            p.metric("b").unwrap().extract_aliases,
            ["average selling price", "ASP"]
        );
        let on_formula = MINIMAL.replace(
            "  period = \"ttm\"\n",
            "  period = \"ttm\"\n  extract_aliases = [\"xyz\"]\n",
        );
        assert!(errors_of(&on_formula)[0].contains("belongs to kpi metrics"));
        for (alias, want) in [
            ("\"ab\"", "3 to 60 characters"),
            ("\"\"", "3 to 60 characters"),
            ("\"b\\u0007bb\"", "control character"),
        ] {
            let bad = MINIMAL.replace(
                "  cadence = \"quarterly\"\n",
                &format!("  cadence = \"quarterly\"\n  extract_aliases = [{alias}]\n"),
            );
            assert!(errors_of(&bad).iter().any(|e| e.contains(want)), "{alias}");
        }
        let many = MINIMAL.replace(
            "  cadence = \"quarterly\"\n",
            "  cadence = \"quarterly\"\n  extract_aliases = [\"aaa\",\"bbb\",\"ccc\",\"ddd\",\"eee\",\"fff\",\"ggg\",\"hhh\",\"iii\"]\n",
        );
        assert!(errors_of(&many).iter().any(|e| e.contains("at most 8")));
    }

    #[test]
    fn a_name_two_kpis_share_is_refused() {
        let two_kpis = MINIMAL.replace(
            "  [[dimension.metric]]\n  id = \"c\"",
            "  [[dimension.metric]]\n  id = \"k2\"\n  label = \"K2\"\n  layer = \"kpi\"\n  unit = \"USD/t\"\n  direction = \"higher\"\n  why = \"Alasan.\"\n  cadence = \"quarterly\"\n  extract_aliases = [\"b\"]\n  [[dimension.metric]]\n  id = \"c\"",
        );
        // `B` (label of kpi b) and the alias `b` of k2 are the same name.
        let errs = errors_of(&two_kpis);
        assert!(
            errs.iter()
                .any(|e| e.contains("also a label or extract alias")),
            "{errs:?}"
        );
    }
}
