//! Deterministic, close-confirmed implementation of the FMT session rules.
//!
//! Candle timestamps are UTC candle-open timestamps. The strategy evaluates each
//! candle once, in chronological order, using America/New_York session time.
//! This crate emits strategy events only; it never places orders.

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use chrono_tz::America::New_York;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MINUTES_PER_CANDLE: u32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    pub open_time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    /// Bybit linear perpetual kline volume, expressed in base-asset quantity.
    pub volume: f64,
}

impl Candle {
    pub fn validate(&self) -> Result<(), StrategyError> {
        let values = [self.open, self.high, self.low, self.close, self.volume];
        if values.iter().any(|v| !v.is_finite()) {
            return Err(StrategyError::InvalidCandle("non-finite OHLCV value"));
        }
        if self.volume < 0.0 {
            return Err(StrategyError::InvalidCandle("negative volume"));
        }
        if self.high < self.low
            || self.high < self.open
            || self.high < self.close
            || self.low > self.open
            || self.low > self.close
        {
            return Err(StrategyError::InvalidCandle("inconsistent OHLC values"));
        }
        let local = self.open_time.with_timezone(&New_York);
        if local.minute() % MINUTES_PER_CANDLE != 0 || local.second() != 0 {
            return Err(StrategyError::InvalidCandle(
                "timestamp is not aligned to a 15-minute candle",
            ));
        }
        Ok(())
    }

    fn typical_price(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StrategyEvent {
    Enter { side: Side, price: f64, time: DateTime<Utc> },
    Exit { side: Side, price: f64, time: DateTime<Utc>, reason: ExitReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitReason {
    PreSessionBoundaryReached,
    CloseCrossedSessionVwap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Flat,
    Open(Side),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StrategyError {
    #[error("invalid candle: {0}")]
    InvalidCandle(&'static str),
    #[error("candles must be supplied in strictly increasing timestamp order")]
    OutOfOrderCandle,
}

/// One engine instance must be used per strategy stream/account-symbol configuration.
#[derive(Debug, Clone)]
pub struct FmtEngine {
    last_open_time: Option<DateTime<Utc>>,
    previous_close: Option<f64>,
    pre_session_date: Option<NaiveDate>,
    pre_high: Option<f64>,
    pre_low: Option<f64>,
    pre_session_bars: u8,
    pre_session_complete: bool,
    pre_session_sequence_valid: bool,
    pre_session_last_time: Option<DateTime<Utc>>,
    vwap_date: Option<NaiveDate>,
    vwap_pv: f64,
    vwap_volume: f64,
    active_session: Option<NaiveDate>,
    position: Position,
}

impl Default for FmtEngine {
    fn default() -> Self {
        Self {
            last_open_time: None,
            previous_close: None,
            pre_session_date: None,
            pre_high: None,
            pre_low: None,
            pre_session_bars: 0,
            pre_session_complete: false,
            pre_session_sequence_valid: true,
            pre_session_last_time: None,
            vwap_date: None,
            vwap_pv: 0.0,
            vwap_volume: 0.0,
            active_session: None,
            position: Position::Flat,
        }
    }
}

impl FmtEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process one completed M15 candle. Call exactly once per candle.
    pub fn on_candle(&mut self, candle: Candle) -> Result<Vec<StrategyEvent>, StrategyError> {
        candle.validate()?;
        if self.last_open_time.is_some_and(|last| candle.open_time <= last) {
            return Err(StrategyError::OutOfOrderCandle);
        }

        let local = candle.open_time.with_timezone(&New_York);
        let date = local.date_naive();
        let minute_of_day = local.hour() * 60 + local.minute();
        let in_pre_session = (18 * 60..20 * 60).contains(&minute_of_day);
        let in_tokyo = minute_of_day >= 20 * 60 || minute_of_day < 6 * 60;

        let mut events = Vec::new();

        if in_pre_session {
            if self.pre_session_date != Some(date) {
                self.pre_session_date = Some(date);
                self.pre_high = None;
                self.pre_low = None;
                self.pre_session_bars = 0;
                self.pre_session_complete = false;
                self.pre_session_sequence_valid = true;
                self.pre_session_last_time = None;
                self.vwap_date = Some(date);
                self.vwap_pv = 0.0;
                self.vwap_volume = 0.0;
            }
            let contiguous = self.pre_session_last_time
                .map_or(true, |last| candle.open_time.signed_duration_since(last).num_minutes() == 15);
            if (self.pre_session_bars == 0 && minute_of_day != 18 * 60) || !contiguous {
                self.pre_session_sequence_valid = false;
            }
            self.pre_session_bars = self.pre_session_bars.saturating_add(1);
            self.pre_session_last_time = Some(candle.open_time);
            if self.pre_session_bars == 8 {
                // The first bar must be 18:00 and all eight M15 bars must be contiguous.
                self.pre_session_complete = self.pre_session_sequence_valid
                    && minute_of_day == 19 * 60 + 45;
            }
            self.pre_high = Some(self.pre_high.map_or(candle.high, |v| v.max(candle.high)));
            self.pre_low = Some(self.pre_low.map_or(candle.low, |v| v.min(candle.low)));
            self.accumulate_vwap(&candle);
        }

        if in_tokyo {
            let session_date = if minute_of_day < 6 * 60 {
                date.pred_opt().expect("valid previous calendar date")
            } else {
                date
            };

            if self.active_session != Some(session_date) {
                self.active_session = Some(session_date);
                self.position = Position::Flat;
                if self.vwap_date != Some(session_date) {
                    // Missing pre-session data must not silently reuse the prior day.
                    self.vwap_date = Some(session_date);
                    self.vwap_pv = 0.0;
                    self.vwap_volume = 0.0;
                }
            }

            self.accumulate_vwap(&candle);
            let session_vwap = if self.vwap_volume > 0.0 {
                Some(self.vwap_pv / self.vwap_volume)
            } else {
                None
            };
            let range_is_valid = self.pre_session_complete && self.pre_session_date == Some(session_date);
            let pre_high = self.pre_high.filter(|_| range_is_valid);
            let pre_low = self.pre_low.filter(|_| range_is_valid);

            // Pine-like sequential order: evaluate exits against the position that
            // existed before this candle, then evaluate entries on the same candle.
            if let (Some(vwap), Some(high), Some(low), Position::Open(side)) =
                (session_vwap, pre_high, pre_low, self.position)
            {
                let exit = match side {
                    Side::Buy if candle.high >= high => Some(ExitReason::PreSessionBoundaryReached),
                    Side::Buy if candle.close < vwap => Some(ExitReason::CloseCrossedSessionVwap),
                    Side::Sell if candle.low <= low => Some(ExitReason::PreSessionBoundaryReached),
                    Side::Sell if candle.close > vwap => Some(ExitReason::CloseCrossedSessionVwap),
                    _ => None,
                };
                if let Some(reason) = exit {
                    events.push(StrategyEvent::Exit {
                        side,
                        price: candle.close,
                        time: candle.open_time,
                        reason,
                    });
                    self.position = Position::Flat;
                }
            }

            if let (Some(vwap), Some(high), Some(low), Some(previous_close)) =
                (session_vwap, pre_high, pre_low, self.previous_close)
            {
                if self.position == Position::Flat {
                    let buy = candle.low <= low
                        && candle.close > low
                        && previous_close <= candle.close
                        && candle.close >= vwap;
                    let sell = candle.high >= high
                        && candle.close < high
                        && previous_close >= candle.close
                        && candle.close <= vwap;

                    if buy {
                        events.push(StrategyEvent::Enter {
                            side: Side::Buy,
                            price: candle.close,
                            time: candle.open_time,
                        });
                        self.position = Position::Open(Side::Buy);
                    } else if sell {
                        events.push(StrategyEvent::Enter {
                            side: Side::Sell,
                            price: candle.close,
                            time: candle.open_time,
                        });
                        self.position = Position::Open(Side::Sell);
                    }
                }
            }
        } else if minute_of_day >= 6 * 60 && minute_of_day < 18 * 60 {
            // The Pine indicator only resets its logical position at the next
            // Tokyo-session start; do not clear it at the 06:00 session end.
            self.active_session = None;
        }

        // Pine close[1] refers to the immediately preceding chart candle,
        // including the 19:45 pre-session candle on the first Tokyo bar.
        self.previous_close = Some(candle.close);
        self.last_open_time = Some(candle.open_time);
        Ok(events)
    }

    fn accumulate_vwap(&mut self, candle: &Candle) {
        self.vwap_pv += candle.typical_price() * candle.volume;
        self.vwap_volume += candle.volume;
    }

    pub fn is_in_position(&self) -> bool {
        matches!(self.position, Position::Open(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn candle(time: &str, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        let utc = DateTime::parse_from_rfc3339(time).unwrap().with_timezone(&Utc);
        Candle { open_time: utc, open, high, low, close, volume }
    }

    #[test]
    fn rejects_out_of_order_candles() {
        let mut engine = FmtEngine::new();
        let first = candle("2026-01-12T23:00:00Z", 100.0, 101.0, 99.0, 100.0, 10.0);
        let second = candle("2026-01-12T22:45:00Z", 100.0, 101.0, 99.0, 100.0, 10.0);
        engine.on_candle(first).unwrap();
        assert_eq!(engine.on_candle(second), Err(StrategyError::OutOfOrderCandle));
    }

    #[test]
    fn validates_15_minute_alignment_and_ohlc() {
        let mut engine = FmtEngine::new();
        let bad = candle("2026-01-12T23:07:00Z", 100.0, 101.0, 99.0, 100.0, 1.0);
        assert!(matches!(engine.on_candle(bad), Err(StrategyError::InvalidCandle(_))));
    }

    #[test]
    fn new_york_timezone_handles_winter_session_open() {
        let utc = Utc.with_ymd_and_hms(2026, 1, 13, 1, 0, 0).unwrap();
        let local = utc.with_timezone(&New_York);
        assert_eq!(local.hour(), 20);
        assert_eq!(local.minute(), 0);
    }
}
