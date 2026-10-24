use super::*;

/// A curve's knots as a policy holds them, at most the engine's knot count.
fn pack(knots: &[(f32, f32)]) -> Arc<[[f32; 2]]> {
    knots
        .iter()
        .take(asset_iw4::SND_CURVE_MAX_KNOTS)
        .map(|&(x, y)| [x, y])
        .collect::<Vec<_>>()
        .into()
}

pub(super) fn native_curve(
    row: &CapturedAlias,
) -> Result<SpatialPlaybackPolicy, SpatialPolicyFailure> {
    let curve = row
        .volume_falloff
        .as_ref()
        .filter(|curve| !curve.knots.is_empty())
        .ok_or(SpatialPolicyFailure::MissingFalloffCurve)?;
    if !row.dist_min.is_finite()
        || !row.dist_max.is_finite()
        || curve
            .knots
            .iter()
            .chain(row.near_falloff.iter().flat_map(|near| near.knots.iter()))
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(SpatialPolicyFailure::InvalidFalloffCurve);
    }
    Ok(SpatialPlaybackPolicy {
        dist_min: row.dist_min,
        dist_max: row.dist_max,
        knots: pack(&curve.knots),
        near_knots: row.near_falloff.as_ref().map(|curve| pack(&curve.knots)),
    })
}

/// The alias's reverb send, with its falloff when both its curves resolve
/// to finite knots.
pub(super) fn reverb_send(row: &CapturedAlias) -> Option<ReverbSendPolicy> {
    let send = row.reverb_send.as_ref()?;
    let finite = |curve: &&crate::CapturedSndCurve| {
        !curve.knots.is_empty()
            && curve
                .knots
                .iter()
                .all(|(x, y)| x.is_finite() && y.is_finite())
    };
    let falloff = send
        .falloff
        .as_ref()
        .filter(finite)
        .zip(send.near_falloff.as_ref().filter(finite))
        .filter(|_| row.dist_min.is_finite() && send.dist_max.is_finite())
        .map(|(far, near)| SpatialPlaybackPolicy {
            dist_min: row.dist_min,
            dist_max: send.dist_max,
            knots: pack(&far.knots),
            near_knots: Some(pack(&near.knots)),
        });
    Some(ReverbSendPolicy {
        send: if send.send.is_finite() {
            send.send
        } else {
            0.0
        },
        falloff,
    })
}

pub(super) fn iw_speaker_gains(row: &CapturedAlias) -> Option<[[f32; 2]; 2]> {
    row.stereo_speaker_gains.filter(|gains| {
        gains
            .iter()
            .flatten()
            .all(|gain| gain.is_finite() && *gain >= 0.0)
    })
}
