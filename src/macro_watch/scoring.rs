//! Composite macro scoring, ported from MC `backend/services/macro_scoring.py`
//! (`calculate_macro_scoring`, `_effective_internal_weights`,
//! `evaluate_metric_signal`, `determine_regime_from_score`).
//!
//! Numerics follow CPython: `sum()` is Neumaier-compensated, `round()` is
//! half-even, `round(x, n)` is correctly rounded.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use crate::py::{py_fsum, py_round, py_round_int, py_title};

pub const SCORING_CONFIG_VERSION: &str = "2026.09-scoring-final";

pub const PILLAR_WEIGHTS: [(&str, f64); 6] = [
    ("energy", 0.15),
    ("ust_rates", 0.20),
    ("usd_id", 0.15),
    ("inflation_cb", 0.15),
    ("growth", 0.20),
    ("liquidity", 0.15),
];

fn internal_weights(pillar: &str) -> &'static [(&'static str, f64)] {
    match pillar {
        "energy" => &[
            ("brent_front_month", 0.40),
            ("brent_curve", 0.30),
            ("wti_spot", 0.30),
        ],
        "ust_rates" => &[
            ("ust_2y", 0.25),
            ("ust_10y", 0.25),
            ("ust_10y_2y_spread", 0.25),
            ("ust_10y_real_yield", 0.25),
        ],
        "usd_id" => &[("dxy", 0.50), ("usd_idr", 0.50)],
        "inflation_cb" => &[
            ("fed_funds_path", 0.40),
            ("us_core_cpi", 0.30),
            ("id_core_cpi", 0.30),
        ],
        "growth" => &[("china_pmi_primary", 1.00)],
        "liquidity" => &[("vix", 0.50), ("us_hy_oas", 0.50)],
        _ => &[],
    }
}

/// `evaluate_metric_signal`: -2 (adverse) … +2 (supportive).
pub fn signal(metric_id: &str, v: f64) -> i64 {
    match metric_id {
        "brent_front_month" => {
            if v >= 90.0 {
                2
            } else if v >= 75.0 {
                1
            } else {
                0
            }
        }
        "brent_curve" => {
            if v > 0.0 {
                2
            } else if v < 0.0 {
                -1
            } else {
                0
            }
        }
        "wti_spot" => {
            if v >= 85.0 {
                2
            } else if v >= 70.0 {
                1
            } else {
                0
            }
        }
        "ust_2y" => {
            if v >= 4.5 {
                -1
            } else if v >= 3.5 {
                0
            } else {
                1
            }
        }
        "ust_10y" => {
            if v >= 5.0 {
                -2
            } else if v >= 4.2 {
                -1
            } else {
                0
            }
        }
        "ust_10y_2y_spread" => 0,
        "ust_10y_real_yield" => {
            if v >= 2.5 {
                -2
            } else if v >= 1.8 {
                -1
            } else {
                0
            }
        }
        "dxy" => {
            if v >= 101.0 {
                -1
            } else if v >= 98.0 {
                0
            } else {
                1
            }
        }
        "usd_idr" => {
            if v >= 16500.0 {
                -2
            } else if v >= 15800.0 {
                -1
            } else {
                0
            }
        }
        "fed_funds_path" => {
            if v >= 3.75 {
                -1
            } else {
                0
            }
        }
        "us_core_cpi" => {
            if v >= 2.4 {
                -1
            } else {
                1
            }
        }
        "id_core_cpi" => {
            if (1.5..=3.5).contains(&v) {
                1
            } else if v > 3.5 {
                -1
            } else {
                0
            }
        }
        "china_pmi_primary" => {
            if v < 50.0 {
                -1
            } else {
                1
            }
        }
        "vix" => {
            if v < 30.0 {
                1
            } else {
                -2
            }
        }
        "us_hy_oas" => {
            if v < 5.0 {
                1
            } else {
                -2
            }
        }
        _ => 0,
    }
}

pub fn regime(score: f64) -> &'static str {
    if score > 35.0 {
        "Risk-on"
    } else if score >= 10.0 {
        "Selective risk-on"
    } else if score >= -15.0 {
        "Neutral"
    } else if score >= -50.0 {
        "Defensive"
    } else {
        "Liquidity stress"
    }
}

/// `_effective_internal_weights` with the 40% intra-pillar cap.
pub fn effective_weights(configured: &[(&str, f64)], valid: &[&str]) -> Option<Vec<(String, f64)>> {
    const CAP: f64 = 0.40;
    if valid.is_empty() {
        return None;
    }
    let w = |m: &str| {
        configured
            .iter()
            .find(|(k, _)| *k == m)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    };
    if configured.len() < 3 {
        let sum = py_fsum(&valid.iter().map(|m| w(m)).collect::<Vec<_>>());
        if sum <= 0.0 || !sum.is_finite() {
            return None;
        }
        return Some(valid.iter().map(|m| (m.to_string(), w(m) / sum)).collect());
    }
    if valid.len() as f64 * CAP < 1.0 - 1e-12 {
        return None;
    }
    let mut weights: Vec<(String, f64)> = valid
        .iter()
        .map(|m| (m.to_string(), w(m).min(CAP)))
        .collect();
    let mut remaining = 1.0 - py_fsum(&weights.iter().map(|(_, v)| *v).collect::<Vec<_>>());
    let get = |ws: &[(String, f64)], m: &str| {
        ws.iter()
            .find(|(k, _)| k == m)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    };
    let mut candidates: Vec<&str> = valid
        .iter()
        .copied()
        .filter(|m| CAP - get(&weights, m) > 1e-12)
        .collect();

    while remaining > 1e-12 && !candidates.is_empty() {
        let mut pref_total = py_fsum(&candidates.iter().map(|m| w(m).max(0.0)).collect::<Vec<_>>());
        if pref_total <= 0.0 {
            pref_total = candidates.len() as f64;
        }
        let mut distributed = 0.0;
        for m in &candidates {
            let pref = if w(m).max(0.0) == 0.0 {
                1.0
            } else {
                w(m).max(0.0)
            };
            let current = get(&weights, m);
            let alloc = (CAP - current).min(remaining * pref / pref_total);
            if let Some(e) = weights.iter_mut().find(|(k, _)| k == m) {
                e.1 += alloc;
            }
            distributed += alloc;
        }
        if distributed <= 1e-12 {
            break;
        }
        remaining -= distributed;
        candidates.retain(|m| CAP - get(&weights, m) > 1e-12);
    }
    if remaining > 1e-9 {
        return None;
    }
    Some(weights)
}

/// `calculate_macro_scoring` → the `CompositeScoringResult` dump.
pub fn calculate(
    observations: &[Value],
    disabled: &BTreeSet<String>,
    score_vintage: &str,
) -> Value {
    let obs = |m: &str| observations.iter().find(|o| o["metric_id"] == m);
    let mut details = Map::new();
    let mut scores: Vec<(String, Option<f64>)> = Vec::new();
    let mut coverage = Map::new();
    let (mut valid_pillars, mut excluded, mut reasons, mut blocking) = (
        Vec::<String>::new(),
        Vec::new(),
        Map::new(),
        BTreeSet::new(),
    );

    for (pillar, configured_weight) in PILLAR_WEIGHTS {
        let internal = internal_weights(pillar);
        let total = py_fsum(&internal.iter().map(|(_, w)| *w).collect::<Vec<_>>());
        let (mut valid, mut missing, mut valid_sum) = (Vec::new(), Vec::new(), 0.0f64);
        for (mid, w) in internal {
            let ok = obs(mid).is_some_and(|o| {
                o["value"].as_f64().is_some_and(f64::is_finite)
                    && o["upstream_ok"] == true
                    && o["freshness"] != "unavailable"
                    && !disabled.contains(*mid)
            });
            if ok {
                valid.push(*mid);
                valid_sum += w;
            } else {
                missing.push(*mid);
                if matches!(*mid, "vix" | "us_hy_oas") || disabled.contains(*mid) {
                    blocking.insert(mid.to_string());
                }
            }
        }
        let coverage_pct = py_round_int(valid_sum / total * 100.0);
        coverage.insert(pillar.into(), json!(coverage_pct));
        let eff = effective_weights(internal, &valid);
        let capacity_ok = eff.is_some();
        let coverage_ok = coverage_pct >= 70;
        if !coverage_ok {
            reasons.insert(
                pillar.into(),
                json!(format!(
                    "Coverage {coverage_pct}/100 below 70/100 threshold; missing feed: {}",
                    missing.join(", ")
                )),
            );
        } else if !capacity_ok {
            reasons.insert(
                pillar.into(),
                json!(format!(
                    "Internal weight capacity insufficient for {pillar}"
                )),
            );
        }

        let base = json!({
            "pillar_id": pillar,
            "name": py_title(&pillar.replace('_', " ")),
            "coverage_pct": coverage_pct,
            "configured_weight": configured_weight,
            "valid_metrics": valid,
            "missing_metrics": missing,
            "capacity_gate_passed": capacity_ok,
        });
        let mut detail = base.as_object().cloned().expect("object");
        match eff.filter(|_| coverage_ok && !valid.is_empty()) {
            Some(eff) => {
                let mut weighted = 0.0;
                for m in &valid {
                    let v = obs(m).and_then(|o| o["value"].as_f64()).unwrap_or(0.0);
                    let w = eff
                        .iter()
                        .find(|(k, _)| k == m)
                        .map(|(_, w)| *w)
                        .unwrap_or(0.0);
                    weighted += w * (signal(m, v) as f64 / 2.0);
                }
                let p_score = py_round(100.0 * weighted, 1);
                let raw = py_round(configured_weight * p_score, 2);
                scores.push((pillar.into(), Some(p_score)));
                valid_pillars.push(pillar.into());
                detail.insert("score".into(), json!(p_score));
                detail.insert(
                    "effective_internal_weights".into(),
                    json!(eff
                        .into_iter()
                        .map(|(k, w)| (k, json!(w)))
                        .collect::<Map<String, Value>>()),
                );
                detail.insert("is_valid".into(), json!(true));
                detail.insert("raw_contribution".into(), json!(raw));
                detail.insert("exclusion_reason".into(), Value::Null);
            }
            None => {
                scores.push((pillar.into(), None));
                excluded.push(pillar);
                detail.insert("score".into(), Value::Null);
                detail.insert("effective_internal_weights".into(), json!({}));
                detail.insert("is_valid".into(), json!(false));
                detail.insert("raw_contribution".into(), Value::Null);
                detail.insert(
                    "exclusion_reason".into(),
                    reasons.get(pillar).cloned().unwrap_or(Value::Null),
                );
            }
        }
        details.insert(pillar.into(), Value::Object(detail));
    }

    let weight_of = |p: &str| {
        PILLAR_WEIGHTS
            .iter()
            .find(|(k, _)| *k == p)
            .map(|(_, w)| *w)
            .unwrap_or(0.0)
    };
    let effective = py_fsum(
        &valid_pillars
            .iter()
            .map(|p| weight_of(p))
            .collect::<Vec<_>>(),
    );
    let score_of = |p: &str| scores.iter().find(|(k, _)| k == p).and_then(|(_, s)| *s);

    let (mut composite, mut composite_rounded, mut candidate) =
        (Value::Null, Value::Null, Value::Null);
    let (mut regime_final, mut coverage_state) =
        ("insufficient evidence".to_string(), "insufficient_evidence");
    let (mut confirmed, mut supporting) = (false, 0usize);
    let has = |p: &str| valid_pillars.iter().any(|v| v == p);
    if effective >= 0.70 && has("liquidity") && has("ust_rates") {
        let contribs: Vec<f64> = valid_pillars
            .iter()
            .filter_map(|p| details[p.as_str()]["raw_contribution"].as_f64())
            .collect();
        let score = py_round(py_fsum(&contribs) / effective, 1);
        let cand = regime(score);
        composite = json!(score);
        composite_rounded = json!(py_round_int(score));
        candidate = json!(cand);
        coverage_state = if valid_pillars.len() == PILLAR_WEIGHTS.len() {
            "full"
        } else {
            "partial_comparable"
        };
        supporting = valid_pillars
            .iter()
            .filter(|p| match score_of(p) {
                None => false,
                Some(s) => match cand {
                    "Risk-on" | "Selective risk-on" => s > 0.0,
                    "Defensive" => s < 0.0,
                    "Neutral" => true,
                    _ => false,
                },
            })
            .count();
        confirmed = supporting >= 2;
        regime_final = if matches!(cand, "Risk-on" | "Defensive") && !confirmed {
            "insufficient evidence".into()
        } else {
            cand.into()
        };
    }

    json!({
        "coverage_state": coverage_state,
        "effective_valid_weight": py_round(effective * 100.0, 1),
        "effective_valid_weight_integer": py_round_int(effective * 100.0),
        "valid_pillar_ids": valid_pillars,
        "excluded_pillar_ids": excluded,
        "excluded_pillar_reasons": reasons,
        "configured_pillar_weights": PILLAR_WEIGHTS.iter().map(|(k, v)| (k.to_string(), json!(py_round(v * 100.0, 1)))).collect::<Map<_, _>>(),
        "pillar_scores": scores.into_iter().map(|(k, v)| (k, json!(v))).collect::<Map<_, _>>(),
        "pillar_coverage": coverage,
        "cross_pillar_confirmation": confirmed,
        "regime_candidate": candidate,
        "regime_final": regime_final,
        "score_vintage": score_vintage,
        "scoring_configuration_version": SCORING_CONFIG_VERSION,
        "composite_score": composite,
        "composite_score_rounded": composite_rounded,
        // MC emits `list(set(...))`, whose order is hash-randomized per process; sorted here.
        "blocking_feeds": blocking.into_iter().collect::<Vec<_>>(),
        "supporting_pillar_count": supporting,
        "pillar_details": details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_redistribution() {
        let energy = internal_weights("energy");
        let all =
            effective_weights(energy, &["brent_front_month", "brent_curve", "wti_spot"]).unwrap();
        assert_eq!(
            all.iter().map(|(_, w)| *w).collect::<Vec<_>>(),
            [0.4, 0.3, 0.3]
        );
        assert!(
            effective_weights(energy, &["brent_front_month", "wti_spot"]).is_none(),
            "2 of 3 metrics cannot carry 100% under a 40% cap"
        );
        let liq = effective_weights(internal_weights("liquidity"), &["vix"]).unwrap();
        assert_eq!(liq, vec![("vix".to_string(), 1.0)]);
        assert!(effective_weights(internal_weights("liquidity"), &[]).is_none());
    }

    #[test]
    fn regimes() {
        assert_eq!(regime(35.1), "Risk-on");
        assert_eq!(regime(35.0), "Selective risk-on");
        assert_eq!(regime(-15.0), "Neutral");
        assert_eq!(regime(-50.0), "Defensive");
        assert_eq!(regime(-50.1), "Liquidity stress");
    }
}
