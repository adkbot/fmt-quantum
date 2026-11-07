//! Dual VWAP Sniper — independent signal module.
//!
//! Follows the direction of two VWAPs (macro = session/day-anchored, fast = rolling window),
//! waits for a pullback that tests the fast VWAP without losing the macro VWAP, then requires a
//! strong confirmation candle that closes back beyond the fast VWAP. Entries are only emitted on
//! CLOSED candles (no repaint, no future data); a 0-100 quality score must reach the minimum.
//!
//! This module does not touch the FMT engine or its VWAP calculation. It produces signals and
//! explicit rejection reasons only; order placement stays in the executor behind its own gates.

use chrono::{DateTime, Datelike, Utc};
use fmt_strategy::{Candle, Side};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct SniperParams {
    pub fast_n: usize,
    pub slope_k: usize,
    pub slope_min_bps: f64,
    pub lookback: u32,
    pub body_min: f64,
    pub range_atr_min: f64,
    pub buffer_atr: f64,
    pub max_dist_atr: f64,
    pub max_flips: usize,
    pub sustain: u32,
    pub min_score: f64,
    pub min_rr: f64,
    pub min_space_r: f64,
    pub min_risk_bps: f64,
    pub max_risk_bps: f64,
    /// Candles during which a direction stays quiet after a signal (no duplicate entries in the same move).
    pub cooldown_candles: u32,
}

impl Default for SniperParams {
    fn default() -> Self {
        Self {
            fast_n: 16,
            slope_k: 8,
            slope_min_bps: 3.0,
            lookback: 6,
            body_min: 0.5,
            range_atr_min: 0.7,
            buffer_atr: 0.3,
            max_dist_atr: 2.0,
            max_flips: 1,
            sustain: 3,
            min_score: 80.0,
            min_rr: 2.0,
            min_space_r: 0.5,
            min_risk_bps: 8.0,
            max_risk_bps: 250.0,
            cooldown_candles: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SniperSignal {
    pub side: Side,
    pub time: DateTime<Utc>,
    pub entry: f64,
    pub stop: f64,
    pub target1: f64,
    pub target2: f64,
    pub risk_bps: f64,
    pub rr_plan: f64,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SniperOutcome {
    Signal(SniperSignal),
    Rejected { side: Side, time: DateTime<Utc>, reason: &'static str },
}

#[derive(Debug, Clone, Default)]
struct Pullback {
    on: bool,
    ext: Option<f64>,
    age: u32,
    wick: f64,
}

impl Pullback {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug)]
pub struct SniperEngine {
    p: SniperParams,
    day: Option<(i32, u32)>,
    pv: f64,
    pv2: f64,
    vol: f64,
    tpv: Vec<f64>,
    vols: Vec<f64>,
    vm_hist: Vec<f64>,
    vf_hist: Vec<f64>,
    sgn_hist: Vec<i32>,
    trs: Vec<f64>,
    atr: Option<f64>,
    prev_close: Option<f64>,
    pb: [Pullback; 2],
    streak: [u32; 2],
    cooldown: [u32; 2],
}

fn clamp01(x: f64) -> f64 {
    x.clamp(f64::NEG_INFINITY, 1.0)
}

impl SniperEngine {
    pub fn new(params: SniperParams) -> Self {
        Self {
            p: params,
            day: None,
            pv: 0.0,
            pv2: 0.0,
            vol: 0.0,
            tpv: Vec::new(),
            vols: Vec::new(),
            vm_hist: Vec::new(),
            vf_hist: Vec::new(),
            sgn_hist: Vec::new(),
            trs: Vec::new(),
            atr: None,
            prev_close: None,
            pb: [Pullback::default(), Pullback::default()],
            streak: [0, 0],
            cooldown: [0, 0],
        }
    }

    /// Feed ONE closed candle (oldest first). Returns signals/rejections decided at this close.
    pub fn on_candle(&mut self, c: Candle) -> Vec<SniperOutcome> {
        let mut out = Vec::new();
        let p = self.p.clone();

        // Macro VWAP, anchored at the UTC day start (24/7 market), plus volume-weighted sigma for the bands.
        let day = (c.open_time.year(), c.open_time.ordinal());
        if self.day != Some(day) {
            self.day = Some(day);
            self.pv = 0.0;
            self.pv2 = 0.0;
            self.vol = 0.0;
        }
        let tp = (c.high + c.low + c.close) / 3.0;
        self.pv += tp * c.volume;
        self.pv2 += tp * tp * c.volume;
        self.vol += c.volume;
        let vwap_m = if self.vol > 0.0 { self.pv / self.vol } else { c.close };
        let sigma = if self.vol > 0.0 { (self.pv2 / self.vol - vwap_m * vwap_m).max(0.0).sqrt() } else { 0.0 };

        // Fast VWAP: rolling window of `fast_n` candles.
        self.tpv.push(tp * c.volume);
        self.vols.push(c.volume);
        if self.vols.len() > p.fast_n {
            self.tpv.remove(0);
            self.vols.remove(0);
        }
        let vsum: f64 = self.vols.iter().sum();
        let vwap_f = (self.vols.len() >= p.fast_n && vsum > 0.0).then(|| self.tpv.iter().sum::<f64>() / vsum);

        self.vm_hist.push(vwap_m);
        self.vf_hist.push(vwap_f.unwrap_or(vwap_m));
        if self.vm_hist.len() > p.slope_k + 1 {
            self.vm_hist.remove(0);
            self.vf_hist.remove(0);
        }
        if let Some(f) = vwap_f {
            self.sgn_hist.push(if f >= vwap_m { 1 } else { -1 });
            if self.sgn_hist.len() > 31 {
                self.sgn_hist.remove(0);
            }
        }

        for cd in &mut self.cooldown {
            *cd = cd.saturating_sub(1);
        }

        if let (Some(vwap_f), Some(atr)) = (vwap_f, self.atr) {
            if self.vm_hist.len() > p.slope_k && self.sgn_hist.len() >= 2 {
                let rng = c.high - c.low;
                let slope_m = (vwap_m - self.vm_hist[0]) / vwap_m * 10_000.0;
                let slope_f = (vwap_f - self.vf_hist[0]) / vwap_f * 10_000.0;
                let flips = self.sgn_hist.windows(2).filter(|w| w[0] != w[1]).count();

                for d in 0..2 {
                    let dir: f64 = if d == 0 { 1.0 } else { -1.0 };
                    let side = if d == 0 { Side::Buy } else { Side::Sell };

                    self.streak[d] = if dir * (c.close - vwap_m) > 0.0 && dir * (c.close - vwap_f) >= -atr {
                        self.streak[d] + 1
                    } else {
                        0
                    };
                    let aligned = dir * (vwap_f - vwap_m) > 0.0 && dir * slope_m >= p.slope_min_bps && dir * slope_f >= p.slope_min_bps;
                    if !aligned || dir * (c.close - vwap_m) <= 0.0 {
                        self.pb[d].reset();
                        continue;
                    }
                    if self.cooldown[d] > 0 {
                        continue;
                    }

                    // Confirmation candle after a retest of the fast VWAP.
                    let confirming = self.pb[d].on
                        && self.pb[d].age >= 1
                        && dir * (c.close - vwap_f) > 0.0
                        && dir * (c.close - c.open) > 0.0
                        && self.streak[d] >= p.sustain;
                    if confirming {
                        let verdict = (|| {
                            if rng <= 0.0 {
                                return Err("candle_sem_amplitude");
                            }
                            let body = (c.close - c.open).abs() / rng;
                            if body < p.body_min || rng < p.range_atr_min * atr {
                                return Err("corpo_ou_forca_insuficiente");
                            }
                            if flips > p.max_flips {
                                return Err("vwaps_cruzando_repetidamente");
                            }
                            if (c.close - vwap_f).abs() > p.max_dist_atr * atr {
                                return Err("preco_distante_da_vwap");
                            }
                            let entry = c.close;
                            let stop = self.pb[d].ext.ok_or("sem_fundo_do_reteste")? - dir * p.buffer_atr * atr;
                            let risk = (entry - stop).abs();
                            let risk_bps = risk / entry * 10_000.0;
                            if !(risk > 0.0 && dir * (entry - stop) > 0.0 && sigma > 0.0) {
                                return Err("stop_invalido");
                            }
                            if risk_bps < p.min_risk_bps || risk_bps > p.max_risk_bps {
                                return Err("stop_fora_do_risco_permitido");
                            }
                            let (mut t1, mut t2): (Option<f64>, Option<f64>) = (None, None);
                            for k in 1..=4 {
                                let band = vwap_m + dir * f64::from(k) * sigma;
                                if t1.is_none() && dir * (band - entry) >= p.min_space_r * risk {
                                    t1 = Some(band);
                                } else if t1.is_some() && t2.is_none() {
                                    t2 = Some(band);
                                }
                            }
                            let (Some(t1), Some(t2)) = (t1, t2) else { return Err("sem_espaco_ate_a_banda") };
                            let rr_plan = dir * (t2 - entry) / risk;
                            let space = dir * (t1 - entry) / risk;
                            if rr_plan < p.min_rr {
                                return Err("risco_retorno_menor_que_minimo");
                            }
                            let sep = dir * (vwap_f - vwap_m);
                            let score = 15.0 * clamp01(sep / (0.5 * atr))
                                + 15.0 * clamp01((dir * slope_m).min(dir * slope_f) / (2.0 * p.slope_min_bps))
                                + 10.0 * (1.0 - clamp01((entry - vwap_f).abs() / (p.max_dist_atr * atr)))
                                + 20.0 * clamp01(self.pb[d].wick / 0.6)
                                + 15.0 * clamp01(body / 0.8) * clamp01(rng / (1.2 * atr))
                                + 15.0 * clamp01(space / 1.5)
                                + 10.0 * clamp01(rr_plan / 3.0);
                            if score < p.min_score {
                                return Err("pontuacao_abaixo_do_minimo");
                            }
                            Ok(SniperSignal {
                                side,
                                time: c.open_time,
                                entry,
                                stop,
                                target1: t1,
                                target2: t2,
                                risk_bps,
                                rr_plan,
                                score,
                                reasons: vec![
                                    "vwaps_alinhadas_com_inclinacao".into(),
                                    format!("reteste_da_vwap_rapida_com_rejeicao_pavio_{:.2}", self.pb[d].wick),
                                    format!("candle_de_confirmacao_corpo_{body:.2}"),
                                    format!("espaco_ate_banda1_{space:.2}R"),
                                    format!("rr_ate_banda2_{rr_plan:.2}"),
                                ],
                            })
                        })();
                        match verdict {
                            Ok(signal) => {
                                out.push(SniperOutcome::Signal(signal));
                                self.pb[d].reset();
                                self.cooldown[d] = p.cooldown_candles;
                                continue;
                            }
                            Err(reason) => out.push(SniperOutcome::Rejected { side, time: c.open_time, reason }),
                        }
                    }

                    // Retest bookkeeping with THIS closed candle.
                    let touched = (d == 0 && c.low <= vwap_f) || (d == 1 && c.high >= vwap_f);
                    if touched {
                        let wick = if rng > 0.0 {
                            (if d == 0 { c.open.min(c.close) - c.low } else { c.high - c.open.max(c.close) }) / rng
                        } else {
                            0.0
                        };
                        let pb = &mut self.pb[d];
                        pb.on = true;
                        pb.age += 1;
                        pb.wick = pb.wick.max(wick);
                        pb.ext = Some(match pb.ext {
                            None => if d == 0 { c.low } else { c.high },
                            Some(e) => if d == 0 { e.min(c.low) } else { e.max(c.high) },
                        });
                    } else if self.pb[d].on {
                        self.pb[d].age += 1;
                    }
                    if self.pb[d].on && self.pb[d].age > p.lookback {
                        self.pb[d].reset();
                    }
                }
            }
        }

        // ATR(14) as a simple average, updated at the END of the candle (never uses the future).
        let tr = match self.prev_close {
            None => c.high - c.low,
            Some(pc) => (c.high - c.low).max((c.high - pc).abs()).max((c.low - pc).abs()),
        };
        self.trs.push(tr);
        if self.trs.len() > 14 {
            self.trs.remove(0);
        }
        self.atr = (self.trs.len() >= 14).then(|| self.trs.iter().sum::<f64>() / self.trs.len() as f64);
        self.prev_close = Some(c.close);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn load_fixture() -> Vec<Candle> {
        include_str!("../tests/fixture_btc_m15.csv")
            .lines()
            .map(|line| {
                let f: Vec<f64> = line.split(',').map(|x| x.parse().unwrap()).collect();
                Candle { open_time: Utc.timestamp_opt(f[0] as i64, 0).unwrap(), open: f[1], high: f[2], low: f[3], close: f[4], volume: f[5] }
            })
            .collect()
    }

    #[test]
    fn matches_the_sql_reference_on_real_btc_candles() {
        // Reference (backtest.sniper_run on the same window): first signal = SELL at 1770239700,
        // entry 73050.1, score 89.92, rr_plan 2.590, risk 128.9 bps.
        let mut engine = SniperEngine::new(SniperParams::default());
        let mut signals = Vec::new();
        for candle in load_fixture() {
            for outcome in engine.on_candle(candle) {
                if let SniperOutcome::Signal(s) = outcome {
                    signals.push(s);
                }
            }
        }
        let first = signals.first().expect("the SQL reference found a signal in this window");
        assert_eq!(first.side, Side::Sell);
        assert_eq!(first.time.timestamp(), 1_770_239_700);
        assert!((first.entry - 73_050.1).abs() < 1e-6);
        assert!((first.score - 89.92).abs() < 0.05, "score {}", first.score);
        assert!((first.rr_plan - 2.590).abs() < 0.01, "rr {}", first.rr_plan);
        assert!((first.risk_bps - 128.9).abs() < 0.2, "risk {}", first.risk_bps);
        assert!(first.stop > first.entry && first.target1 < first.entry && first.target2 < first.target1);
        assert!(first.score >= 80.0);
    }

    #[test]
    fn no_signal_on_a_flat_horizontal_market() {
        let mut engine = SniperEngine::new(SniperParams::default());
        for i in 0..400 {
            let t = Utc.timestamp_opt(1_770_000_000 + i * 900, 0).unwrap();
            let wiggle = if i % 2 == 0 { 0.5 } else { -0.5 };
            let c = Candle { open_time: t, open: 100.0, high: 101.0 + wiggle, low: 99.0 + wiggle, close: 100.0 + wiggle, volume: 10.0 };
            assert!(engine.on_candle(c).iter().all(|o| !matches!(o, SniperOutcome::Signal(_))));
        }
    }

    #[test]
    fn signals_use_only_closed_candles_in_order() {
        // Feeding candles one by one must equal feeding the same prefix: no look-ahead state.
        let all = load_fixture();
        let mut a = SniperEngine::new(SniperParams::default());
        let mut b = SniperEngine::new(SniperParams::default());
        let mut first_a = None;
        for (i, c) in all.iter().enumerate() {
            let oa = a.on_candle(*c);
            if first_a.is_none() && oa.iter().any(|o| matches!(o, SniperOutcome::Signal(_))) {
                first_a = Some(i);
            }
        }
        let cut = first_a.expect("signal exists");
        let mut got = false;
        for c in &all[..=cut] {
            got = b.on_candle(*c).iter().any(|o| matches!(o, SniperOutcome::Signal(_)));
        }
        assert!(got, "the signal must already exist at its own closing candle");
    }

    #[test]
    fn cooldown_prevents_duplicate_entries_in_the_same_move() {
        let mut engine = SniperEngine::new(SniperParams { cooldown_candles: 1_000, ..SniperParams::default() });
        let n = load_fixture().into_iter().flat_map(|c| engine.on_candle(c)).filter(|o| matches!(o, SniperOutcome::Signal(_))).count();
        assert!(n <= 2, "at most one signal per direction with an endless cooldown, got {n}");
    }
}
