//! The game's master chain, from its sound driver's master presets: the
//! preset the game's menus pick (else the driver's default) sets the two
//! submix effects' parameters each frame.

use bevy::prelude::*;

use crate::master_chain::MasterChainParameters;

fn update_master_mix(
    bank: Option<Res<crate::SoundBank>>,
    namespace: Option<Res<crate::ambient::SoundBankNamespace>>,
    preset: Option<Res<frame::MasterPreset>>,
    runtime: Res<crate::AudioRuntime>,
) {
    let chain = bank.zip(namespace).and_then(|(bank, namespace)| {
        let namespace: asset_core::AssetNamespace = namespace.namespace;
        let name = preset.and_then(|preset| preset.0).unwrap_or("default");
        let hash = asset_transport::sab::snd_hash_name(name);
        let preset = bank.0.master_preset(namespace, hash).or_else(|| {
            bank.0
                .master_preset(namespace, asset_transport::sab::snd_hash_name("default"))
        })?;
        Some((
            MasterChainParameters {
                master_bus: preset.master_bus,
                no_voice_bus: preset.no_voice_bus,
            },
            namespace,
        ))
    });
    if std::env::var_os("IW4L_PROBE").is_some() { static LAST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None); let now = format!("{:?}", chain.as_ref().map(|(p, ns)| (ns, p.master_bus, p.no_voice_bus))); let mut last = LAST.lock().unwrap(); if last.as_deref() != Some(now.as_str()) { eprintln!("PROBE master chain {now}"); *last = Some(now); } }
    runtime.set_master_chain(chain);
}

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        update_master_mix
            .in_set(frame::InMatch)
            .in_set(net::ClientSet::Effects),
    );
}
