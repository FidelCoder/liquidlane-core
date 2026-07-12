use anyhow::{Result, anyhow};
use uuid::Uuid;

use crate::domain::{LpPosition, PositionStatus};

pub(super) fn receipt_cell_id(id: Uuid) -> String {
    format!("ll-receipt-{id}")
}

pub(super) fn request_cell_id(id: Uuid) -> String {
    format!("ll-request-{id}")
}

pub(super) fn reserve_positions_with_fee(
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
        let taken = position.available_amount.min(amount);
        if taken == 0 {
            continue;
        }
        let fee_share = if amount == taken {
            undistributed_fee
        } else {
            lease_fee
                .saturating_mul(taken)
                .saturating_div(total_amount)
                .min(undistributed_fee)
        };

        position.available_amount -= taken;
        position.reserved_amount += taken;
        position.fees_earned += fee_share;
        position.updated_at = now;
        amount -= taken;
        undistributed_fee = undistributed_fee.saturating_sub(fee_share);
    }
    if amount > 0 {
        return Err(anyhow!("liquidity was just reserved by another request"));
    }
    Ok(())
}

pub(super) fn deploy_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    for position in active_positions_mut(positions, asset) {
        if amount == 0 {
            break;
        }
        let deployed = position.reserved_amount.min(amount);
        if deployed == 0 {
            continue;
        }
        position.reserved_amount -= deployed;
        position.deployed_amount += deployed;
        position.updated_at = now;
        amount -= deployed;
    }
    if amount > 0 {
        return Err(anyhow!(
            "reserved liquidity was already released or deployed"
        ));
    }
    Ok(())
}

pub(super) fn undeploy_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    for position in active_positions_mut(positions, asset) {
        if amount == 0 {
            break;
        }
        let returned = position.deployed_amount.min(amount);
        if returned == 0 {
            continue;
        }
        position.deployed_amount -= returned;
        position.reserved_amount += returned;
        position.updated_at = now;
        amount -= returned;
    }
    if amount > 0 {
        return Err(anyhow!(
            "deployed liquidity was already returned or withdrawn"
        ));
    }
    Ok(())
}

pub(super) fn release_positions(
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
        return Err(anyhow!(
            "reserved liquidity was already released or deployed"
        ));
    }
    Ok(())
}

pub(super) fn settle_positions(
    positions: &mut [LpPosition],
    asset: &str,
    mut amount: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    for position in active_positions_mut(positions, asset) {
        if amount == 0 {
            break;
        }
        let settled = position.deployed_amount.min(amount);
        if settled == 0 {
            continue;
        }
        position.deployed_amount -= settled;
        position.available_amount += settled;
        position.updated_at = now;
        amount -= settled;
    }
    if amount > 0 {
        return Err(anyhow!(
            "deployed liquidity was already settled or released"
        ));
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
