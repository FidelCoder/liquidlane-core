use anyhow::{Result, anyhow};
use uuid::Uuid;

use crate::domain::{LpPosition, PositionStatus};

pub(super) fn receipt_cell_id(id: Uuid) -> String {
    format!("ll-receipt-{id}")
}

pub(super) fn request_cell_id(id: Uuid) -> String {
    format!("ll-request-{id}")
}

pub(super) fn reserve_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    for position in active_positions_mut(positions, asset) {
        if amount == 0 {
            break;
        }
        let taken = position.available_amount.min(amount);
        if taken == 0 {
            continue;
        }
        position.available_amount -= taken;
        position.reserved_amount += taken;
        position.updated_at = now;
        amount -= taken;
    }
    if amount > 0 {
        return Err(anyhow!("liquidity was just reserved by another request"));
    }
    Ok(())
}

pub(super) fn deploy_reserved_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    lease_fee: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let total_amount = amount.max(1);
    let mut undistributed_fee = lease_fee;

    for position in active_positions_mut(positions, asset) {
        if amount == 0 {
            break;
        }
        let moved = position.reserved_amount.min(amount);
        if moved == 0 {
            continue;
        }
        let fee_share = if amount == moved {
            undistributed_fee
        } else {
            lease_fee
                .saturating_mul(moved)
                .saturating_div(total_amount)
                .min(undistributed_fee)
        };

        position.reserved_amount -= moved;
        position.deployed_amount += moved;
        position.fees_earned += fee_share;
        position.updated_at = now;
        amount -= moved;
        undistributed_fee = undistributed_fee.saturating_sub(fee_share);
    }
    if amount > 0 {
        return Err(anyhow!("reserved liquidity accounting is incomplete"));
    }
    Ok(())
}

pub(super) fn release_reserved_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    for position in active_positions_mut(positions, asset) {
        if amount == 0 {
            break;
        }
        let released = position.reserved_amount.min(amount);
        if released == 0 {
            continue;
        }
        position.reserved_amount -= released;
        position.available_amount += released;
        position.updated_at = now;
        amount -= released;
    }
    if amount > 0 {
        return Err(anyhow!("reserved liquidity accounting is incomplete"));
    }
    Ok(())
}

fn active_positions_mut<'a>(
    positions: &'a mut [LpPosition],
    asset: &'a str,
) -> impl Iterator<Item = &'a mut LpPosition> {
    positions.iter_mut().filter(move |position| {
        position.asset == asset && position.status == PositionStatus::Active
    })
}
