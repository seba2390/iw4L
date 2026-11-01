use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use net::transport::fragment::{Fragmenter, Reassembler};
use net::transport::protocol::MAX_PACKET_BYTES;
use serde_json::{Value, json};

use super::Profile;

const DATAGRAM_BYTES: usize = master_protocol::MAX_OPAQUE_PAYLOAD;
const MAX_IN_FLIGHT: usize = 4096;
const MAX_IN_FLIGHT_BYTES: usize = 8 * 1024 * 1024;
const STREAM_RETRY_MS: u64 = 200;

pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut n = self.0;
        n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        n ^ (n >> 31)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

struct Delivery {
    sequence: u64,
    stream: bool,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct Stats {
    datagram_packets: u64,
    encoded_datagram_bytes: u64,
    attempts: u64,
    dropped: u64,
    datagram_attempts: u64,
    datagram_drops: u64,
    stream_attempts: u64,
    stream_retries: u64,
    delivered: u64,
    reordered: u64,
    delayed_for_reorder: u64,
    fragmented_packets: u64,
    completed_packets: u64,
    bytes_attempted: u64,
    max_queue: usize,
    max_queue_bytes: usize,
    delays: Vec<u64>,
    impaired_datagram_delays: Vec<u64>,
}

pub struct Link {
    random: Random,
    fragmenter: Fragmenter,
    reassembler: Reassembler,
    pending: BTreeMap<(u64, u64), Delivery>,
    queued_bytes: usize,
    next_sequence: u64,
    highest_datagram: u64,
    stream_due: u64,
    stats: Stats,
}

impl Link {
    pub fn new(seed: u64) -> Self {
        Self {
            random: Random::new(seed),
            fragmenter: Fragmenter::new(DATAGRAM_BYTES, MAX_PACKET_BYTES as usize),
            reassembler: Reassembler::new(DATAGRAM_BYTES, MAX_PACKET_BYTES as usize),
            pending: BTreeMap::new(),
            queued_bytes: 0,
            next_sequence: 0,
            highest_datagram: 0,
            stream_due: 0,
            stats: Stats::default(),
        }
    }

    pub fn send(
        &mut self,
        bytes: Vec<u8>,
        stream: bool,
        now: u64,
        profile: Profile,
    ) -> Result<(), String> {
        if stream {
            // This is the ordered/reliable stream contract, not QUIC's recovery
            // algorithm. Failed attempts add a fixed retry delay to that model.
            let mut attempt = now;
            for _ in 0..128 {
                self.stats.attempts += 1;
                self.stats.stream_attempts += 1;
                self.stats.bytes_attempted += bytes.len() as u64;
                if self.lost(attempt, profile) {
                    self.stats.dropped += 1;
                    self.stats.stream_retries += 1;
                    attempt += STREAM_RETRY_MS;
                    continue;
                }
                let due = (attempt + self.delay(profile)).max(self.stream_due);
                self.stream_due = due;
                return self.enqueue(bytes, true, due, now);
            }
            return Err("modeled reliable stream exhausted 128 retry attempts".into());
        }
        let fragments = self.fragmenter.split(&bytes).map_err(String::from)?;
        self.stats.datagram_packets += 1;
        self.stats.encoded_datagram_bytes += bytes.len() as u64;
        self.stats.fragmented_packets += u64::from(fragments.len() > 1);
        for fragment in fragments {
            self.stats.attempts += 1;
            self.stats.datagram_attempts += 1;
            self.stats.bytes_attempted += fragment.len() as u64;
            if self.lost(now, profile) {
                self.stats.dropped += 1;
                self.stats.datagram_drops += 1;
                continue;
            }
            let due = now + self.delay(profile);
            if profile.name != "clean" {
                self.stats.impaired_datagram_delays.push(due - now);
            }
            self.enqueue(fragment, false, due, now)?;
        }
        Ok(())
    }

    fn lost(&mut self, now: u64, profile: Profile) -> bool {
        profile
            .blackout
            .is_some_and(|(start, end)| (start..end).contains(&now))
            || self.random.chance(profile.loss_percent)
    }

    fn delay(&mut self, profile: Profile) -> u64 {
        let jitter =
            (self.random.next() % (2 * profile.jitter_ms + 1)) as i64 - profile.jitter_ms as i64;
        let mut delay = (profile.latency_ms as i64 + jitter).max(0) as u64;
        if self.random.chance(profile.reorder_percent) {
            delay += profile.reorder_delay_ms;
            self.stats.delayed_for_reorder += 1;
        }
        delay
    }

    fn enqueue(&mut self, bytes: Vec<u8>, stream: bool, due: u64, sent: u64) -> Result<(), String> {
        if self.pending.len() >= MAX_IN_FLIGHT
            || self.queued_bytes + bytes.len() > MAX_IN_FLIGHT_BYTES
        {
            return Err("simulated link exceeded its packet/byte capacity".into());
        }
        self.next_sequence += 1;
        self.queued_bytes += bytes.len();
        self.pending.insert(
            (due, self.next_sequence),
            Delivery {
                sequence: self.next_sequence,
                stream,
                bytes,
            },
        );
        self.stats.delays.push(due - sent);
        self.stats.max_queue = self.stats.max_queue.max(self.pending.len());
        self.stats.max_queue_bytes = self.stats.max_queue_bytes.max(self.queued_bytes);
        Ok(())
    }

    pub fn deliver(&mut self, now: u64, epoch: Instant) -> Result<Vec<(bool, Vec<u8>)>, String> {
        let mut packets = Vec::new();
        while self
            .pending
            .first_key_value()
            .is_some_and(|(key, _)| key.0 <= now)
        {
            let (_, delivery) = self.pending.pop_first().expect("due delivery exists");
            self.queued_bytes -= delivery.bytes.len();
            self.stats.delivered += 1;
            if delivery.stream {
                packets.push((true, delivery.bytes));
            } else {
                self.stats.reordered += u64::from(delivery.sequence < self.highest_datagram);
                self.highest_datagram = self.highest_datagram.max(delivery.sequence);
                if let Some(bytes) = self
                    .reassembler
                    .push_at(&delivery.bytes, epoch + Duration::from_millis(now))
                    .map_err(String::from)?
                {
                    self.stats.completed_packets += 1;
                    packets.push((false, bytes));
                }
            }
        }
        Ok(packets)
    }

    pub fn report(&self) -> Value {
        json!({
            "attempts": self.stats.attempts, "dropped_attempts": self.stats.dropped,
            "datagram_attempts": self.stats.datagram_attempts,
            "datagram_packets": self.stats.datagram_packets,
            "encoded_datagram_bytes": self.stats.encoded_datagram_bytes,
            "datagram_drops": self.stats.datagram_drops,
            "modeled_stream_attempts": self.stats.stream_attempts,
            "modeled_stream_retries": self.stats.stream_retries,
            "delivered_fragments_or_stream_messages": self.stats.delivered,
            "reordered_datagrams": self.stats.reordered,
            "delayed_for_reorder": self.stats.delayed_for_reorder,
            "fragmented_packets": self.stats.fragmented_packets,
            "completed_datagram_packets": self.stats.completed_packets,
            "bytes_attempted": self.stats.bytes_attempted,
            "max_in_flight": self.stats.max_queue,
            "max_in_flight_bytes": self.stats.max_queue_bytes,
            "scheduled_delay_ms": distribution(&self.stats.delays),
            "impaired_datagram_delay_ms": distribution(&self.stats.impaired_datagram_delays),
            "pending_at_end": self.pending.len(),
            "partial_reassemblies_at_end": !self.reassembler.is_empty(),
        })
    }
}

pub fn distribution(values: &[u64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let percentile = |p: usize| sorted[(sorted.len() * p).div_ceil(100).saturating_sub(1)];
    json!({"count": sorted.len(), "min": sorted[0], "p50": percentile(50),
        "p95": percentile(95), "p99": percentile(99), "max": sorted.last()})
}
