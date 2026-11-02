//! Per-user, fail-closed risk validation.
//! Monetary amounts use integer USD cents; leverage and collateral ratios use
//! basis points. This crate approves or rejects exposure and never submits orders.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ABSOLUTE_MAX_OPEN_POSITIONS: usize = 8;
pub const BASIS_POINTS: u128 = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_open_positions: usize,
    pub max_position_notional_cents: u64,
    pub max_total_exposure_cents: u64,
    /// 10,000 bps = 1.0x portfolio leverage.
    pub max_portfolio_leverage_bps: u32,
    pub max_price_divergence_bps: u32,
    /// 2,000 bps = 20% minimum free-collateral buffer.
    pub min_free_collateral_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExistingPosition {
    pub market_address: String,
    pub notional_cents: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskRequest {
    pub user_id: String,
    pub market_symbol: String,
    pub market_address: String,
    pub market_is_verified_and_active: bool,
    pub requested_notional_cents: u64,
    pub required_collateral_cents: u64,
    pub available_collateral_cents: u64,
    pub account_equity_cents: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskOutcome {
    Approved {
        approved_notional_cents: u64,
        total_exposure_after_cents: u64,
    },
    Rejected { reason: RejectionReason },
    Deferred { reason: DeferralReason },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RiskConfigError {
    #[error("max_open_positions must be between 1 and 8")]
    InvalidPositionLimit,
    #[error("monetary and leverage limits must be positive")]
    InvalidNumericLimit,
    #[error("min_free_collateral_bps must be below 10000")]
    InvalidCollateralBuffer,
    #[error("max_price_divergence_bps must be between 0 and 1000")]
    InvalidDivergenceLimit,
}

impl RiskConfig {
    pub fn validate(&self) -> Result<(), RiskConfigError> {
        if !(1..=ABSOLUTE_MAX_OPEN_POSITIONS).contains(&self.max_open_positions) {
            return Err(RiskConfigError::InvalidPositionLimit);
        }
        if self.max_position_notional_cents == 0
            || self.max_total_exposure_cents == 0
            || self.max_portfolio_leverage_bps == 0
        {
            return Err(RiskConfigError::InvalidNumericLimit);
        }
        if self.min_free_collateral_bps >= 10_000 {
            return Err(RiskConfigError::InvalidCollateralBuffer);
        }
        if self.max_price_divergence_bps > 1000 {
            return Err(RiskConfigError::InvalidDivergenceLimit);
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
        || request.requested_notional_cents == 0
        || request.required_collateral_cents == 0
        || request.available_collateral_cents == 0
        || request.account_equity_cents == 0
        || request.existing_positions.iter().any(|p| p.market_address.trim().is_empty())
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
    if request.requested_notional_cents > config.max_position_notional_cents {
        return RiskOutcome::Rejected { reason: RejectionReason::PositionNotionalLimitExceeded };
    }

    let current_exposure = request.existing_positions.iter().try_fold(0_u64, |total, position| {
        total.checked_add(position.notional_cents)
    });
    let Some(current_exposure) = current_exposure else {
        return RiskOutcome::Rejected { reason: RejectionReason::TotalExposureLimitExceeded };
    };
    let Some(total_after) = current_exposure.checked_add(request.requested_notional_cents) else {
        return RiskOutcome::Rejected { reason: RejectionReason::TotalExposureLimitExceeded };
    };
    if total_after > config.max_total_exposure_cents {
        return RiskOutcome::Rejected { reason: RejectionReason::TotalExposureLimitExceeded };
    }

    let leverage_left = u128::from(total_after) * BASIS_POINTS;
    let leverage_right = u128::from(request.account_equity_cents)
        * u128::from(config.max_portfolio_leverage_bps);
    if leverage_left > leverage_right {
        return RiskOutcome::Rejected { reason: RejectionReason::PortfolioLeverageExceeded };
    }

    let Some(collateral_after) = request
        .available_collateral_cents
        .checked_sub(request.required_collateral_cents)
    else {
        return RiskOutcome::Rejected { reason: RejectionReason::InsufficientCollateralBuffer };
    };
    let required_buffer = (u128::from(request.available_collateral_cents)
        * u128::from(config.min_free_collateral_bps))
        / BASIS_POINTS;
    if u128::from(collateral_after) < required_buffer {
        return RiskOutcome::Rejected { reason: RejectionReason::InsufficientCollateralBuffer };
    }

    RiskOutcome::Approved {
        approved_notional_cents: request.requested_notional_cents,
        total_exposure_after_cents: total_after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> RiskConfig {
        RiskConfig {
            max_open_positions: 8,
            max_position_notional_cents: 200_000,
            max_total_exposure_cents: 1_000_000,
            max_portfolio_leverage_bps: 30_000,
            max_price_divergence_bps: 50,
            min_free_collateral_bps: 2_000,
        }
    }

    fn request() -> RiskRequest {
        RiskRequest {
            user_id: "user-1".into(),
            market_symbol: "BTC/USD".into(),
            market_address: "0xmarket".into(),
            market_is_verified_and_active: true,
            requested_notional_cents: 50_000,
            required_collateral_cents: 20_000,
            available_collateral_cents: 100_000,
            account_equity_cents: 500_000,
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
            notional_cents: 10_000,
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

    #[test]
    fn rejects_insufficient_collateral_without_float_rounding() {
        let mut req = request();
        req.required_collateral_cents = 80_001;
        assert_eq!(
            evaluate(&config(), &req),
            RiskOutcome::Rejected { reason: RejectionReason::InsufficientCollateralBuffer }
        );
    }
}
