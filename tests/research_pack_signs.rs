//! Sign and selection regressions found in visual QA round 2 (lane E): the facts keep the
//! element names and signs the filings carry, the packs normalise them in one place.
//! BRIS reports the profit-sharing expense negative and tags both a mudharabah time-deposit
//! sub-line and the total; LEAD reports capex negative in some quarters and positive in
//! others. Magnitudes are rounded and invented, the shapes are not.

use std::collections::HashMap;

use finance_app::research::packs::{
    evaluate, Binding, Ctx, Outcome, PackSet, Packs, Period, Quarter, Source, View,
};

#[derive(Default)]
struct Facts {
    values: HashMap<(String, i32, u32), f64>,
}

impl Facts {
    fn set(&mut self, element: &str, fy: i32, q: u32, v: f64) {
        self.values.insert((element.into(), fy, q), v);
    }

    /// Every quarter 2025Q2 to 2026Q2 (five instants for `avg4q`, four flows for `ttm`).
    fn series(&mut self, element: &str, v: f64) {
        for (fy, q) in [(2025, 2), (2025, 3), (2025, 4), (2026, 1), (2026, 2)] {
            self.set(element, fy, q, v);
        }
    }
}

impl Source for Facts {
    fn element(&self, element: &str, dims: &str, q: Quarter, _view: View) -> Option<f64> {
        if !dims.is_empty() {
            return None;
        }
        self.values.get(&(element.into(), q.fy, q.q)).copied()
    }

    fn kpi(&self, _pack: &str, _metric: &str, _q: Quarter) -> Option<f64> {
        None
    }

    fn unit(&self, _element: &str) -> Option<String> {
        Some("IDR".into())
    }
}

fn eval(set: &PackSet, pack: &str, metric: &str, src: &Facts, q: Quarter) -> Outcome {
    let ctx = Ctx {
        source: src,
        packs: set,
    };
    let pack = if pack == "core" {
        set.core()
    } else {
        set.get(pack).unwrap()
    };
    let m = pack.metric(metric).unwrap();
    evaluate(
        &ctx,
        pack,
        m.formula.as_ref().unwrap(),
        m.period.unwrap_or(Period::Standalone),
        q,
    )
}

/// A BRIS-shaped bank: the expense arrives negative, the time-deposit sub-line is a sliver
/// of the total.
fn bris(expense_sign: f64) -> Facts {
    let mut f = Facts::default();
    f.series("idx-cor:WadiahDemandDeposits", 20e12);
    f.series("idx-cor:MudharabahSavingsDeposits", 106e12);
    f.series("idx-cor:MudharabahTimeDeposits", 55e9);
    f.series("idx-cor:TotalMudharabahTimeDeposits", 155e12);
    f.series("idx-cor:MurabahahReceivablesThirdParties", 250e12);
    f.series("idx-cor:MusyarakahFinancingThirdParties", 20e12);
    f.series("idx-cor:MarketableSecuritiesThirdParties", 17e12);
    f.series(
        "idx-cor:PlacementsWithBankIndonesiaAndOtherBanksThirdParties",
        26e12,
    );
    f.series("idx-cor:RevenueFromFundManagementAsMudharib", 7e12);
    f.series(
        "idx-cor:ThirdPartiesShareOnReturnOfTemporarySyirkahFunds",
        expense_sign * 2.25e12,
    );
    f
}

const Q2_26: Quarter = Quarter { fy: 2026, q: 2 };

fn value(o: &Outcome) -> f64 {
    o.value.unwrap_or_else(|| panic!("gap: {:?}", o.gap))
}

#[test]
fn bank_funding_costs_use_the_expense_magnitude_whatever_its_sign() {
    let set = Packs::embedded().set();
    // ttm expense 9 T over average funds 281 T, 3,2 %; margin (28 T - 9 T) / 313 T, 6,1 %.
    for sign in [-1.0, 1.0] {
        let src = bris(sign);
        let cof = value(&eval(&set, "bank_sharia", "cost_of_funds", &src, Q2_26));
        assert!(
            (cof - 3.2028).abs() < 0.001,
            "cost of funds {cof} at {sign}"
        );
        let nim = value(&eval(
            &set,
            "bank_sharia",
            "net_margin_earning_assets",
            &src,
            Q2_26,
        ));
        assert!((nim - 6.0703).abs() < 0.001, "margin {nim} at {sign}");
        let cti_in = eval(&set, "bank_sharia", "cost_to_income", &src, Q2_26);
        // No G&A facts here, so the ratio is a gap, not a number built on a signed expense.
        assert!(cti_in.value.is_none());
    }
}

#[test]
fn third_party_funds_prefer_the_total_time_deposit_line() {
    let set = Packs::embedded().set();
    let src = bris(-1.0);
    let dpk = value(&eval(&set, "bank_sharia", "third_party_funds", &src, Q2_26));
    assert_eq!(dpk, 20e12 + 106e12 + 155e12);
    let casa = value(&eval(&set, "bank_sharia", "casa_ratio", &src, Q2_26));
    assert!(casa > 40.0 && casa < 50.0, "CASA {casa}");
    let fdr = value(&eval(&set, "bank_sharia", "fdr", &src, Q2_26));
    assert!(fdr > 90.0 && fdr < 100.0, "FDR {fdr}");
}

#[test]
fn time_deposit_sub_line_still_answers_when_the_total_is_not_tagged() {
    let set = Packs::embedded().set();
    let mut src = bris(-1.0);
    src.values
        .retain(|(e, _, _), _| e != "idx-cor:TotalMudharabahTimeDeposits");
    let dpk = value(&eval(&set, "bank_sharia", "third_party_funds", &src, Q2_26));
    assert_eq!(dpk, 20e12 + 106e12 + 55e9);
}

#[test]
fn capex_and_free_cash_flow_use_the_capex_magnitude() {
    let set = Packs::embedded().set();
    let mut src = Facts::default();
    let ocf = "idx-cor:NetCashFlowsReceivedFromUsedInOperatingActivities";
    let capex = "idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment";
    // Quarters handed over with both signs (derive already takes the YTD magnitudes of these
    // tags, see derive::MAGNITUDE_FLOWS; the binding still reads a magnitude on its own).
    src.set(ocf, 2025, 1, 6_069_176.0);
    src.set(capex, 2025, 1, 190_813.0);
    src.set(ocf, 2025, 2, 1_377_747.0);
    src.set(capex, 2025, 2, -1_387_815.0);
    for (q, spent, ocf_q) in [(1, 190_813.0, 6_069_176.0), (2, 1_387_815.0, 1_377_747.0)] {
        let quarter = Quarter::new(2025, q);
        let c = value(&eval(&set, "core", "capex", &src, quarter));
        assert_eq!(c, spent, "capex Q{q}");
        let fcf = value(&eval(&set, "core", "free_cash_flow", &src, quarter));
        assert_eq!(fcf, ocf_q - spent, "free cash flow Q{q}");
    }
}

/// `abs:` on a derived quarter cannot undo a sign flip between the YTD filings it was
/// differenced from, so every `abs:` tag must also be a magnitude flow in `derive`.
#[test]
fn every_abs_tag_of_the_packs_is_a_magnitude_flow_in_derive() {
    let set = Packs::embedded().set();
    for pack in set.iter() {
        for m in pack.metrics() {
            let Some(f) = &m.formula else { continue };
            for b in f.bindings.values().flatten() {
                if let Binding::Element {
                    element, abs: true, ..
                } = b
                {
                    assert!(
                        finance_app::research::derive::MAGNITUDE_FLOWS.contains(&element.as_str()),
                        "{}.{}: `abs:{element}` is not in derive::MAGNITUDE_FLOWS",
                        pack.id,
                        m.id
                    );
                }
            }
        }
    }
}

#[test]
fn abs_prefix_parses_for_tagged_facts_only() {
    let b = Binding::parse("abs:idx-cor:Expense").unwrap();
    assert_eq!(
        b,
        Binding::Element {
            element: "idx-cor:Expense".into(),
            dims: String::new(),
            abs: true
        }
    );
    assert_eq!(b.source(), "idx-cor:Expense");
    assert!(Binding::parse("abs:metric:capex").is_err());
    assert!(Binding::parse("abs:kpi:units").is_err());
    let plain = Binding::parse("idx-cor:Expense").unwrap();
    assert_eq!(plain.source(), "idx-cor:Expense");
}
