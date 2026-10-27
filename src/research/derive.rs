//! Standalone, TTM and annual views over stored facts, comparative backfill, restatement
//! detection and the reconciliation check (research plan §4.4, design.md §3.2).
//!
//! Pure: `derive` takes a ticker's undimensioned facts (with the filing each came from) and
//! returns one `Cell` per (element, fiscal year, quarter), the restatements it found and the
//! reconciliation failures. Rules:
//!
//! - A duration fact whose start is 1 January and whose end is a quarter end of the same
//!   year is the year-to-date figure of that quarter (`FY` = Q4 YTD). Fiscal years are
//!   calendar years (ASUMSI, true of every filing on disk).
//! - The value of a period comes from the filing that reported it originally; when that
//!   filing is missing, the latest later filing's comparative fills it (`from_comparative`).
//! - A later comparative that differs from the original by more than `0.5 × 10^(−decimals)`
//!   is a restatement: the cell is flagged `restated`, the original stays the displayed
//!   value (plan §4.4) and both values are returned for the fact drawer.
//! - A later filing that reports exactly `−original` (within the rounding) is a
//!   `sign_conflict`, not `restated`; the restatement row is still written.
//! - Standalone: Q1 = YTD Q1; Qn = YTD Qn − YTD Qn−1; Q4 = FY − YTD Q3 (`derived_fy`).
//!   A missing input is a `gap`, never a guess; so is a difference of two vintages (inputs
//!   reported by filings of different fiscal years, e.g. an original YTD and a comparative
//!   from the next year's filing).
//! - TTM(Qn) = YTD Qn + FY(prev) − YTD Qn(prev) when those three exist (so Q1 is not an
//!   input), else the sum of the four latest standalone quarters, else a gap. TTM(Q4) = FY.
//! - Basis break: a fiscal year where a later filing restated at least 30% of the
//!   comparable FY elements with a median relative gap above 20%. Every cell of that year
//!   is flagged `basis_break`; TTM of the next year's Q1..Q3 uses the new basis only (YTD
//!   and prior-year YTD from the quarter's own filing, prior FY from a filing of that year
//!   published no later), else a gap; never the four-quarter fallback.
//! - Per-share elements (`PerShare` in the name) are not additive: arithmetic values are
//!   flagged `per_share_approx`, there is no four-quarter TTM and no reconciliation.
//! - Instants carry their value in `standalone` (and `annual` at Q4) with flag `instant`.
//! - Reconciliation: Σ standalone Q1..Q4 vs FY within four roundings when all four exist,
//!   for a FY that is not already a recorded restatement. A restated FY is explained: it
//!   stays visible through `restated` and the restatements table, not as a failure (ADR 0001).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use super::xbrl::CtxKind;

/// Bumped whenever a derivation rule changes; stored on every derived cell.
pub const RULES_VERSION: &str = "2026.10-r4";

/// Outflow and expense tags that filings report with either sign, sometimes flipping between
/// the filings of one year (LEAD capex: Q1 positive, H1 negative). Their year-to-date values
/// are read as magnitudes before the quarters are differenced; taking the magnitude of a
/// derived quarter instead would add the two YTD values. The packs read them with `abs:`.
pub const MAGNITUDE_FLOWS: &[&str] = &[
    "idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment",
    "idx-cor:PaymentsForAcquisitionOfPropertyAndEquipment",
    "idx-cor:AcquisitionOfPropertyPlantAndEquipment",
    "idx-cor:ThirdPartiesShareOnReturnOfTemporarySyirkahFunds",
    "idx-cor:SubtotalShariaExpense",
];

/// Share, in percent, of a year's comparable FY elements a later filing restated, from
/// which the year may be a basis break (with `BASIS_BREAK_GAP`).
pub const BASIS_BREAK_SHARE_PCT: usize = 30;
/// Median relative gap of those restated FYs above which the year is a basis break.
pub const BASIS_BREAK_GAP: f64 = 0.20;

/// One stored fact with the filing it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct FactIn {
    pub filing_id: String,
    pub filing_fy: i32,
    /// 1 to 4; an FY filing is 4.
    pub filing_q: u32,
    pub element: String,
    pub ctx_kind: CtxKind,
    pub start: Option<NaiveDate>,
    pub end: NaiveDate,
    pub dims: String,
    pub value: f64,
    pub decimals: Option<i32>,
}

/// Identifies a stored fact: the store's primary key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FactRef {
    pub filing_id: String,
    pub element: String,
    pub ctx_kind: String,
    pub start: Option<String>,
    pub end: String,
    pub dims: String,
}

impl FactRef {
    fn of(f: &FactIn) -> Self {
        Self {
            filing_id: f.filing_id.clone(),
            element: f.element.clone(),
            ctx_kind: f.ctx_kind.as_str().into(),
            start: f.start.map(|d| d.to_string()),
            end: f.end.to_string(),
            dims: f.dims.clone(),
        }
    }
}

/// One operand of a derived value, with the role it played in the formula.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    /// `ytd_q1`, `ytd_q2`, `fy`, `prev_fy`, `prev_ytd_q2`, `standalone_q3`, `instant`.
    pub role: String,
    pub value: f64,
    pub fact: FactRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Flag {
    DerivedFy,
    FromComparative,
    Restated,
    Gap,
    Instant,
    ReconcileFailed,
    /// The fiscal year was re-reported on another basis by a later filing.
    BasisBreak,
    /// A later filing reports the period with the opposite sign.
    SignConflict,
    /// A per-share figure added or subtracted across periods: an estimate.
    PerShareApprox,
}

impl Flag {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DerivedFy => "derived_fy",
            Self::FromComparative => "from_comparative",
            Self::Restated => "restated",
            Self::Gap => "gap",
            Self::Instant => "instant",
            Self::ReconcileFailed => "reconcile_failed",
            Self::BasisBreak => "basis_break",
            Self::SignConflict => "sign_conflict",
            Self::PerShareApprox => "per_share_approx",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "derived_fy" => Some(Self::DerivedFy),
            "from_comparative" => Some(Self::FromComparative),
            "restated" => Some(Self::Restated),
            "gap" => Some(Self::Gap),
            "instant" => Some(Self::Instant),
            "reconcile_failed" => Some(Self::ReconcileFailed),
            "basis_break" => Some(Self::BasisBreak),
            "sign_conflict" => Some(Self::SignConflict),
            "per_share_approx" => Some(Self::PerShareApprox),
            _ => None,
        }
    }
}

pub fn flags_to_string(flags: &BTreeSet<Flag>) -> String {
    flags
        .iter()
        .map(|f| f.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn flags_from_string(s: &str) -> BTreeSet<Flag> {
    s.split(',').filter_map(Flag::parse).collect()
}

/// Inputs of one view, kept apart so the drawer can show each formula.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellInputs {
    pub standalone: Vec<Input>,
    pub ttm: Vec<Input>,
    pub annual: Vec<Input>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub element: String,
    pub fy: i32,
    pub q: u32,
    pub standalone: Option<f64>,
    pub ttm: Option<f64>,
    pub annual: Option<f64>,
    pub flags: BTreeSet<Flag>,
    pub inputs: CellInputs,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Restatement {
    pub element: String,
    pub ctx_kind: CtxKind,
    pub start: Option<NaiveDate>,
    pub end: NaiveDate,
    pub dims: String,
    pub original: f64,
    pub original_filing: String,
    pub restated: f64,
    pub restated_filing: String,
}

/// Σ displayed standalone Q1..Q4 differs from the FY as the latest filing reports it, and
/// nothing explains it: the FY is not a recorded restatement (those are reported through the
/// `restated` flag and the restatements table). The standalone quarters telescope to the
/// original FY, so this fires only when the latest FY differs without being a restatement.
#[derive(Clone, Debug, PartialEq)]
pub struct ReconciliationFailure {
    pub element: String,
    pub fy: i32,
    pub sum_standalone: f64,
    /// FY per the latest filing that carries it.
    pub fy_value: f64,
    pub fy_filing: String,
    pub tolerance: f64,
    /// Flags of the four quarter cells, for the explanation on health.
    pub quarter_flags: Vec<String>,
}

impl ReconciliationFailure {
    /// One line for health and the ingest report.
    pub fn explain(&self) -> String {
        let cause = if self.quarter_flags.iter().any(|f| f.contains("restated")) {
            "a later filing restated one of the inputs; the original quarters stay displayed"
        } else if self
            .quarter_flags
            .iter()
            .any(|f| f.contains("from_comparative"))
        {
            "a quarter was filled from a later comparative that no longer matches the FY"
        } else {
            "the filed quarters and the FY disagree beyond the declared rounding"
        };
        format!(
            "{} FY{}: sum of standalone quarters {} vs FY {} in {} (tolerance {}): {cause}",
            self.element,
            self.fy,
            self.sum_standalone,
            self.fy_value,
            self.fy_filing,
            self.tolerance
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Derived {
    pub cells: Vec<Cell>,
    pub restatements: Vec<Restatement>,
    pub reconciliation_failures: Vec<ReconciliationFailure>,
}

/// Half a unit of the declared rounding; exact facts (`decimals` absent) tolerate nothing.
pub fn restatement_tolerance(decimals: Option<i32>) -> f64 {
    match decimals {
        Some(d) => 0.5 * 10f64.powi(-d),
        None => 0.0,
    }
}

/// `(fy, q)` of a year-to-date duration: 1 January to a quarter end of the same year.
pub fn ytd_quarter(start: NaiveDate, end: NaiveDate) -> Option<(i32, u32)> {
    if start.year() != end.year() || start.month() != 1 || start.day() != 1 {
        return None;
    }
    quarter_end(end).map(|q| (end.year(), q))
}

/// Quarter number when `d` is the last day of a quarter.
pub fn quarter_end(d: NaiveDate) -> Option<u32> {
    match (d.month(), d.day()) {
        (3, 31) => Some(1),
        (6, 30) => Some(2),
        (9, 30) => Some(3),
        (12, 31) => Some(4),
        _ => None,
    }
}

/// The chosen value for one period plus everything the drawer needs.
#[derive(Clone, Debug)]
struct Chosen {
    value: f64,
    fact: FactRef,
    decimals: Option<i32>,
    from_comparative: bool,
    /// Fiscal year of the filing the value comes from: an original of year Y is vintage Y,
    /// a comparative in a filing of Y + 1 is vintage Y + 1.
    vintage: i32,
    restated: bool,
    /// A later filing reports `−value` (beyond the rounding); not counted as `restated`.
    sign_conflict: bool,
    /// The same period as the latest filing reports it (equals `value` unless restated).
    latest_value: f64,
    latest_filing: String,
}

/// Picks the reported value of one period from every filing that carries it, in filing
/// order, and records restatements against the earliest (original) one.
fn choose(
    period_key: (i32, u32),
    mut candidates: Vec<&FactIn>,
    restatements: &mut Vec<Restatement>,
) -> Chosen {
    candidates.sort_by_key(|f| (f.filing_fy, f.filing_q, f.filing_id.clone()));
    let original_idx = candidates
        .iter()
        .position(|f| (f.filing_fy, f.filing_q) == period_key);
    let base = original_idx.map_or(candidates[0], |i| candidates[i]);
    let mut restated = false;
    let mut sign_conflict = false;
    for later in candidates
        .iter()
        .filter(|f| (f.filing_fy, f.filing_q) > (base.filing_fy, base.filing_q))
    {
        // The coarser of the two declared roundings, so a filing rounded to millions is
        // not called a restatement of one reported to the rupiah.
        let tol = restatement_tolerance(base.decimals).max(restatement_tolerance(later.decimals));
        if (later.value - base.value).abs() > tol {
            // The same figure with the opposite sign is a filing error, not a restatement.
            if (later.value + base.value).abs() <= tol {
                sign_conflict = true;
            } else {
                restated = true;
            }
            restatements.push(Restatement {
                element: base.element.clone(),
                ctx_kind: base.ctx_kind,
                start: base.start,
                end: base.end,
                dims: base.dims.clone(),
                original: base.value,
                original_filing: base.filing_id.clone(),
                restated: later.value,
                restated_filing: later.filing_id.clone(),
            });
        }
    }
    // Display: the original filing's value; else the latest comparative (plan §4.2).
    let shown = match original_idx {
        Some(i) => candidates[i],
        None => candidates[candidates.len() - 1],
    };
    let latest = candidates[candidates.len() - 1];
    Chosen {
        value: shown.value,
        fact: FactRef::of(shown),
        decimals: shown.decimals,
        from_comparative: original_idx.is_none(),
        vintage: shown.filing_fy,
        restated,
        sign_conflict,
        latest_value: latest.value,
        latest_filing: latest.filing_id.clone(),
    }
}

fn input(role: &str, c: &Chosen) -> Input {
    Input {
        role: role.into(),
        value: c.value,
        fact: c.fact.clone(),
    }
}

fn mark(flags: &mut BTreeSet<Flag>, c: &Chosen) {
    if c.from_comparative {
        flags.insert(Flag::FromComparative);
    }
    if c.restated {
        flags.insert(Flag::Restated);
    }
    if c.sign_conflict {
        flags.insert(Flag::SignConflict);
    }
}

/// Per-share figures (EPS) are not additive across periods. Matched on the local name.
fn is_per_share(element: &str) -> bool {
    element
        .rsplit_once(':')
        .map_or(element, |(_, local)| local)
        .contains("PerShare")
}

fn fact_input(role: &str, f: &FactIn) -> Input {
    Input {
        role: role.into(),
        value: f.value,
        fact: FactRef::of(f),
    }
}

/// The chosen YTD value of every period of one flow element.
fn choose_ytd(
    facts: &[&FactIn],
    restatements: &mut Vec<Restatement>,
) -> BTreeMap<(i32, u32), Chosen> {
    let mut by_period: BTreeMap<(i32, u32), Vec<&FactIn>> = BTreeMap::new();
    for f in facts {
        if let Some(k) = f.start.and_then(|s| ytd_quarter(s, f.end)) {
            by_period.entry(k).or_default().push(f);
        }
    }
    by_period
        .into_iter()
        .map(|(k, c)| (k, choose(k, c, restatements)))
        .collect()
}

/// Median of a non-empty slice.
fn median(values: &mut [f64]) -> f64 {
    debug_assert!(!values.is_empty());
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Fiscal years a later filing re-reported on another basis. Among the flow elements whose
/// FY both the original FY filing and a later filing report (per-share excluded), at least
/// `BASIS_BREAK_SHARE_PCT` were restated and the median relative gap of those restated,
/// `|latest − original| / max(|latest|, |original|)`, exceeds `BASIS_BREAK_GAP`. A sign
/// conflict is a filing error, not a restatement: it counts as comparable, not as restated.
fn basis_breaks<'a>(
    flows: impl Iterator<Item = (&'a str, &'a BTreeMap<(i32, u32), Chosen>)>,
) -> BTreeSet<i32> {
    let mut per_year: BTreeMap<i32, (usize, Vec<f64>)> = BTreeMap::new();
    for (_, ytd) in flows.filter(|(e, _)| !is_per_share(e)) {
        for (&(fy, q), c) in ytd {
            if q != 4 || c.from_comparative || c.latest_filing == c.fact.filing_id {
                continue;
            }
            let year = per_year.entry(fy).or_default();
            year.0 += 1;
            if c.restated {
                let scale = c.value.abs().max(c.latest_value.abs());
                let gap = if scale > 0.0 {
                    (c.latest_value - c.value).abs() / scale
                } else {
                    0.0
                };
                year.1.push(gap);
            }
        }
    }
    per_year
        .into_iter()
        .filter_map(|(fy, (n, mut gaps))| {
            let broken = !gaps.is_empty()
                && gaps.len() * 100 >= n * BASIS_BREAK_SHARE_PCT
                && median(&mut gaps) > BASIS_BREAK_GAP;
            broken.then_some(fy)
        })
        .collect()
}

/// TTM(fy, q) when fy − 1 is a basis break, from the new basis only: YTD q, its prior-year
/// comparative and the prior FY, all from the quarter's own (original) filing. None when that
/// filing does not carry them, never a mix of the two bases.
fn one_basis_ttm(
    fy: i32,
    q: u32,
    facts: &[&FactIn],
    ytd: &BTreeMap<(i32, u32), Chosen>,
) -> Option<(f64, Vec<Input>)> {
    let cur = ytd.get(&(fy, q)).filter(|c| !c.from_comparative)?;
    let in_own_filing = |period: (i32, u32)| {
        facts.iter().copied().find(|f| {
            f.filing_id == cur.fact.filing_id
                && f.start.and_then(|s| ytd_quarter(s, f.end)) == Some(period)
        })
    };
    let prev_q = in_own_filing((fy - 1, q))?;
    let prev_fy = in_own_filing((fy - 1, 4))?;
    Some((
        cur.value + prev_fy.value - prev_q.value,
        vec![
            input(&format!("ytd_q{q}"), cur),
            fact_input("prev_fy", prev_fy),
            fact_input(&format!("prev_ytd_q{q}"), prev_q),
        ],
    ))
}

fn derive_flow(
    element: &str,
    facts: &[&FactIn],
    ytd: &BTreeMap<(i32, u32), Chosen>,
    breaks: &BTreeSet<i32>,
    failures: &mut Vec<ReconciliationFailure>,
) -> Vec<Cell> {
    let (Some(&(first_fy, _)), Some(&last)) = (ytd.keys().next(), ytd.keys().next_back()) else {
        return Vec::new();
    };
    let per_share = is_per_share(element);

    let mut cells: Vec<Cell> = Vec::new();
    for fy in first_fy..=last.0 {
        for q in 1..=4u32 {
            if (fy, q) > last {
                break;
            }
            let mut flags = BTreeSet::new();
            let mut inputs = CellInputs::default();
            // ---- standalone ----
            let cur = ytd.get(&(fy, q));
            let prev = (q > 1).then(|| ytd.get(&(fy, q - 1))).flatten();
            let standalone = match (q, cur, prev) {
                (1, Some(c), _) => {
                    inputs.standalone.push(input("ytd_q1", c));
                    mark(&mut flags, c);
                    Some(c.value)
                }
                (_, Some(c), Some(p)) => {
                    mark(&mut flags, c);
                    mark(&mut flags, p);
                    if c.vintage == p.vintage {
                        let role = if q == 4 { "fy" } else { "ytd_q" };
                        inputs.standalone.push(input(
                            &if q == 4 {
                                role.to_string()
                            } else {
                                format!("{role}{q}")
                            },
                            c,
                        ));
                        inputs.standalone.push(input(&format!("ytd_q{}", q - 1), p));
                        if q == 4 {
                            flags.insert(Flag::DerivedFy);
                        }
                        if per_share {
                            flags.insert(Flag::PerShareApprox);
                        }
                        Some(c.value - p.value)
                    } else {
                        // The two inputs were reported by filings of different years.
                        flags.insert(Flag::Gap);
                        None
                    }
                }
                _ => {
                    flags.insert(Flag::Gap);
                    None
                }
            };
            // ---- annual ----
            let annual = match (q, cur) {
                (4, Some(c)) => {
                    inputs.annual.push(input("fy", c));
                    Some(c.value)
                }
                _ => None,
            };
            cells.push(Cell {
                element: element.into(),
                fy,
                q,
                standalone,
                ttm: None,
                annual,
                flags,
                inputs,
            });
        }
    }
    // ---- TTM, after every standalone exists ----
    let standalone_of =
        |fy: i32, q: u32, cells: &[Cell]| -> Option<(f64, Vec<Input>, BTreeSet<Flag>)> {
            cells.iter().find(|c| c.fy == fy && c.q == q).and_then(|c| {
                c.standalone
                    .map(|v| (v, c.inputs.standalone.clone(), c.flags.clone()))
            })
        };
    for i in 0..cells.len() {
        let (fy, q) = (cells[i].fy, cells[i].q);
        let mut ttm_inputs = Vec::new();
        let mut ttm_flags = BTreeSet::new();
        let ttm = if q == 4 {
            ytd.get(&(fy, 4)).map(|c| {
                ttm_inputs.push(input("fy", c));
                mark(&mut ttm_flags, c);
                c.value
            })
        } else if breaks.contains(&(fy - 1)) {
            one_basis_ttm(fy, q, facts, ytd).map(|(v, ins)| {
                if let Some(c) = ytd.get(&(fy, q)) {
                    mark(&mut ttm_flags, c);
                }
                if per_share {
                    ttm_flags.insert(Flag::PerShareApprox);
                }
                ttm_inputs = ins;
                v
            })
        } else {
            match (
                ytd.get(&(fy, q)),
                ytd.get(&(fy - 1, 4)),
                ytd.get(&(fy - 1, q)),
            ) {
                (Some(c), Some(pfy), Some(pq)) => {
                    ttm_inputs.push(input(&format!("ytd_q{q}"), c));
                    ttm_inputs.push(input("prev_fy", pfy));
                    ttm_inputs.push(input(&format!("prev_ytd_q{q}"), pq));
                    mark(&mut ttm_flags, c);
                    mark(&mut ttm_flags, pfy);
                    mark(&mut ttm_flags, pq);
                    if per_share {
                        ttm_flags.insert(Flag::PerShareApprox);
                    }
                    Some(c.value + pfy.value - pq.value)
                }
                // Four per-share quarters do not add up to a year.
                _ if per_share => None,
                _ => {
                    // Fall back to the four latest standalone quarters.
                    let mut sum = 0.0;
                    let mut ok = true;
                    let (mut y, mut qq) = (fy, q);
                    for _ in 0..4 {
                        match standalone_of(y, qq, &cells) {
                            Some((v, ins, fl)) => {
                                sum += v;
                                for mut inp in ins {
                                    inp.role = format!("standalone_{y}q{qq}:{}", inp.role);
                                    ttm_inputs.push(inp);
                                }
                                ttm_flags.extend(fl.into_iter().filter(|f| *f != Flag::Gap));
                            }
                            None => {
                                ok = false;
                                break;
                            }
                        }
                        if qq == 1 {
                            y -= 1;
                            qq = 4;
                        } else {
                            qq -= 1;
                        }
                    }
                    ok.then_some(sum)
                }
            }
        };
        cells[i].ttm = ttm;
        cells[i].inputs.ttm = ttm_inputs;
        cells[i].flags.extend(ttm_flags);
    }
    // ---- reconciliation (per-share figures do not add up to the FY) ----
    if per_share {
        return cells;
    }
    for fy in first_fy..=last.0 {
        let Some(fy_fact) = ytd.get(&(fy, 4)) else {
            continue;
        };
        let quarters: Vec<&Cell> = cells
            .iter()
            .filter(|c| c.fy == fy && c.standalone.is_some())
            .collect();
        if quarters.len() != 4 {
            continue;
        }
        let sum: f64 = quarters.iter().map(|c| c.standalone.unwrap_or(0.0)).sum();
        let tolerance = 4.0 * restatement_tolerance(fy_fact.decimals) + 1e-6;
        // A recorded restatement of the FY explains the gap (ADR 0001).
        if !fy_fact.restated && (sum - fy_fact.latest_value).abs() > tolerance {
            failures.push(ReconciliationFailure {
                element: element.into(),
                fy,
                sum_standalone: sum,
                fy_value: fy_fact.latest_value,
                fy_filing: fy_fact.latest_filing.clone(),
                tolerance,
                quarter_flags: quarters.iter().map(|c| flags_to_string(&c.flags)).collect(),
            });
            if let Some(c) = cells.iter_mut().find(|c| c.fy == fy && c.q == 4) {
                c.flags.insert(Flag::ReconcileFailed);
            }
        }
    }
    cells
}

fn derive_instant(
    element: &str,
    facts: &[&FactIn],
    restatements: &mut Vec<Restatement>,
) -> Vec<Cell> {
    let mut by_period: BTreeMap<(i32, u32), Vec<&FactIn>> = BTreeMap::new();
    for f in facts {
        if let Some(q) = quarter_end(f.end) {
            by_period.entry((f.end.year(), q)).or_default().push(f);
        }
    }
    by_period
        .into_iter()
        .map(|(k, c)| {
            let chosen = choose(k, c, restatements);
            let mut flags = BTreeSet::from([Flag::Instant]);
            mark(&mut flags, &chosen);
            let inp = vec![input("instant", &chosen)];
            Cell {
                element: element.into(),
                fy: k.0,
                q: k.1,
                standalone: Some(chosen.value),
                ttm: None,
                annual: (k.1 == 4).then_some(chosen.value),
                flags,
                inputs: CellInputs {
                    standalone: inp.clone(),
                    ttm: Vec::new(),
                    annual: if k.1 == 4 { inp } else { Vec::new() },
                },
            }
        })
        .collect()
}

/// Derives every undimensioned element of one ticker. Dimensioned facts are left to the
/// drawer and the packs; odd durations (not year-to-date) are ignored.
pub fn derive(facts: &[FactIn]) -> Derived {
    let mut out = Derived::default();
    let is_magnitude = |f: &FactIn| MAGNITUDE_FLOWS.contains(&f.element.as_str());
    let magnitudes: Vec<FactIn> = facts
        .iter()
        .filter(|f| f.dims.is_empty() && is_magnitude(f))
        .map(|f| FactIn {
            value: f.value.abs(),
            ..f.clone()
        })
        .collect();
    let mut by_element: BTreeMap<(&str, CtxKind), Vec<&FactIn>> = BTreeMap::new();
    let signed = facts
        .iter()
        .filter(|f| f.dims.is_empty() && !is_magnitude(f));
    for f in signed.chain(&magnitudes) {
        by_element
            .entry((f.element.as_str(), f.ctx_kind))
            .or_default()
            .push(f);
    }
    // Every flow element's YTD values first: a basis break is a property of the year
    // across elements.
    let ytds: Vec<BTreeMap<(i32, u32), Chosen>> = by_element
        .iter()
        .map(|((_, kind), facts)| match kind {
            CtxKind::Duration => choose_ytd(facts, &mut out.restatements),
            CtxKind::Instant => BTreeMap::new(),
        })
        .collect();
    let breaks = basis_breaks(by_element.keys().map(|(e, _)| *e).zip(ytds.iter()));
    for (((element, kind), facts), ytd) in by_element.iter().zip(&ytds) {
        let cells = match kind {
            CtxKind::Duration => derive_flow(
                element,
                facts,
                ytd,
                &breaks,
                &mut out.reconciliation_failures,
            ),
            CtxKind::Instant => derive_instant(element, facts, &mut out.restatements),
        };
        out.cells.extend(cells);
    }
    for c in out.cells.iter_mut().filter(|c| breaks.contains(&c.fy)) {
        c.flags.insert(Flag::BasisBreak);
    }
    out.restatements.sort_by(|a, b| {
        (&a.element, a.end, &a.restated_filing).cmp(&(&b.element, b.end, &b.restated_filing))
    });
    out.restatements.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// A YTD fact of `element` for (fy, q) reported by filing (ffy, fq).
    fn ytd(ffy: i32, fq: u32, fy: i32, q: u32, value: f64) -> FactIn {
        let end = match q {
            1 => format!("{fy}-03-31"),
            2 => format!("{fy}-06-30"),
            3 => format!("{fy}-09-30"),
            _ => format!("{fy}-12-31"),
        };
        FactIn {
            filing_id: format!(
                "AAAA_{ffy}_{}",
                if fq == 4 {
                    "FY".into()
                } else {
                    format!("Q{fq}")
                }
            ),
            filing_fy: ffy,
            filing_q: fq,
            element: "idx-cor:Revenue".into(),
            ctx_kind: CtxKind::Duration,
            start: Some(d(&format!("{fy}-01-01"))),
            end: d(&end),
            dims: String::new(),
            value,
            decimals: Some(-6),
        }
    }

    fn cell(dv: &Derived, fy: i32, q: u32) -> &Cell {
        dv.cells
            .iter()
            .find(|c| c.fy == fy && c.q == q && c.element == "idx-cor:Revenue")
            .unwrap()
    }

    /// One full year of filings with comparatives in the next year's filings.
    fn two_years() -> Vec<FactIn> {
        let m = 1e6;
        vec![
            ytd(2024, 1, 2024, 1, 100.0 * m),
            ytd(2024, 2, 2024, 2, 230.0 * m),
            ytd(2024, 3, 2024, 3, 350.0 * m),
            ytd(2024, 4, 2024, 4, 500.0 * m),
            ytd(2025, 1, 2025, 1, 120.0 * m),
            ytd(2025, 1, 2024, 1, 100.0 * m),
            ytd(2025, 2, 2025, 2, 260.0 * m),
            ytd(2025, 2, 2024, 2, 230.0 * m),
            ytd(2025, 2, 2024, 4, 500.0 * m),
        ]
    }

    #[test]
    fn a_magnitude_flow_filed_with_both_signs_in_one_year_differences_its_magnitudes() {
        // Q1 filed as a positive payment, H1 as a negative one.
        let capex = "idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment";
        let facts: Vec<FactIn> = [(1, 190_813.0), (2, -1_197_002.0)]
            .into_iter()
            .map(|(q, v)| FactIn {
                element: capex.into(),
                ..ytd(2025, q, 2025, q, v)
            })
            .collect();
        let dv = derive(&facts);
        let q2 = dv
            .cells
            .iter()
            .find(|c| c.element == capex && (c.fy, c.q) == (2025, 2))
            .unwrap();
        assert_eq!(q2.standalone, Some(1_006_189.0));
        // A signed flow keeps its sign.
        let dv = derive(&[ytd(2025, 1, 2025, 1, -5.0)]);
        assert_eq!(cell(&dv, 2025, 1).standalone, Some(-5.0));
    }

    #[test]
    fn standalone_ttm_annual_and_q4_flag() {
        let dv = derive(&two_years());
        let m = 1e6;
        assert_eq!(cell(&dv, 2024, 1).standalone, Some(100.0 * m));
        assert_eq!(cell(&dv, 2024, 2).standalone, Some(130.0 * m));
        assert_eq!(cell(&dv, 2024, 3).standalone, Some(120.0 * m));
        let q4 = cell(&dv, 2024, 4);
        assert_eq!(q4.standalone, Some(150.0 * m));
        assert!(q4.flags.contains(&Flag::DerivedFy));
        assert_eq!(q4.annual, Some(500.0 * m));
        assert_eq!(q4.ttm, Some(500.0 * m), "TTM at Q4 is the FY");
        assert_eq!(cell(&dv, 2024, 2).annual, None);
        // TTM(2025 Q2) = YTD Q2 2025 + FY 2024 − YTD Q2 2024 = 260 + 500 − 230.
        let q2 = cell(&dv, 2025, 2);
        assert_eq!(q2.ttm, Some(530.0 * m));
        let roles: Vec<_> = q2.inputs.ttm.iter().map(|i| i.role.as_str()).collect();
        assert_eq!(roles, ["ytd_q2", "prev_fy", "prev_ytd_q2"]);
        // Q1 2025 TTM falls back to four standalone quarters: 120 + 150 + 120 + 130.
        assert_eq!(cell(&dv, 2025, 1).ttm, Some(520.0 * m));
        assert!(q4.inputs.standalone.iter().any(|i| i.role == "fy"));
        assert!(q4.inputs.standalone.iter().any(|i| i.role == "ytd_q3"));
        assert_eq!(q4.inputs.standalone[0].fact.filing_id, "AAAA_2024_FY");
        assert!(dv.restatements.is_empty());
        assert!(dv.reconciliation_failures.is_empty());
    }

    #[test]
    fn missing_ytd_is_filled_from_a_later_comparative_or_is_a_gap() {
        let m = 1e6;
        let mut facts = two_years();
        // The original Q1 2024 filing is lost; the Q1 2025 comparative still carries it.
        facts.retain(|f| f.filing_id != "AAAA_2024_Q1");
        let dv = derive(&facts);
        let q1 = cell(&dv, 2024, 1);
        assert_eq!(q1.standalone, Some(100.0 * m));
        assert!(q1.flags.contains(&Flag::FromComparative));
        assert_eq!(q1.inputs.standalone[0].fact.filing_id, "AAAA_2025_Q1");
        // Q2 would subtract that comparative from the original YTD Q2: two vintages, a gap.
        let q2 = cell(&dv, 2024, 2);
        assert_eq!(q2.standalone, None);
        assert!(q2.flags.contains(&Flag::Gap));
        assert!(q2.flags.contains(&Flag::FromComparative), "says why");
        // No comparative at all for Q3 2024 → Q3 and Q4 are gaps, never guesses.
        let mut facts = two_years();
        facts.retain(|f| f.filing_id != "AAAA_2024_Q3");
        let dv = derive(&facts);
        let q3 = cell(&dv, 2024, 3);
        assert_eq!(q3.standalone, None);
        assert!(q3.flags.contains(&Flag::Gap));
        let q4 = cell(&dv, 2024, 4);
        assert_eq!(q4.standalone, None);
        assert!(q4.flags.contains(&Flag::Gap));
        assert_eq!(q4.annual, Some(500.0 * m), "annual still comes from the FY");
        assert_eq!(q4.ttm, Some(500.0 * m));
        assert!(
            dv.reconciliation_failures.is_empty(),
            "nothing to reconcile with gaps"
        );
    }

    #[test]
    fn restatement_is_detected_just_above_the_rounding_bound() {
        let m = 1e6;
        let base = two_years();
        // decimals −6 → tolerance 500 000: exactly at the bound is not a restatement.
        let mut at_bound = base.clone();
        at_bound
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-06-30"))
            .unwrap()
            .value = 230.0 * m + 500_000.0;
        let dv = derive(&at_bound);
        assert!(dv.restatements.is_empty());
        assert!(!cell(&dv, 2024, 2).flags.contains(&Flag::Restated));

        let mut above = base;
        above
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-06-30"))
            .unwrap()
            .value = 230.0 * m + 500_001.0;
        let dv = derive(&above);
        assert_eq!(dv.restatements.len(), 1);
        let r = &dv.restatements[0];
        assert_eq!((r.original, r.restated), (230.0 * m, 230.0 * m + 500_001.0));
        assert_eq!(
            (r.original_filing.as_str(), r.restated_filing.as_str()),
            ("AAAA_2024_Q2", "AAAA_2025_Q2")
        );
        let q2 = cell(&dv, 2024, 2);
        assert!(q2.flags.contains(&Flag::Restated));
        assert_eq!(
            q2.standalone,
            Some(130.0 * m),
            "the original value stays displayed"
        );
        // The 2025 Q2 TTM uses the comparative as the company restated it, and says so.
        let q2_25 = cell(&dv, 2025, 2);
        assert!(q2_25.flags.contains(&Flag::Restated));
    }

    #[test]
    fn exact_facts_tolerate_no_difference() {
        let mut facts = two_years();
        for f in &mut facts {
            f.decimals = None;
        }
        facts
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-06-30"))
            .unwrap()
            .value += 1.0;
        assert_eq!(derive(&facts).restatements.len(), 1);
        assert_eq!(restatement_tolerance(Some(2)), 0.005);
        assert_eq!(restatement_tolerance(Some(0)), 0.5);
        assert_eq!(restatement_tolerance(None), 0.0);
    }

    #[test]
    fn reconciliation_fails_only_when_sources_disagree() {
        let m = 1e6;
        let mut facts = two_years();
        // Lose the original Q2 2024 and restate its comparative by 10: Q2 and Q3 standalone
        // would mix the comparative with the original Q1 and Q3, so both are gaps and the
        // year has nothing to reconcile.
        facts.retain(|f| f.filing_id != "AAAA_2024_Q2");
        facts
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-06-30"))
            .unwrap()
            .value = 240.0 * m;
        let dv = derive(&facts);
        assert_eq!(cell(&dv, 2024, 2).standalone, None);
        assert_eq!(cell(&dv, 2024, 3).standalone, None);
        assert!(dv.reconciliation_failures.is_empty());
        // A filing that reports a discrete quarter inconsistent with the FY does fail.
        let mut facts = two_years();
        facts
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2024_FY")
            .unwrap()
            .value = 480.0 * m;
        facts.push(FactIn {
            filing_id: "AAAA_2025_Q2".into(),
            filing_fy: 2025,
            filing_q: 2,
            element: "idx-cor:Revenue".into(),
            ctx_kind: CtxKind::Duration,
            start: Some(d("2024-01-01")),
            end: d("2024-12-31"),
            dims: String::new(),
            value: 500.0 * m,
            decimals: Some(-6),
        });
        let dv = derive(&facts);
        // Original FY 480 stays displayed (Q4 = 480 − 350 = 130, Σ = 480) while the latest
        // filing says 500: the FY is a recorded restatement, so the year is explained and
        // is not a reconciliation failure.
        assert_eq!(dv.restatements.len(), 1);
        assert!(dv.reconciliation_failures.is_empty());
        let q4 = cell(&dv, 2024, 4);
        assert!(q4.flags.contains(&Flag::Restated));
        assert!(!q4.flags.contains(&Flag::ReconcileFailed));
    }

    #[test]
    fn restated_fy_is_not_a_failure_but_an_unrestated_mismatch_still_is() {
        let m = 1e6;
        // The latest filing reports the FY with the opposite sign: a sign conflict, not a
        // restatement, so nothing explains why Σ standalone (the original FY) differs.
        let mut facts = two_years();
        facts
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-12-31"))
            .unwrap()
            .value = -500.0 * m;
        let dv = derive(&facts);
        assert_eq!(dv.reconciliation_failures.len(), 1);
        let f = &dv.reconciliation_failures[0];
        assert_eq!(
            (f.fy, f.sum_standalone, f.fy_value),
            (2024, 500.0 * m, -500.0 * m)
        );
        let q4 = cell(&dv, 2024, 4);
        assert!(q4.flags.contains(&Flag::ReconcileFailed));
        assert!(q4.flags.contains(&Flag::SignConflict));
        assert!(!q4.flags.contains(&Flag::Restated));
        assert!(!cell(&dv, 2024, 3).flags.contains(&Flag::ReconcileFailed));
        // The same FY restated to another figure is explained: no failure, flagged restated.
        let mut facts = two_years();
        facts
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-12-31"))
            .unwrap()
            .value = 520.0 * m;
        let dv = derive(&facts);
        assert!(dv.reconciliation_failures.is_empty());
        let q4 = cell(&dv, 2024, 4);
        assert!(q4.flags.contains(&Flag::Restated));
        assert!(!q4.flags.contains(&Flag::ReconcileFailed));
        assert_eq!(dv.restatements.len(), 1, "the restatements table keeps it");
    }

    #[test]
    fn instants_are_carried_without_derivation() {
        let inst = |ffy: i32, fq: u32, end: &str, value: f64| FactIn {
            filing_id: format!("AAAA_{ffy}_Q{fq}"),
            filing_fy: ffy,
            filing_q: fq,
            element: "idx-cor:Equity".into(),
            ctx_kind: CtxKind::Instant,
            start: None,
            end: d(end),
            dims: String::new(),
            value,
            decimals: Some(0),
        };
        let facts = vec![
            inst(2025, 1, "2025-03-31", 10.0),
            inst(2025, 1, "2024-12-31", 9.0),
            inst(2025, 2, "2025-06-30", 11.0),
            inst(2025, 2, "2024-12-31", 9.0),
            inst(2025, 2, "2023-12-31", 7.0),
        ];
        let dv = derive(&facts);
        let find = |fy, q| dv.cells.iter().find(|c| c.fy == fy && c.q == q).unwrap();
        assert_eq!(find(2025, 2).standalone, Some(11.0));
        assert_eq!(find(2025, 2).ttm, None);
        assert!(find(2025, 2).flags.contains(&Flag::Instant));
        let fy24 = find(2024, 4);
        assert_eq!((fy24.standalone, fy24.annual), (Some(9.0), Some(9.0)));
        assert!(fy24.flags.contains(&Flag::FromComparative));
        assert_eq!(find(2023, 4).standalone, Some(7.0));
        assert_eq!(dv.cells.len(), 4, "no gap rows are invented for instants");
    }

    #[test]
    fn dimensioned_and_odd_duration_facts_are_ignored() {
        let mut f = ytd(2024, 1, 2024, 1, 5.0);
        f.dims = "idx-cor:Axis=idx-cor:M".into();
        let mut g = ytd(2024, 1, 2024, 1, 5.0);
        g.start = Some(d("2024-02-01"));
        assert!(derive(&[f, g]).cells.is_empty());
        assert_eq!(
            ytd_quarter(d("2024-01-01"), d("2024-06-30")),
            Some((2024, 2))
        );
        assert_eq!(ytd_quarter(d("2023-01-01"), d("2024-06-30")), None);
        assert_eq!(ytd_quarter(d("2024-01-01"), d("2024-05-31")), None);
        assert_eq!(quarter_end(d("2024-12-31")), Some(4));
    }

    #[test]
    fn flags_round_trip_as_text() {
        let f = BTreeSet::from([Flag::Restated, Flag::DerivedFy]);
        assert_eq!(flags_to_string(&f), "derived_fy,restated");
        assert_eq!(flags_from_string("derived_fy,restated,bogus"), f);
        assert!(flags_from_string("").is_empty());
        for (flag, text) in [
            (Flag::BasisBreak, "basis_break"),
            (Flag::SignConflict, "sign_conflict"),
            (Flag::PerShareApprox, "per_share_approx"),
        ] {
            assert_eq!(flag.as_str(), text);
            assert_eq!(Flag::parse(text), Some(flag));
        }
    }

    /// `ytd` for another element.
    fn ytd_of(element: &str, ffy: i32, fq: u32, fy: i32, q: u32, value: f64) -> FactIn {
        FactIn {
            element: element.into(),
            ..ytd(ffy, fq, fy, q, value)
        }
    }

    fn cell_of<'a>(dv: &'a Derived, element: &str, fy: i32, q: u32) -> &'a Cell {
        dv.cells
            .iter()
            .find(|c| c.fy == fy && c.q == q && c.element == element)
            .unwrap()
    }

    #[test]
    fn standalone_from_comparatives_of_one_filing_year_is_kept() {
        let m = 1e6;
        let mut facts = two_years();
        // The original Q3 and FY 2024 filings are lost; the Q2 and Q3 2025 filings carry
        // FY and YTD Q3 2024: two filings of one year, one vintage.
        facts.retain(|f| !matches!(f.filing_id.as_str(), "AAAA_2024_Q3" | "AAAA_2024_FY"));
        facts.push(ytd(2025, 3, 2024, 3, 350.0 * m));
        let dv = derive(&facts);
        let q4 = cell(&dv, 2024, 4);
        assert_eq!(q4.standalone, Some(150.0 * m), "one vintage: kept");
        assert!(q4.flags.contains(&Flag::FromComparative));
        let filings: Vec<_> = q4
            .inputs
            .standalone
            .iter()
            .map(|i| i.fact.filing_id.as_str())
            .collect();
        assert_eq!(filings, ["AAAA_2025_Q2", "AAAA_2025_Q3"]);
        // Q3 = comparative YTD Q3 − original YTD Q2: two vintages.
        assert_eq!(cell(&dv, 2024, 3).standalone, None);
    }

    #[test]
    fn mixed_vintage_q4_is_a_gap() {
        let m = 1e6;
        let mut facts = two_years();
        // YTD Q3 2024 only survives as a later comparative, restated; the FY is original.
        facts.retain(|f| f.filing_id != "AAAA_2024_Q3");
        facts.push(ytd(2025, 3, 2024, 3, 900.0 * m));
        let dv = derive(&facts);
        let q4 = cell(&dv, 2024, 4);
        assert_eq!(
            q4.standalone, None,
            "FY − a later filing's YTD Q3 is no quarter"
        );
        assert!(q4.flags.contains(&Flag::Gap));
        assert!(!q4.flags.contains(&Flag::DerivedFy));
        assert_eq!(q4.annual, Some(500.0 * m));
    }

    #[test]
    fn a_later_filing_that_flips_the_sign_is_a_sign_conflict() {
        let m = 1e6;
        let mut facts = two_years();
        facts
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-12-31"))
            .unwrap()
            .value = -500.0 * m;
        let dv = derive(&facts);
        let q4 = cell(&dv, 2024, 4);
        assert!(q4.flags.contains(&Flag::SignConflict));
        assert!(!q4.flags.contains(&Flag::Restated));
        assert_eq!(q4.annual, Some(500.0 * m), "the original stays displayed");
        assert_eq!(
            dv.restatements.len(),
            1,
            "the restatement row is still written"
        );
        assert_eq!(dv.restatements[0].restated, -500.0 * m);
        // −base within rounding is still a sign conflict; anything else is a restatement.
        let mut facts = two_years();
        facts
            .iter_mut()
            .find(|f| f.filing_id == "AAAA_2025_Q2" && f.end == d("2024-12-31"))
            .unwrap()
            .value = -499.0 * m;
        let dv = derive(&facts);
        let q4 = cell(&dv, 2024, 4);
        assert!(q4.flags.contains(&Flag::Restated));
        assert!(!q4.flags.contains(&Flag::SignConflict));
    }

    #[test]
    fn per_share_elements_are_approximations_and_not_reconciled() {
        let eps = "idx-cor:BasicEarningsLossPerShareFromContinuingOperations";
        let mut facts: Vec<FactIn> = two_years()
            .into_iter()
            .map(|f| FactIn {
                element: eps.into(),
                value: f.value / 1e6,
                decimals: Some(2),
                ..f
            })
            .collect();
        // The 2025 filing restates FY 2024: a revenue-like element would fail to reconcile.
        facts.push(ytd_of(eps, 2025, 4, 2024, 4, 520.0));
        facts.push(ytd_of(eps, 2025, 4, 2025, 4, 600.0));
        let dv = derive(&facts);
        assert!(dv.reconciliation_failures.is_empty(), "EPS is not additive");
        let q1 = cell_of(&dv, eps, 2024, 1);
        assert_eq!(q1.standalone, Some(100.0), "Q1 is the filed figure");
        assert!(!q1.flags.contains(&Flag::PerShareApprox));
        let q2 = cell_of(&dv, eps, 2024, 2);
        assert_eq!(
            q2.standalone,
            Some(130.0),
            "a YTD difference, kept as an estimate"
        );
        assert!(q2.flags.contains(&Flag::PerShareApprox));
        assert!(cell_of(&dv, eps, 2024, 4)
            .flags
            .contains(&Flag::PerShareApprox));
        let q2_25 = cell_of(&dv, eps, 2025, 2);
        assert_eq!(q2_25.ttm, Some(530.0));
        assert!(q2_25.flags.contains(&Flag::PerShareApprox));
        // Q4 TTM is the filed FY, exact.
        let q4_25 = cell_of(&dv, eps, 2025, 4);
        assert_eq!(q4_25.ttm, Some(600.0));
        assert_eq!(q4_25.standalone, None, "no YTD Q3 2025");
        assert!(!q4_25.flags.contains(&Flag::PerShareApprox));
    }

    /// Ten elements, 2024 to 2025, every filing carrying the prior year's YTD comparatives
    /// and the 2025 interim filings of `fy_in` also FY 2024. `restate(i)` gives the factor
    /// the 2025 filings apply to element i's 2024 figures.
    fn ten_elements(restate: impl Fn(usize) -> f64, fy_in: &[u32]) -> Vec<FactIn> {
        let m = 1e6;
        let mut facts = Vec::new();
        for i in 0..10 {
            let e = format!("idx-cor:E{i}");
            let k = restate(i);
            let y24 = [100.0, 230.0, 350.0, 500.0];
            let y25 = [120.0, 260.0, 390.0, 560.0];
            for q in 1..=4u32 {
                let qi = (q - 1) as usize;
                facts.push(ytd_of(&e, 2024, q, 2024, q, y24[qi] * m));
                facts.push(ytd_of(&e, 2025, q, 2025, q, y25[qi] * m));
                facts.push(ytd_of(&e, 2025, q, 2024, q, y24[qi] * k * m));
            }
            for &q in fy_in {
                facts.push(ytd_of(&e, 2025, q, 2024, 4, 500.0 * k * m));
            }
        }
        facts.push(FactIn {
            element: "idx-cor:Equity".into(),
            ctx_kind: CtxKind::Instant,
            start: None,
            ..ytd(2024, 4, 2024, 4, 7.0 * m)
        });
        facts
    }

    #[test]
    fn a_basis_break_flags_the_year_and_keeps_next_year_ttm_on_one_basis() {
        let m = 1e6;
        // 4 of 10 elements restated ×5 (gap 80%): a basis break of FY 2024.
        let restate = |i: usize| if i < 4 { 5.0 } else { 1.0 };
        let dv = derive(&ten_elements(restate, &[1, 2, 3]));
        for c in dv.cells.iter().filter(|c| c.fy == 2024) {
            assert!(
                c.flags.contains(&Flag::BasisBreak),
                "{} {}Q{}",
                c.element,
                c.fy,
                c.q
            );
        }
        assert!(!dv
            .cells
            .iter()
            .any(|c| c.fy == 2025 && c.flags.contains(&Flag::BasisBreak)));
        // TTM(2025 Q2) of a restated element: YTD Q2 2025 + FY 2024 − YTD Q2 2024, all three
        // from the Q2 2025 filing (the 2024 figures as restated).
        let q2 = cell_of(&dv, "idx-cor:E0", 2025, 2);
        assert_eq!(q2.ttm, Some((260.0 + 2500.0 - 1150.0) * m));
        let filings: Vec<_> = q2
            .inputs
            .ttm
            .iter()
            .map(|i| i.fact.filing_id.as_str())
            .collect();
        assert_eq!(filings, ["AAAA_2025_Q2"; 3]);
        // An unrestated element gives the same number on either basis.
        assert_eq!(
            cell_of(&dv, "idx-cor:E9", 2025, 2).ttm,
            Some((260.0 + 500.0 - 230.0) * m)
        );
    }

    #[test]
    fn a_basis_break_without_a_published_new_basis_fy_leaves_ttm_a_gap() {
        // No 2025 interim filing carries FY 2024: only the FY 2025 filing (after the
        // quarter) does, so TTM 2025 Q1..Q3 is a gap rather than an old/new mix.
        let restate = |i: usize| if i < 4 { 5.0 } else { 1.0 };
        let dv = derive(&ten_elements(restate, &[]));
        for q in 1..=3 {
            let c = cell_of(&dv, "idx-cor:E0", 2025, q);
            assert_eq!(c.ttm, None, "2025 Q{q}");
            assert!(c.inputs.ttm.is_empty());
        }
        assert_eq!(cell_of(&dv, "idx-cor:E0", 2025, 4).ttm, Some(560.0 * 1e6));
        // FY 2024 only in the Q1 filing: Q1 has one basis, Q2 does not borrow it.
        let dv = derive(&ten_elements(restate, &[1]));
        assert_eq!(
            cell_of(&dv, "idx-cor:E0", 2025, 1).ttm,
            Some((120.0 + 2500.0 - 500.0) * 1e6)
        );
        assert_eq!(cell_of(&dv, "idx-cor:E0", 2025, 2).ttm, None);
    }

    /// Factor that gives a relative gap `g` (`|k − 1| / max(k, 1)` with k > 1).
    fn factor(g: f64) -> f64 {
        1.0 / (1.0 - g)
    }

    fn breaks(restate: impl Fn(usize) -> f64) -> bool {
        derive(&ten_elements(restate, &[1, 2, 3]))
            .cells
            .iter()
            .any(|c| c.flags.contains(&Flag::BasisBreak))
    }

    #[test]
    fn basis_break_thresholds_are_a_share_and_a_median() {
        // Exactly 30% of the elements restated by 80%: a break; 20% is not.
        assert!(breaks(|i| if i < 3 { 5.0 } else { 1.0 }));
        assert!(!breaks(|i| if i < 2 { 5.0 } else { 1.0 }));
        // Every element restated by exactly 20%: not above the bound.
        assert!(!breaks(|_| 1.25));
        // Gaps 5%, 5%, 90%: mean 33% but median 5%, no break.
        let gaps = [0.05, 0.05, 0.9];
        assert!(!breaks(|i| gaps.get(i).map_or(1.0, |g| factor(*g))));
        // Even count: the median is the mean of the two middle gaps.
        let above = [0.1, 0.15, 0.3, 0.9]; // (0.15 + 0.3) / 2 = 0.225
        assert!(breaks(|i| above.get(i).map_or(1.0, |g| factor(*g))));
        let below = [0.1, 0.12, 0.26, 0.9]; // (0.12 + 0.26) / 2 = 0.19
        assert!(!breaks(|i| below.get(i).map_or(1.0, |g| factor(*g))));
    }

    #[test]
    fn sign_conflicts_and_per_share_elements_do_not_make_a_basis_break() {
        // 4 of 10 FYs reported with the opposite sign: filing errors, not restatements.
        assert!(!breaks(|i| if i < 4 { -1.0 } else { 1.0 }));
        // 4 of 10 restated ×5, but they are per-share: out of the count, 0 of 6.
        let facts: Vec<FactIn> = ten_elements(|i| if i < 4 { 5.0 } else { 1.0 }, &[1, 2, 3])
            .into_iter()
            .map(|f| match f.element.as_str() {
                "idx-cor:E0" | "idx-cor:E1" | "idx-cor:E2" | "idx-cor:E3" => FactIn {
                    element: format!("{}PerShare", f.element),
                    ..f
                },
                _ => f,
            })
            .collect();
        assert!(!derive(&facts)
            .cells
            .iter()
            .any(|c| c.flags.contains(&Flag::BasisBreak)));
        assert!(is_per_share(
            "idx-cor:BasicEarningsLossPerShareFromContinuingOperations"
        ));
        assert!(!is_per_share("PerShareExt:Revenue"));
    }

    #[test]
    fn small_or_few_restatements_are_no_basis_break() {
        // 2 of 10 restated ×5: 20% of elements.
        let few = derive(&ten_elements(|i| if i < 2 { 5.0 } else { 1.0 }, &[1, 2, 3]));
        // 10 of 10 restated by 10%: median gap under 20%.
        let small = derive(&ten_elements(|_| 1.1, &[1, 2, 3]));
        for dv in [few, small] {
            assert!(!dv.cells.iter().any(|c| c.flags.contains(&Flag::BasisBreak)));
            // The old formula stands: original FY 2024 and the 2025 Q2 comparative.
            let q2 = cell_of(&dv, "idx-cor:E0", 2025, 2);
            assert!(q2.ttm.is_some());
            assert_eq!(q2.inputs.ttm[1].fact.filing_id, "AAAA_2024_FY");
        }
    }
}
