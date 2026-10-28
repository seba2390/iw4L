# Render: what lives where

`render_frontend` reads the presented snapshot; `render_gpu` writes pixels and
owns `wgpu`. Rendering decides no gameplay and never writes `AuthorityWorld`.
Every game uses these crates; the `iw4` tags below are
[shared debt](PROGRESS.md#shared-technical-debt) (see [`ARCHITECTURE.md`](ARCHITECTURE.md)).

| crate | role |
|---|---|
| `render_frontend` | scene preparation, culling, occupancy adapters, frame products and tessellation |
| `render_scene` | lighting, frustum, camera and occupancy storage; static light/BSP data publish on installation and clear on teardown |
| `render_anim`, `render_fx` | animation poses and effects (`render_fx` is declared `game = "iw4"`) |
| `render_frame` | frame products, revisions, packed segments; declared `game = "iw4"` like `fx` |
| `render_material`, `render_backend` | material tables and draw-list views |
| `render_gpu` | GPU preparation and recording |
| `render` | composition, extraction and diagnostics |

```text
zone → asset IR → prepare/scene/ → assemble/drawsurf/
     → sorted draw list → tessellation → material execution
     → SM3 or DXBC (SM5) → WGSL → GPU contract → wgpu
```

One drawsurf machine accepts world, static-model and viewmodel emitters.
Lighting uses `lighting_iw4`, `smodel_lighting.rs` and `model_lighting_atlas.rs`;
shadows use `sun_shadow.rs`. `StaticSunAndFx` overlaps static sun preparation
with FX, joins before frame products consume results, then merges dynamic
casters. Sun model draws reuse colour geometry's index buffers. Retained lanes
in `retained_list/` key static reuse on world, material, visibility and eye/LOD.
FX also keys on distortion; glass re-sorts for each eye. GPU skinning computes
FPV and remote body packed vertices before native colour/shadows. Shapes use
stable IDs and retire with live jobs; invalid jobs refuse geometry.

GPU modules live in `render_gpu/src/drawsurf/colour_submit/`:

* `publication.rs` seals a frame against its installed world; both generations must match.
* `geometry.rs` and `residency.rs` track capacity, live length, allocation generation and uploaded revision. Shrinking retains buffers; changed packed segments upload alone.
* `binding.rs` and `constants.rs` cache textures and constants. Recording checks prepared work and table epochs against the extracted frame.
* `prepare_camera.rs` and `shadow_prepare.rs` prepare concurrently after shared shadow-view installation. Shadow frames remove dynamic tails and retain static patch targets. Multiple camera views refuse with `MultipleCameraViews`.
* `extra_cam.rs` is a game's extra camera (`ModeRules::extra_camera`): the frontend's `prepare/scene/extra_view.rs` swaps in the second view's resources and reruns cull, lists and camera products for it; the seal adds its products to the main frame's geometry, and its own lane draws it into a target the main view samples as a code texture.
* `record.rs`, `encode.rs`, `shadow_encode.rs` and `indirect.rs` record draws. With device support, adjacent compatible commands merge into indirect draws while preserving order (`IW4L_MULTI_DRAW=0` turns it off).

`render/src/extract.rs` replaces `InstalledRenderWorld` when geometry, ports or
SMC maps change and publishes `PublishedRenderFrame` each frame. `d3d9_decl`,
`d3d9_sm3` and `d3d9_state` describe D3D9 semantics; `dxbc_sm5` decodes BO2's
SM5 shaders. GPU adapters convert them to wgpu. `session` installs matches;
`assets` supplies bytes. Runtime switches are env variables (`IW4L_SINGLE_CELL`,
`IW4L_MULTI_DRAW`, `IW4L_SMODEL_LIGHTING`, `IW4L_NO_FRUSTUM`, …); the sun
shadow uses console `sm_enable`/`sm_sunEnable`. Observation uses [`PERF.md`](PERF.md).
Film vision presets come from map/common fastfiles; console `visionSetNaked` and
`visionReset` transition `FilmVisionView` between presets ([`POSTFX.md`](POSTFX.md)).
