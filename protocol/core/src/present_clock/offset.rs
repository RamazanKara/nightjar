//! Host-to-client video clock offset estimation from NTP-style clock sync exchanges (`0x5537`).

use crate::control::ClockSyncReply;

/// Clock sync samples kept, about 8 s at four probes a second.
const SYNC_SAMPLES: usize = 32;

#[derive(Clone, Copy, Debug, Default)]
struct SyncSample {
    local_us: i64,
    offset_us: i64,
    rtt_us: i64,
}

/// The offset from the host's video clock to the client's clock (`client = host + offset`),
/// from NTP-style clock sync exchanges.
#[derive(Clone, Debug, Default)]
pub struct ClockOffset {
    samples: [SyncSample; SYNC_SAMPLES],
    len: usize,
    next: usize,
}

impl ClockOffset {
    /// Adds the exchange `reply` answered, received at `client_receive_us`. Returns its round
    /// trip, or `None` for an exchange that cannot be right. An offset more than a second
    /// away from the current one means the host restarted its video clock: the older
    /// samples are dropped.
    pub fn add(&mut self, reply: &ClockSyncReply, client_receive_us: u64) -> Option<i64> {
        let (Ok(t1), Ok(t2), Ok(t3), Ok(t4)) = (
            i64::try_from(reply.client_us),
            i64::try_from(reply.host_receive_us),
            i64::try_from(reply.host_send_us),
            i64::try_from(client_receive_us),
        ) else {
            return None;
        };
        if t4 < t1 {
            return None;
        }
        // Compute in i128 without 128-bit division (32-bit Android runtime has no __divti3).
        let rtt128 = (i128::from(t4) - i128::from(t1)) - (i128::from(t3) - i128::from(t2));
        if !(0..=1_000_000).contains(&rtt128) {
            return None;
        }
        let rtt = rtt128 as i64;
        let sum = (i128::from(t1) - i128::from(t2)) + (i128::from(t4) - i128::from(t3));
        let offset128 = (sum + if sum < 0 { sum & 1 } else { 0 }) >> 1;
        let Ok(offset) = i64::try_from(offset128) else {
            return None;
        };
        if self
            .offset_at(t4)
            .is_some_and(|current| offset.abs_diff(current) > 1_000_000)
        {
            self.clear();
        }
        let local_sum = i128::from(t1) + i128::from(t4);
        let local_us = ((local_sum + if local_sum < 0 { local_sum & 1 } else { 0 }) >> 1) as i64;
        self.samples[self.next] = SyncSample {
            local_us,
            offset_us: offset,
            rtt_us: rtt,
        };
        self.next = (self.next + 1) % SYNC_SAMPLES;
        self.len = (self.len + 1).min(SYNC_SAMPLES);
        Some(rtt)
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
    }

    fn live(&self) -> &[SyncSample] {
        &self.samples[..self.len]
    }

    fn best(samples: impl Iterator<Item = SyncSample>) -> Option<SyncSample> {
        samples.min_by_key(|s| s.rtt_us)
    }

    /// The round trip of the exchange the offset comes from.
    pub fn rtt_us(&self) -> Option<i64> {
        Self::best(self.live().iter().copied()).map(|s| s.rtt_us)
    }

    /// The offset at client time `local_us`: the exchange with the shortest round trip
    /// (whose error is at most half of it), carried forward by the drift between the best
    /// exchanges of the older and the newer half of the samples.
    pub fn offset_at(&self, local_us: i64) -> Option<i64> {
        let live = self.live();
        let best = Self::best(live.iter().copied())?;
        let (oldest, newest) = live.iter().fold((i64::MAX, i64::MIN), |(lo, hi), s| {
            (lo.min(s.local_us), hi.max(s.local_us))
        });
        let middle = oldest.saturating_add(newest.saturating_sub(oldest) / 2);
        let older = Self::best(live.iter().copied().filter(|s| s.local_us < middle));
        let newer = Self::best(live.iter().copied().filter(|s| s.local_us >= middle));
        // Parts per billion, at most 1000 ppm; quartz is within 100 ppm. No i128: 32-bit
        // Android has no 128-bit division in its runtime. The offsets differ by under a
        // second (a larger jump clears the samples), so the products fit in an i64.
        let drift_ppb = match (older, newer) {
            (Some(a), Some(b)) if b.local_us.saturating_sub(a.local_us) >= 2_000_000 => {
                let dt = b.local_us.saturating_sub(a.local_us);
                (b.offset_us
                    .saturating_sub(a.offset_us)
                    .clamp(-1_000_000, 1_000_000)
                    * 1_000_000_000
                    / dt)
                    .clamp(-1_000_000, 1_000_000)
            }
            _ => 0,
        };
        // A day either way keeps the product within an i64.
        let elapsed = local_us
            .saturating_sub(best.local_us)
            .clamp(-86_400_000_000, 86_400_000_000);
        Some(
            best.offset_us
                .saturating_add(elapsed * drift_ppb / 1_000_000_000),
        )
    }
}
