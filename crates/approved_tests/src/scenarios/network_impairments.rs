mod link;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use master_protocol::MemberId;
use net::transport::udp_session::RelayMailbox;
use net::{
    ClientActionInbox, ClientCommandInbox, CmdSeq, ContentFingerprint, HandshakeHello, MoveHistory,
    MoveRecord, ReliableEventHub, ReliableRow, ServerPacket, UdpAuthorityHub, UdpClientLink,
};
use serde_json::{Value, json};
use sim::{ClientId, Snapshot, Tick};

use link::{Link, Random, distribution};

const STEP_MS: u64 = 10;
const TICK_MS: u64 = net::AUTHORITY_MS as u64;
const RECOVERY_MS: u64 = 5000;
const DRAIN_MS: u64 = 1000;
const EVENT_INTERVAL_MS: u64 = 250;
const PEER: MemberId = MemberId([1; 16]);
const HOST: MemberId = MemberId([0; 16]);
const CLIENT: ClientId = ClientId(1);

#[derive(Clone, Copy, Debug)]
pub(super) struct Profile {
    name: &'static str,
    latency_ms: u64,
    loss_percent: u64,
    jitter_ms: u64,
    reorder_percent: u64,
    reorder_delay_ms: u64,
    blackout: Option<(u64, u64)>,
}

impl Profile {
    const CLEAN: Self = Self {
        name: "clean",
        latency_ms: 0,
        loss_percent: 0,
        jitter_ms: 0,
        reorder_percent: 0,
        reorder_delay_ms: 0,
        blackout: None,
    };

    fn json(self) -> Value {
        json!({"name": self.name, "one_way_latency_ms": self.latency_ms,
            "loss_percent": self.loss_percent, "jitter_plus_minus_ms": self.jitter_ms,
            "reorder_percent": self.reorder_percent, "reorder_delay_ms": self.reorder_delay_ms,
            "blackout_ms": self.blackout})
    }
}

fn profiles(duration: u64) -> Vec<Profile> {
    vec![
        Profile::CLEAN,
        Profile {
            name: "latency",
            latency_ms: 100,
            ..Profile::CLEAN
        },
        Profile {
            name: "loss",
            loss_percent: 5,
            ..Profile::CLEAN
        },
        Profile {
            name: "jitter",
            latency_ms: 100,
            jitter_ms: 80,
            ..Profile::CLEAN
        },
        Profile {
            name: "reorder",
            reorder_percent: 25,
            reorder_delay_ms: 150,
            ..Profile::CLEAN
        },
        Profile {
            name: "combined",
            latency_ms: 100,
            loss_percent: 10,
            jitter_ms: 60,
            reorder_percent: 20,
            reorder_delay_ms: 150,
            blackout: None,
        },
        Profile {
            name: "outage",
            blackout: Some((duration / 2, duration / 2 + 2000)),
            ..Profile::CLEAN
        },
    ]
}

struct Options {
    seed: u64,
    duration_ms: u64,
    selected: String,
    custom: Option<Profile>,
    output: Option<PathBuf>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut out = Options {
        seed: 1234,
        duration_ms: 30_000,
        selected: "all".into(),
        custom: None,
        output: None,
    };
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let raw = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        if flag == "--profile" {
            out.selected.clone_from(raw);
            continue;
        }
        if flag == "--output" {
            out.output = Some(raw.into());
            continue;
        }
        let value: u64 = raw
            .parse()
            .map_err(|_| format!("{flag}: expected an unsigned integer"))?;
        match flag.as_str() {
            "--seed" => out.seed = value,
            "--duration-ms" => out.duration_ms = value,
            "--latency-ms" | "--loss-percent" | "--jitter-ms" | "--reorder-percent"
            | "--reorder-delay-ms" => {
                let profile = out.custom.get_or_insert(Profile {
                    name: "custom",
                    reorder_delay_ms: 150,
                    ..Profile::CLEAN
                });
                match flag.as_str() {
                    "--latency-ms" => profile.latency_ms = value,
                    "--loss-percent" => profile.loss_percent = value,
                    "--jitter-ms" => profile.jitter_ms = value,
                    "--reorder-percent" => profile.reorder_percent = value,
                    _ => profile.reorder_delay_ms = value,
                }
            }
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    if !(5000..=120_000).contains(&out.duration_ms) || !out.duration_ms.is_multiple_of(TICK_MS) {
        return Err("--duration-ms must be a multiple of 50 in 5000..=120000".into());
    }
    if let Some(profile) = out.custom {
        if out.selected != "all" {
            return Err("custom impairments cannot be combined with --profile".into());
        }
        if profile.loss_percent > 100
            || profile.reorder_percent > 100
            || profile.latency_ms > 1000
            || profile.jitter_ms > 1000
            || profile.reorder_delay_ms > 1000
        {
            return Err("percentages must be 0..=100; delays must be 0..=1000 ms".into());
        }
    } else if out.selected != "all"
        && !profiles(out.duration_ms)
            .iter()
            .any(|p| p.name == out.selected)
    {
        return Err(
            "--profile must be all, clean, latency, loss, jitter, reorder, combined or outage"
                .into(),
        );
    }
    Ok(out)
}

pub fn run(root: &Path, args: &[String]) -> Result<bool, String> {
    if args.iter().any(|arg| arg == "--help") {
        println!(
            "network_impairments [--seed N] [--profile all|clean|latency|loss|jitter|reorder|combined|outage] \
            [--duration-ms 30000] [--output DIR]\nCustom profile: --latency-ms N --loss-percent N \
            --jitter-ms N --reorder-percent N --reorder-delay-ms N\nDelays are one-way. No game data, display, sockets or master required."
        );
        return Ok(true);
    }
    let opts = options(args)?;
    let profiles: Vec<Profile> = match opts.custom {
        Some(profile) => vec![profile],
        None => profiles(opts.duration_ms)
            .into_iter()
            .filter(|p| opts.selected == "all" || opts.selected == p.name)
            .collect(),
    };
    let directory = opts.output.unwrap_or_else(|| {
        root.join("iw4l-artifacts/approved-tests").join(format!(
            "{}-network_impairments-{}",
            crate::utc_stamp(),
            std::process::id()
        ))
    });
    std::fs::create_dir_all(&directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    let mut report = json!({"scenario": "network_impairments", "schema": 1,
        "seed": opts.seed, "active_ms": opts.duration_ms, "recovery_ms": RECOVERY_MS,
        "clock_step_ms": STEP_MS, "authority_tick_ms": TICK_MS,
        "fixture": {"players": 1, "entities_min": 128, "entities_max": 192,
            "events_per_second": 4, "resend_commands_per_packet": 16},
        "build": crate::report::build_facts(root, &std::env::current_exe().map_err(|e| e.to_string())?),
        "coverage": "production session ingress/fanout, commands, ACKs, snapshot codecs, compression, fragmentation and reliable control queues",
        "excluded": ["game simulation and prediction", "lobby/bootstrap admission", "actual QUIC loss recovery and congestion", "sockets, master relay, rendering and BO2 Zombies gameplay"],
        "control_transport": "ideal ordered/reliable messages; sampled losses add fixed 200 ms retries, not a QUIC implementation",
        "result": "running", "profiles": []});
    crate::report::write(&directory, &report)?;
    let mut passed = true;
    for profile in profiles {
        let result = match execute(profile, opts.seed, opts.duration_ms) {
            Ok(mut first) => {
                let deterministic = match execute(profile, opts.seed, opts.duration_ms) {
                    Ok(repeat) => {
                        let matches = first == repeat;
                        if !matches {
                            first["repeat_result"] = repeat;
                        }
                        matches
                    }
                    Err(error) => {
                        first["repeat_error"] = error.into();
                        false
                    }
                };
                first["deterministic_repeat"] = deterministic.into();
                if !deterministic {
                    first["passed"] = false.into();
                }
                first
            }
            Err(error) => json!({"profile": profile.json(), "passed": false, "error": error}),
        };
        let ok = result["passed"] == true;
        passed &= ok;
        if let Some(error) = result["error"].as_str() {
            println!("FAIL {}: {error}", profile.name);
        } else {
            println!(
                "{} {}: snapshots={} max_age_ms={} recovery_ms={}",
                if ok { "PASS" } else { "FAIL" },
                profile.name,
                result["snapshots_received"],
                result["snapshot_age_ms"]["max"],
                result["recovery_ms"]
            );
        }
        report["profiles"]
            .as_array_mut()
            .expect("profiles array")
            .push(result);
        crate::report::write(&directory, &report)?;
    }
    report["result"] = if passed { "passed" } else { "failed" }.into();
    crate::report::write(&directory, &report)?;
    println!(
        "{}: {}",
        if passed { "passed" } else { "failed" },
        directory.join("run.json").display()
    );
    Ok(passed)
}

#[derive(Default)]
struct Observations {
    snapshots: usize,
    active_snapshots: usize,
    baseline_snapshots: usize,
    stale_snapshots: usize,
    latest_tick: Option<u32>,
    last_arrival: Option<u64>,
    ages: Vec<u64>,
    gaps: Vec<u64>,
    command_ages: Vec<u64>,
    active_commands: usize,
    event_ages: Vec<u64>,
    max_command_queue: usize,
    max_history: usize,
    max_reliable: usize,
    event_count: u16,
    events_received: u16,
    recovery: Option<u64>,
}

fn fixture(tick: u32, state_tick: u32, command_time: i32) -> Snapshot {
    let mut snapshot = Snapshot::unpublished(Tick(tick));
    let mut player = sim::blank_player_state();
    player.command_time = command_time;
    player.origin = [command_time as f32, (command_time / 50 % 97) as f32, 64.0];
    player.health = 100 - (state_tick % 90) as i32;
    snapshot.players.push((CLIENT, player));
    snapshot.meta.match_elapsed_ms = tick * TICK_MS as u32;
    snapshot.meta.score_limit = 75;
    let mut random = Random::new(u64::from(state_tick));
    snapshot
        .meta
        .entities
        .resize_with(128 + (state_tick / 20 % 3) as usize * 32, Default::default);
    for (index, entity) in snapshot.meta.entities.iter_mut().enumerate() {
        entity.number = index as i32 + 64;
        entity.e_type = 1;
        entity.tr_time = state_tick as i32 * TICK_MS as i32;
        entity.tr_base = std::array::from_fn(|_| (random.next() % 4096) as f32);
        entity.event_sequence = (state_tick / 7) as i32;
    }
    snapshot
}

fn transfer(
    mailbox: &RelayMailbox,
    link: &mut Link,
    now: u64,
    profile: Profile,
) -> Result<usize, String> {
    let mut baseline_count = 0;
    for (_, bytes) in mailbox.take_outbound() {
        if let Ok(ServerPacket::Snapshot { baseline_seq, .. }) =
            net::decode_server_packet(&bytes, &net::ProtocolLimits::default_listen())
        {
            baseline_count += usize::from(baseline_seq != 0);
        }
        link.send(bytes, false, now, profile)?;
    }
    for (_, bytes) in mailbox.take_control_outbound() {
        link.send(bytes, true, now, profile)?;
    }
    Ok(baseline_count)
}

fn deliver(
    link: &mut Link,
    mailbox: &RelayMailbox,
    now: u64,
    epoch: Instant,
) -> Result<(), String> {
    for (stream, bytes) in link.deliver(now, epoch)? {
        if stream {
            mailbox
                .push_control_inbound(PEER, bytes)
                .map_err(str::to_owned)?;
        } else if mailbox.push_inbound(PEER, bytes) {
            return Err("production relay mailbox overflowed".into());
        }
    }
    Ok(())
}

fn fanout(
    hub: &mut UdpAuthorityHub,
    snapshot: &Snapshot,
    acks: &[(ClientId, CmdSeq)],
    reliable: &ReliableEventHub,
) -> Result<(), String> {
    hub.fanout_with_seats(
        snapshot,
        acks,
        |_, _| None,
        None,
        None,
        None,
        Some(reliable),
        false,
    )
    .map_err(|e| e.to_string())
}

fn execute(profile: Profile, seed: u64, duration: u64) -> Result<Value, String> {
    let epoch = Instant::now();
    let hello = HandshakeHello::current(ContentFingerprint::default());
    let host_mail = RelayMailbox::with_default_cap();
    let client_mail = RelayMailbox::with_default_cap();
    let mut hub = UdpAuthorityHub::relay(hello, CLIENT.0, host_mail.clone());
    hub.reconcile_relay_membership(&[HOST, PEER], HOST, &[]);
    let mut client = UdpClientLink::relay(hello, client_mail.clone());
    let mut commands = ClientCommandInbox::default();
    let mut actions = ClientActionInbox::default();
    let mut reliable = ReliableEventHub::default();
    let mut history = MoveHistory::new();
    let mut up = Link::new(seed);
    let mut down = Link::new(seed ^ 0xd1b5_4a32_d192_ed03);
    let mut expected = BTreeMap::new();
    let mut applied = BTreeSet::new();
    let mut observed = Observations::default();
    let initial = fixture(0, 0, 0);
    expected.insert(0, initial.clone());
    fanout(&mut hub, &initial, &[], &reliable)?;
    for (_, bytes) in host_mail.take_outbound() {
        let ServerPacket::Snapshot { header, .. } =
            net::decode_server_packet(&bytes, &hello.limits).map_err(|e| e.to_string())?
        else {
            return Err("missing initial snapshot".into());
        };
        client.note_accept(header.connection, CLIENT);
        down.send(bytes, false, 0, profile)?;
    }
    let last_send = duration + RECOVERY_MS - DRAIN_MS;
    for now in (0..=duration + RECOVERY_MS).step_by(STEP_MS as usize) {
        let current_profile = if now <= duration {
            profile
        } else {
            Profile::CLEAN
        };
        deliver(&mut up, &host_mail, now, epoch)?;
        deliver(&mut down, &client_mail, now, epoch)?;
        for received in client.recv_ticks()? {
            let snapshot = received.snapshot;
            let source = expected
                .get(&snapshot.tick.0)
                .ok_or("received an unknown snapshot tick")?;
            if snapshot.players != source.players
                || snapshot.meta.entities != source.meta.entities
                || snapshot.meta.match_elapsed_ms != source.meta.match_elapsed_ms
                || snapshot.meta.score_limit != source.meta.score_limit
                || received.frame.state_hash != net::compute_state_hash(&snapshot.players)
            {
                return Err(format!(
                    "snapshot {} changed across the wire",
                    snapshot.tick.0
                ));
            }
            observed.snapshots += 1;
            observed.active_snapshots += usize::from(now <= duration);
            observed
                .ages
                .push(now - u64::from(snapshot.tick.0) * TICK_MS);
            if let Some(last) = observed.last_arrival {
                observed.gaps.push(now - last);
            }
            observed.last_arrival = Some(now);
            if observed
                .latest_tick
                .is_some_and(|tick| snapshot.tick.0 <= tick)
            {
                observed.stale_snapshots += 1;
                continue;
            }
            observed.latest_tick = Some(snapshot.tick.0);
            if let Some(ack) = received.frame.ack_for(CLIENT)
                && matches!(history.clear_acknowledged(ack), net::AckMatch::Broken)
            {
                return Err("snapshot acknowledged a command outside retained history".into());
            }
        }
        for payload in client.take_controls() {
            if payload.dropped_oldest != 0 {
                return Err("reliable event overflow".into());
            }
            for (sequence, row) in payload.rows {
                if sequence != observed.events_received + 1 {
                    return Err("control events were duplicated, lost or reordered".into());
                }
                let ReliableRow::Scores(text) = row else {
                    return Err("unexpected control event".into());
                };
                let sent: u64 = text
                    .strip_prefix("event:")
                    .ok_or("wrong event payload")?
                    .parse()
                    .map_err(|_| "invalid event timestamp")?;
                if sent != u64::from(sequence) * EVENT_INTERVAL_MS {
                    return Err("control event payload changed".into());
                }
                observed.events_received = sequence;
                observed.event_ages.push(now - sent);
            }
        }
        if now.is_multiple_of(TICK_MS) {
            if now > 0 && now <= duration {
                let sequence = CmdSeq((now / TICK_MS) as u32);
                let mut cmd = net::idle_usercmd(now as i32);
                cmd.forwardmove = 127;
                if history.push(MoveRecord {
                    seq: sequence,
                    tick: Tick(sequence.0),
                    cmd,
                    input: sim::blank_player_state(),
                    output: sim::blank_player_state(),
                }) {
                    return Err("unacknowledged command history overflowed".into());
                }
            }
            if now <= last_send {
                let resend: Vec<_> = history
                    .iter()
                    .take(16)
                    .map(|record| (record.seq, record.cmd, sim::ShotSampleProvenance::NO_CLAIM))
                    .collect();
                client
                    .send_commands(&resend, &[], observed.events_received)
                    .map_err(|e| e.to_string())?;
            }
            observed.max_history = observed.max_history.max(history.len());
            hub.ingress(&mut commands, &mut actions, None, Some(&mut reliable), None)?;
            observed.max_command_queue = observed.max_command_queue.max(commands.pending(CLIENT));
            let gathered = commands.take_for_tick(now as i32);
            if !gathered.backlog_faults.is_empty() {
                return Err(format!("input backlog: {:?}", gathered.backlog_faults));
            }
            for command in &gathered.cmds {
                let sequence = command.sequence.ok_or("unexpected unsequenced command")?;
                if !applied.insert(sequence.0) {
                    return Err("authority applied a command twice".into());
                }
                if command.command.forwardmove != 127 {
                    return Err("command payload changed".into());
                }
                observed
                    .command_ages
                    .push(now - command.command.server_time as u64);
                observed.active_commands += usize::from(now <= duration);
            }
            if now > 0 && now <= duration && now.is_multiple_of(EVENT_INTERVAL_MS) {
                observed.event_count += 1;
                reliable
                    .queue_mut(CLIENT)
                    .push(ReliableRow::Scores(format!("event:{now}")));
            }
            observed.max_reliable = observed
                .max_reliable
                .max(reliable.queue_mut(CLIENT).pending_len());
            if now > 0 && now <= last_send {
                let command_time = commands
                    .last_consumed(CLIENT)
                    .map_or(0, |cmd| cmd.server_time);
                let snapshot = fixture(
                    (now / TICK_MS) as u32,
                    (now.min(duration) / TICK_MS) as u32,
                    command_time,
                );
                fanout(&mut hub, &snapshot, &gathered.acks, &reliable)?;
                expected.insert(snapshot.tick.0, snapshot);
            }
        }
        observed.baseline_snapshots += transfer(&host_mail, &mut down, now, current_profile)?;
        transfer(&client_mail, &mut up, now, current_profile)?;
        if now >= duration
            && observed.recovery.is_none()
            && history.is_empty()
            && observed.events_received == observed.event_count
            && reliable.queue_mut(CLIENT).pending_len() == 0
            && observed
                .latest_tick
                .is_some_and(|tick| tick >= (duration / TICK_MS) as u32)
        {
            observed.recovery = Some(now - duration);
        }
    }
    let down_report = down.report();
    let up_report = up.report();
    let mut assertions = Vec::new();
    let mut check =
        |name: &str, passed: bool| assertions.push(json!({"name": name, "passed": passed}));
    let blackout_ms = profile.blackout.map_or(0, |(start, end)| end - start);
    let progress_budget_ms = blackout_ms
        + 1000
        + 2 * (profile.latency_ms + profile.jitter_ms + profile.reorder_delay_ms);
    check(
        "snapshots_progress_during_impairment",
        observed.active_snapshots > 0,
    );
    check(
        "commands_progress_during_impairment",
        observed.active_commands > 1,
    );
    check(
        "snapshot_progress_bounded",
        observed.gaps.iter().all(|gap| *gap <= progress_budget_ms),
    );
    check(
        "command_age_bounded",
        observed
            .command_ages
            .iter()
            .all(|age| *age <= progress_budget_ms),
    );
    check("baseline_deltas_exercised", observed.baseline_snapshots > 0);
    check(
        "fragmented_snapshots_exercised",
        down_report["fragmented_packets"].as_u64().unwrap_or(0) > 0,
    );
    check(
        "final_snapshot_recovered",
        observed.latest_tick.is_some_and(|tick| {
            u64::from(tick) * TICK_MS >= last_send - 4 * TICK_MS
                && expected.get(&tick).is_some_and(|snapshot| {
                    snapshot.players[0].1.command_time == duration as i32
                        && snapshot.meta.entities
                            == fixture(tick, (duration / TICK_MS) as u32, duration as i32)
                                .meta
                                .entities
                })
        }),
    );
    check(
        "final_command_recovered",
        commands.last_acked_seq(CLIENT) == Some(CmdSeq((duration / TICK_MS) as u32)),
    );
    check("command_history_drained", history.is_empty());
    check(
        "all_events_delivered",
        observed.events_received == observed.event_count && observed.event_count > 0,
    );
    check(
        "reliable_queue_drained",
        reliable.queue_mut(CLIENT).pending_len() == 0,
    );
    check("input_queue_drained", commands.pending(CLIENT) == 0);
    check(
        "links_drained",
        down_report["pending_at_end"] == 0 && up_report["pending_at_end"] == 0,
    );
    check(
        "partial_reassemblies_expired",
        down_report["partial_reassemblies_at_end"] == false
            && up_report["partial_reassemblies_at_end"] == false,
    );
    check(
        "recovered_within_budget",
        observed
            .recovery
            .is_some_and(|ms| ms <= RECOVERY_MS - DRAIN_MS),
    );
    if profile.loss_percent > 0 || profile.blackout.is_some() {
        check(
            "loss_in_both_directions",
            down_report["datagram_drops"].as_u64().unwrap_or(0) > 0
                && up_report["datagram_drops"].as_u64().unwrap_or(0) > 0,
        );
    } else {
        check(
            "no_unconfigured_loss",
            down_report["datagram_drops"] == 0 && up_report["datagram_drops"] == 0,
        );
    }
    if profile.reorder_percent > 0 {
        check(
            "reordering_in_both_directions",
            down_report["reordered_datagrams"].as_u64().unwrap_or(0) > 0
                && up_report["reordered_datagrams"].as_u64().unwrap_or(0) > 0
                && down_report["delayed_for_reorder"].as_u64().unwrap_or(0) > 0
                && up_report["delayed_for_reorder"].as_u64().unwrap_or(0) > 0,
        );
    }
    if profile.reorder_percent == 0 && profile.jitter_ms == 0 && profile.latency_ms == 0 {
        check(
            "no_unconfigured_reordering",
            down_report["reordered_datagrams"] == 0 && up_report["reordered_datagrams"] == 0,
        );
    }
    if profile.jitter_ms > 0 {
        check(
            "jitter_exercised",
            down_report["impaired_datagram_delay_ms"]["min"]
                != down_report["impaired_datagram_delay_ms"]["max"],
        );
    }
    if profile.latency_ms > profile.jitter_ms {
        check(
            "latency_exercised",
            down_report["impaired_datagram_delay_ms"]["min"]
                .as_u64()
                .is_some_and(|ms| ms >= profile.latency_ms - profile.jitter_ms),
        );
    }
    Ok(
        json!({"profile": profile.json(), "passed": assertions.iter().all(|a| a["passed"] == true),
        "assertions": assertions, "snapshots_received": observed.snapshots,
        "stale_snapshots": observed.stale_snapshots, "baseline_snapshots_sent": observed.baseline_snapshots,
        "snapshot_age_ms": distribution(&observed.ages), "snapshot_arrival_gap_ms": distribution(&observed.gaps),
        "command_age_ms": distribution(&observed.command_ages), "commands_applied": applied.len(),
        "event_age_ms": distribution(&observed.event_ages), "events_delivered": observed.events_received,
        "max_command_queue": observed.max_command_queue, "max_command_history": observed.max_history,
        "max_reliable_queue": observed.max_reliable, "recovery_ms": observed.recovery,
        "progress_budget_ms": progress_budget_ms,
        "client_to_host": up_report, "host_to_client": down_report}),
    )
}
