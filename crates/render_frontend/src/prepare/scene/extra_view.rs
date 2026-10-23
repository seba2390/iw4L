//! A game's extra camera: a second view of the world drawn into a target
//! its materials sample (`game_api::ExtraCameraRules`).
//!
//! The view culls, lists and composes its own camera products. The frame's
//! view resources are swapped for the extra view's own, the same frontend
//! systems run once more in [`ExtraViewPass`], and the main view's are put
//! back. The extra view draws the world and its static models: no sun or spot
//! shadows, no viewmodel, no effects and no post chain.

use std::sync::Arc;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use render_scene::{PreparedSceneView, PreparedSunShadowFrame, PublishedCellVis, RZnearDvar};

use crate::assemble::drawsurf::{
    CameraProducts, DrawSurfList, FrameAssemblyInputs, FrameProductsSnapshot, FxDrawLane,
    MaterialExecFrame, MaterialFrameInputs, RenderFrameProducts, SpotProduct, SpotShadowCasterPlan,
    SpotShadowMapLights, StaticDrawLane, SunProduct, SunShadowMapPresent, SunShadowUnmatchedLights,
    XModelDrawLane,
};
use crate::prepare::scene::cull::DpvsFrameStats;
use crate::prepare::scene::smodel_geom_cache::{PretessDvar, SmcEnableDvar};
use crate::prepare::scene::world::{WorldCull, WorldDrawItem, WorldScene};

/// The frontend systems the extra view runs, in the main view's order.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExtraViewPass;

/// The extra view's camera products and material frame this frame, for the
/// seal; `products` is `None` while no extra camera is on.
#[derive(Resource, Default)]
pub struct ExtraViewFrame {
    pub products: Option<Arc<FrameProductsSnapshot>>,
    pub exec_frame: MaterialExecFrame,
    pub frame_id: u64,
    pub size: [u32; 2],
}

/// The view fields the cull writes into the world's cull for one view.
#[derive(Default)]
struct CullView {
    surface_vis: Vec<u8>,
    smodel_vis: Vec<u8>,
    smodel_gump_hidden: Vec<bool>,
    draw_items: Vec<WorldDrawItem>,
    draw_items_id: u64,
    g0_surfs: Vec<u16>,
    cell_vis: Vec<u32>,
    cell_vis_all: bool,
}

impl CullView {
    fn swap(&mut self, cull: &mut WorldCull) {
        std::mem::swap(&mut self.surface_vis, &mut cull.surface_vis);
        std::mem::swap(&mut self.smodel_vis, &mut cull.smodel_vis);
        std::mem::swap(&mut self.smodel_gump_hidden, &mut cull.smodel_gump_hidden);
        std::mem::swap(&mut self.draw_items, &mut cull.draw_items);
        std::mem::swap(&mut self.draw_items_id, &mut cull.draw_items_id);
        std::mem::swap(&mut self.g0_surfs, &mut cull.g0_surfs);
        std::mem::swap(&mut self.cell_vis, &mut cull.cell_vis);
        std::mem::swap(&mut self.cell_vis_all, &mut cull.cell_vis_all);
    }
}

/// The extra view's own copy of every resource a view writes, kept across
/// frames so its retained lists and products reuse as the main view's do.
pub struct ExtraViewState {
    view: PreparedSceneView,
    mat_frame: MaterialFrameInputs,
    assembly: FrameAssemblyInputs,
    dpvs: DpvsFrameStats,
    cell_vis: PublishedCellVis,
    cull: CullView,
    list: DrawSurfList,
    static_lane: StaticDrawLane,
    xmodel_lane: XModelDrawLane,
    fx_lane: FxDrawLane,
    camera: CameraProducts,
    sun: SunProduct,
    spot: SpotProduct,
    products: RenderFrameProducts,
    spot_casters: SpotShadowCasterPlan,
    spot_lights: SpotShadowMapLights,
    sun_present: SunShadowMapPresent,
    sun_unmatched: SunShadowUnmatchedLights,
    sun_frame: PreparedSunShadowFrame,
    smc: SmcEnableDvar,
    pretess: PretessDvar,
}

impl Default for ExtraViewState {
    fn default() -> Self {
        Self {
            view: PreparedSceneView::default(),
            mat_frame: MaterialFrameInputs::default(),
            assembly: FrameAssemblyInputs::default(),
            dpvs: DpvsFrameStats::default(),
            cell_vis: PublishedCellVis::default(),
            cull: CullView::default(),
            list: DrawSurfList::default(),
            static_lane: StaticDrawLane::default(),
            xmodel_lane: XModelDrawLane::default(),
            fx_lane: FxDrawLane::default(),
            camera: CameraProducts::default(),
            sun: SunProduct::default(),
            spot: SpotProduct::default(),
            products: RenderFrameProducts::default(),
            spot_casters: SpotShadowCasterPlan::default(),
            spot_lights: SpotShadowMapLights::default(),
            sun_present: SunShadowMapPresent(false),
            sun_unmatched: SunShadowUnmatchedLights::default(),
            sun_frame: PreparedSunShadowFrame(None),
            // The extra view's static models draw from the map's own static
            // model geometry: the static model cache and its pretessellated
            // indices are the main view's.
            smc: SmcEnableDvar {
                enabled: Some(false),
            },
            pretess: PretessDvar { enabled: false },
        }
    }
}

fn swap_resource<T>(world: &mut World, held: &mut T) -> bool
where
    T: Resource + bevy::ecs::component::Component<Mutability = bevy::ecs::component::Mutable>,
{
    match world.get_resource_mut::<T>() {
        Some(mut live) => {
            std::mem::swap(&mut *live, held);
            true
        }
        None => false,
    }
}

impl ExtraViewState {
    /// Swaps every view resource between the world and the extra view.
    /// Returns false (with nothing swapped) when the world lacks one.
    fn swap(&mut self, world: &mut World) -> bool {
        let ready = world.contains_resource::<PreparedSceneView>()
            && world.contains_resource::<MaterialFrameInputs>()
            && world.contains_resource::<FrameAssemblyInputs>()
            && world.contains_resource::<DpvsFrameStats>()
            && world.contains_resource::<PublishedCellVis>()
            && world.contains_resource::<DrawSurfList>()
            && world.contains_resource::<StaticDrawLane>()
            && world.contains_resource::<XModelDrawLane>()
            && world.contains_resource::<FxDrawLane>()
            && world.contains_resource::<CameraProducts>()
            && world.contains_resource::<SunProduct>()
            && world.contains_resource::<SpotProduct>()
            && world.contains_resource::<RenderFrameProducts>()
            && world.contains_resource::<SpotShadowCasterPlan>()
            && world.contains_resource::<SpotShadowMapLights>()
            && world.contains_resource::<SunShadowMapPresent>()
            && world.contains_resource::<SunShadowUnmatchedLights>()
            && world.contains_resource::<PreparedSunShadowFrame>()
            && world.contains_resource::<SmcEnableDvar>()
            && world.contains_resource::<PretessDvar>()
            && world
                .get_resource::<WorldScene>()
                .is_some_and(|scene| scene.cull.is_some());
        if !ready {
            return false;
        }
        swap_resource(world, &mut self.view);
        swap_resource(world, &mut self.mat_frame);
        swap_resource(world, &mut self.assembly);
        swap_resource(world, &mut self.dpvs);
        swap_resource(world, &mut self.cell_vis);
        swap_resource(world, &mut self.list);
        swap_resource(world, &mut self.static_lane);
        swap_resource(world, &mut self.xmodel_lane);
        swap_resource(world, &mut self.fx_lane);
        swap_resource(world, &mut self.camera);
        swap_resource(world, &mut self.sun);
        swap_resource(world, &mut self.spot);
        swap_resource(world, &mut self.products);
        swap_resource(world, &mut self.spot_casters);
        swap_resource(world, &mut self.spot_lights);
        swap_resource(world, &mut self.sun_present);
        swap_resource(world, &mut self.sun_unmatched);
        swap_resource(world, &mut self.sun_frame);
        swap_resource(world, &mut self.smc);
        swap_resource(world, &mut self.pretess);
        let mut scene = world.resource_mut::<WorldScene>();
        if let Some(cull) = scene.cull.as_mut() {
            self.cull.swap(cull);
        }
        true
    }
}

/// The extra camera the presented client has on, with the game's rules for
/// its target.
fn requested_camera(world: &World) -> Option<(game_api::ExtraCamera, [u32; 2])> {
    let rules = world.get_resource::<game_api::ModeRules>()?.extra_camera?;
    let presented = world.get_resource::<net::PresentedSnapshot>()?;
    let local = world.get_resource::<net::LocalPresentClient>()?;
    let camera = presented
        .snapshot()?
        .meta
        .for_client(local.0)?
        .view_effects
        .client_screen
        .extra_camera?;
    (rules.size[0] > 0 && rules.size[1] > 0).then_some((camera, rules.size))
}

/// Runs the extra camera's view after the main view's products are
/// published, and keeps its products and material frame for the seal.
pub fn run_extra_view(world: &mut World, mut state: Local<Option<Box<ExtraViewState>>>) {
    let request = requested_camera(world);
    let main_ready = world
        .get_resource::<PreparedSceneView>()
        .is_some_and(|view| view.ready);
    let Some((camera, size)) = request.filter(|_| main_ready) else {
        if let Some(mut frame) = world.get_resource_mut::<ExtraViewFrame>() {
            frame.products = None;
        }
        return;
    };
    let view = {
        let main = world.resource::<PreparedSceneView>();
        let znear = world.resource::<RZnearDvar>();
        crate::prepare::scene::view_parms::extra_scene_view(
            camera.origin,
            camera.angles,
            camera.fov,
            size,
            main,
            znear,
        )
    };
    let state = state.get_or_insert_with(Box::default);
    state.view = view;
    if !state.swap(world) {
        return;
    }
    world.run_schedule(ExtraViewPass);
    let products = world.resource::<RenderFrameProducts>().published();
    let frame_id = world.resource::<FrameAssemblyInputs>().frame_id;
    let mut exec_frame = world
        .get_resource::<ExtraViewFrame>()
        .map(|frame| frame.exec_frame.clone())
        .unwrap_or_else(MaterialExecFrame::default);
    crate::assemble::drawsurf::material_exec::refresh(
        &mut exec_frame,
        world.resource::<MaterialFrameInputs>(),
        Some(world.resource::<FrameAssemblyInputs>()),
    );
    state.swap(world);
    world.insert_resource(ExtraViewFrame {
        products: Some(products),
        exec_frame,
        frame_id,
        size,
    });
}

pub(crate) fn register(app: &mut App) {
    use crate::assemble::drawsurf::{
        execute_camera_products, ingest_drawsurf_list, open_frame_products, publish_frame_products,
        rebuild_static_draw_lane, update_command_context_code_sources,
    };
    app.init_resource::<ExtraViewFrame>()
        .add_systems(
            ExtraViewPass,
            (
                update_command_context_code_sources,
                crate::prepare::scene::cull::apply_dpvs_cull,
                ingest_drawsurf_list,
                crate::prepare::scene::smodel_lighting::update_extra_view_smodel_lighting,
                rebuild_static_draw_lane,
                open_frame_products,
                execute_camera_products,
                publish_frame_products,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            run_extra_view
                .in_set(frame::InMatch)
                .after(frame::RenderSet::FrontendAssemble),
        );
}
