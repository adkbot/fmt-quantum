//! Per-user, fail-closed risk validation.
//! This crate approves or rejects a proposed exposure; it never submits orders.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ABSOLUTE_MAX_OPEN_POSITIONS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_open_positions: usize,
    pub max_position_notional_usd: f64,
    pub max_total_exposure_usd: f64,
    pub max_portfolio_leverage: f64,
    pub max_price_divergence_bps: u32,
    /// Required free-collateral buffer, e.g. 0.20 means retain 20%.
    pub min_free_collateral_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExistingPosition {
    pub market_address: String,
    pub notional_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskRequest {
    pub user_id: String,
    pub market_symbol: String,
    pub market_address: String,
    pub market_is_verified_and_active: bool,
    pub requested_notional_usd: f64,
    pub required_collateral_usd: f64,
    pub available_collateral_usd: f64,
    pub account_equity_usd: f64,
    pub current_price_divergence_bps: u32,
    pub existing_positions: Vec<ExistingPosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectionReason {
    InvalidConfiguration,
    InvalidRequest,
    MarketNotVerifiedOrInactive,
    PriceDivergenceTooHigh,
    PositionNotionalLimitExceeded,
    TotalExposureLimitExceeded,
    PortfolioLeverageExceeded,
    InsufficientCollateralBuffer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeferralReason {
    MaximumConcurrentPositionsReached,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RiskOutcome {
    Approved { approved_notional_usd: f64, total_exposure_after_usd: f64 },
    Rejected { reason: RejectionReason },
    Deferred { reason: DeferralReason },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RiskConfigError {
    #[error("max_open_positions must be between 1 and 8")]
    InvalidPositionLimit,
    #[error("risk limits must be finite and positive")]
    InvalidNumericLimit,
    #[error("min_free_collateral_ratio must be in [0, 1)")]
    InvalidCollateralBuffer,
}

impl RiskConfig {
    pub fn validate(&self) -> Result<(), RiskConfigError> {
        if !(1..=ABSOLUTE_MAX_OPEN_POSITIONS).contains(&self.max_open_positions) {
            return Err(RiskConfigError::InvalidPositionLimit);
        }
        let positive = [
            self.max_position_notional_usd,
            self.max_total_exposure_usd,
            self.max_portfolio_leverage,
        ];
        if positive.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err(RiskConfigError::InvalidNumericLimit);
        }
        if !self.min_free_collateral_ratio.is_finite()
            || !(0.0..1.0).contains(&self.min_free_collateral_ratio)
        {
            return Err(RiskConfigError::InvalidCollateralBuffer);
        }
        Ok(())
    }
}

pub fn evaluate(config: &RiskConfig, request: &RiskRequest) -> RiskOutcome {
    if config.validate().is_err() {
        return RiskOutcome::Rejected { reason: RejectionReason::InvalidConfiguration };
    }
    if request.user_id.trim().is_empty()
        || request.market_symbol.trim().is_empty()
        || request.market_address.trim().is_empty()
        || !request.requested_notional_usd.is_finite()
        || request.requested_notional_usd <= 0.0
        || !request.required_collateral_usd.is_finite()
        || request.required_collateral_usd < 0.0
        || !request.available_collateral_usd.is_finite()
        || request.available_collateral_usd < 0.0
        || !request.account_equity_usd.is_finite()
        || request.account_equity_usd <= 0.0
        || request.existing_positions.iter().any(|p| {
            p.market_address.trim().is_empty()
                || !p.notional_usd.is_finite()
                || p.notional_usd < 0.0
        })
    {
        return RiskOutcome::Rejected { reason: RejectionReason::InvalidRequest };
    }
    if !request.market_is_verified_and_active {
        return RiskOutcome::Rejected { reason: RejectionReason::MarketNotVerifiedOrInactive };
    }
    if request.current_price_divergence_bps > config.max_price_divergence_bps {
        return RiskOutcome::Rejected { reason: RejectionReason::PriceDivergenceTooHigh };
    }

    // Never close an existing position to make room. Wait until one closes naturally.
    if request.existing_positions.len() >= config.max_open_positions {
        return RiskOutcome::Deferred {
            reason: DeferralReason::MaximumConcurrentPositionsReached,
        };
    }
    if request.requested_notional_usd > config.max_position_notional_usd {
        return RiskOutcome::Rejected { reason: RejectionReason::PositionNotionalLimitExceeded };
    }

    let current_exposure: f64 = request.existing_positions.iter().map(|p| p.notional_usd).sum();
    let total_after = current_exposure + request.requested_notional_usd;
    if !total_after.is_finite() || total_after > config.max_total_exposure_usd {
        return RiskOutcome::Rejected { reason: RejectionReason::TotalExposureLimitExceeded };
    }
    if total_after / request.account_equity_usd > config.max_portfolio_leverage {
        return RiskOutcome::Rejected { reason: RejectionReason::PortfolioLeverageExceeded };
    }

    let collateral_after = request.available_collateral_usd - request.required_collateral_usd;
    let required_buffer = request.available_collateral_usd * config.min_free_collateral_ratio;
    if collateral_after < required_buffer {
        return RiskOutcome::Rejected { reason: RejectionReason::InsufficientCollateralBuffer };
    }

    RiskOutcome::Approved {
        approved_notional_usd: request.requested_notional_usd,
        total_exposure_after_usd: total_after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> RiskConfig {
        RiskConfig {
            max_open_positions: 8,
            max_position_notional_usd: 2_000.0,
            max_total_exposure_usd: 10_000.0,
            max_portfolio_leverage: 3.0,
            max_price_divergence_bps: 50,
            min_free_collateral_ratio: 0.20,
        }
    }

    fn request() -> RiskRequest {
        RiskRequest {
            user_id: "user-1".into(),
            market_symbol: "BTC/USD".into(),
            market_address: "0xmarket".into(),
            market_is_verified_and_active: true,
            requested_notional_usd: 500.0,
            required_collateral_usd: 200.0,
            available_collateral_usd: 1_000.0,
            account_equity_usd: 5_000.0,
            current_price_divergence_bps: 10,
            existing_positions: vec![],
        }
    }

    #[test]
    fn approves_request_within_user_limits() {
        assert!(matches!(evaluate(&config(), &request()), RiskOutcome::Approved { .. }));
    }

    #[test]
    fn defers_instead_of_closing_a_position_when_eight_are_open() {
        let mut req = request();
        req.existing_positions = (0..8).map(|i| ExistingPosition {
            market_address: format!("0x{i}"),
            notional_usd: 100.0,
        }).collect();
        assert_eq!(
            evaluate(&config(), &req),
            RiskOutcome::Deferred { reason: DeferralReason::MaximumConcurrentPositionsReached }
        );
    }

    #[test]
    fn rejects_unverified_markets() {
        let mut req = request();
        req.market_is_verified_and_active = false;
        assert_eq!(
            evaluate(&config(), &req),
            RiskOutcome::Rejected { reason: RejectionReason::MarketNotVerifiedOrInactive }
        );
    }

    #[test]
    fn rejects_excessive_price_divergence() {
        let mut req = request();
        req.current_price_divergence_bps = 51;
        assert_eq!(
            evaluate(&config(), &req),
            RiskOutcome::Rejected { reason: RejectionReason::PriceDivergenceTooHigh }
        );
    }
}
