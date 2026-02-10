/// Beacon chain period calculation utilities.
///
/// Period is calculated from slot using the following formula:
/// period = slot / (SLOTS_PER_EPOCH * EPOCHS_PER_SYNC_COMMITTEE_PERIOD)

// minimal preset
#[cfg(feature = "minimal")]
pub const SECONDS_PER_SLOT: u64 = 6;
#[cfg(feature = "minimal")]
pub const SLOTS_PER_EPOCH: u64 = 8;
#[cfg(feature = "minimal")]
pub const EPOCHS_PER_SYNC_COMMITTEE_PERIOD: u64 = 8;

// mainnet preset
#[cfg(not(feature = "minimal"))]
pub const SECONDS_PER_SLOT: u64 = 12;
#[cfg(not(feature = "minimal"))]
pub const SLOTS_PER_EPOCH: u64 = 32;
#[cfg(not(feature = "minimal"))]
pub const EPOCHS_PER_SYNC_COMMITTEE_PERIOD: u64 = 256;

/// Returns the number of slots per sync committee period.
pub const fn slots_per_period() -> u64 {
    SLOTS_PER_EPOCH * EPOCHS_PER_SYNC_COMMITTEE_PERIOD
}

/// Computes the epoch from a slot.
pub const fn compute_epoch(slot: u64) -> u64 {
    slot / SLOTS_PER_EPOCH
}

/// Computes the sync committee period from an epoch.
pub const fn compute_sync_committee_period(epoch: u64) -> u64 {
    epoch / EPOCHS_PER_SYNC_COMMITTEE_PERIOD
}

/// Computes the sync committee period from a slot.
pub const fn compute_period_from_slot(slot: u64) -> u64 {
    compute_sync_committee_period(compute_epoch(slot))
}

/// Computes the slot from a timestamp and genesis time.
/// Returns None if the timestamp is before genesis or not aligned to slot boundaries.
pub fn compute_slot_at_timestamp(timestamp: u64, genesis_time: u64) -> Option<u64> {
    if timestamp < genesis_time {
        return None;
    }
    let elapsed = timestamp - genesis_time;
    if elapsed % SECONDS_PER_SLOT != 0 {
        // Allow non-aligned timestamps by rounding down
        return Some(elapsed / SECONDS_PER_SLOT);
    }
    Some(elapsed / SECONDS_PER_SLOT)
}

/// Computes the period from a timestamp and genesis time.
pub fn compute_period_at_timestamp(timestamp: u64, genesis_time: u64) -> Option<u64> {
    let slot = compute_slot_at_timestamp(timestamp, genesis_time)?;
    Some(compute_period_from_slot(slot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slots_per_period() {
        #[cfg(feature = "minimal")]
        assert_eq!(slots_per_period(), 64); // 8 * 8
        #[cfg(not(feature = "minimal"))]
        assert_eq!(slots_per_period(), 8192); // 32 * 256
    }

    #[test]
    fn test_compute_epoch() {
        #[cfg(feature = "minimal")]
        {
            assert_eq!(compute_epoch(0), 0);
            assert_eq!(compute_epoch(7), 0);
            assert_eq!(compute_epoch(8), 1);
            assert_eq!(compute_epoch(16), 2);
        }
        #[cfg(not(feature = "minimal"))]
        {
            assert_eq!(compute_epoch(0), 0);
            assert_eq!(compute_epoch(31), 0);
            assert_eq!(compute_epoch(32), 1);
            assert_eq!(compute_epoch(64), 2);
        }
    }

    #[test]
    fn test_compute_sync_committee_period() {
        #[cfg(feature = "minimal")]
        {
            assert_eq!(compute_sync_committee_period(0), 0);
            assert_eq!(compute_sync_committee_period(7), 0);
            assert_eq!(compute_sync_committee_period(8), 1);
        }
        #[cfg(not(feature = "minimal"))]
        {
            assert_eq!(compute_sync_committee_period(0), 0);
            assert_eq!(compute_sync_committee_period(255), 0);
            assert_eq!(compute_sync_committee_period(256), 1);
        }
    }

    #[test]
    fn test_compute_period_from_slot() {
        #[cfg(feature = "minimal")]
        {
            // period = slot / 64
            assert_eq!(compute_period_from_slot(0), 0);
            assert_eq!(compute_period_from_slot(63), 0);
            assert_eq!(compute_period_from_slot(64), 1);
            assert_eq!(compute_period_from_slot(128), 2);
        }
        #[cfg(not(feature = "minimal"))]
        {
            // period = slot / 8192
            assert_eq!(compute_period_from_slot(0), 0);
            assert_eq!(compute_period_from_slot(8191), 0);
            assert_eq!(compute_period_from_slot(8192), 1);
            assert_eq!(compute_period_from_slot(13631487), 1663); // period 1663's last slot
            assert_eq!(compute_period_from_slot(13631488), 1664); // period 1664's first slot
        }
    }

    #[test]
    fn test_compute_slot_at_timestamp() {
        let genesis_time = 1000;

        // Before genesis
        assert!(compute_slot_at_timestamp(500, genesis_time).is_none());

        // At genesis
        assert_eq!(compute_slot_at_timestamp(1000, genesis_time), Some(0));

        #[cfg(feature = "minimal")]
        {
            // After genesis (6 seconds per slot)
            assert_eq!(compute_slot_at_timestamp(1006, genesis_time), Some(1));
            assert_eq!(compute_slot_at_timestamp(1012, genesis_time), Some(2));
            // Non-aligned timestamp (rounds down)
            assert_eq!(compute_slot_at_timestamp(1005, genesis_time), Some(0));
        }
        #[cfg(not(feature = "minimal"))]
        {
            // After genesis (12 seconds per slot)
            assert_eq!(compute_slot_at_timestamp(1012, genesis_time), Some(1));
            assert_eq!(compute_slot_at_timestamp(1024, genesis_time), Some(2));
        }
    }

    #[test]
    fn test_compute_period_at_timestamp() {
        let genesis_time = 0;

        #[cfg(feature = "minimal")]
        {
            // 64 slots per period, 6 seconds per slot = 384 seconds per period
            assert_eq!(compute_period_at_timestamp(0, genesis_time), Some(0));
            assert_eq!(compute_period_at_timestamp(383, genesis_time), Some(0));
            assert_eq!(compute_period_at_timestamp(384, genesis_time), Some(1));
        }
        #[cfg(not(feature = "minimal"))]
        {
            // 8192 slots per period, 12 seconds per slot = 98304 seconds per period
            assert_eq!(compute_period_at_timestamp(0, genesis_time), Some(0));
            assert_eq!(compute_period_at_timestamp(98303, genesis_time), Some(0));
            assert_eq!(compute_period_at_timestamp(98304, genesis_time), Some(1));
        }
    }
}
