use super::*;

fn detected(origin: [f32; 3], forward: [f32; 3], target: [f32; 3]) -> bool {
    let offset = Vec3::from_array(target) - Vec3::from_array(origin);
    if offset.x * offset.x + offset.y * offset.y > 96.0 * 96.0 || offset.z.abs() > 96.0 {
        return false;
    }
    let direction = offset + Vec3::Z * 32.0;
    let forward = Vec3::from_array(forward);
    direction.dot(forward) >= 20.0
        && direction.normalize_or_zero().dot(forward) > 70.0_f32.to_radians().cos()
}

pub(super) fn advance(world: &mut World, state: &Survival, tick: Tick) {
    let frame = FrameWorld::from_world(world);
    let mut mines = Vec::new();
    frame.visit_projectiles(|projectile| {
        if projectile.live
            && projectile.pos.tr_type == entity_iw4::TR_STATIONARY
            && projectile.detonate_at_ms.is_none()
            && frame.weapon_script_name(projectile.weapon) == "claymore_zm"
        {
            mines.push(*projectile);
        }
    });
    drop(frame);
    for mine in mines {
        let angles = entity_iw4::evaluate_trajectory(&mine.apos, crate::level_time_ms(tick));
        let forward = math_iw4::angle_vectors(angles).0;
        let target = state.actors.iter().find_map(|(&object, actor)| {
            (detected(mine.origin, forward, actor.origin)
                && super::super::natives::engine::damage_visible(world, object, mine.origin))
            .then_some(object)
        });
        if let Some(target) = target {
            let mut frame = FrameWorld::from_world(world);
            if let Some(projectile) = frame.projectile_mut_by_number(mine.entnum) {
                projectile.detonate_at_ms = Some(crate::level_time_ms(tick).saturating_add(400));
            }
            drop(frame);
            let _ = super::super::natives::engine::play_sound_at(
                world,
                mine.origin,
                "wpn_claymore_alert",
            );
            diag::info!(
                Sim,
                "zombies mine triggered projectile={} target={target} delay_ms=400",
                mine.id.0
            );
        }
    }
}
