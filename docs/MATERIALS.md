# Material preparation contracts

`asset_material::compile_material_catalog` builds the runtime catalog through private
family compilers (`MaterialCompiler`: IW4, IW5, T5, T6). They own argument translation,
shader/declaration references, colour and shadow policy, state rows, sort order and
source provenance. A missing shader or declaration stays explicit metadata that
admission refuses. A prepared table holds local banks, sampler bindings, admitted ports
and compiled state; a draw still needs an admitted port for its technique and vertex
layout, and every dynamic code source must bind.

`PreparedMaterialTable::from_catalog` binds products to the catalog's
`MaterialGenerationId`; execution, technique queries and shell capture reject another
generation, so a new catalog needs a rebuilt table. `RuntimeMaterialBuild::publish` mints
the generation inside `render_material`; exact clones keep it, edited parts get a new one.
`TessMaterials::new` checks the catalog/table pair before installation.

`compile_material_state(family, words)` returns `Option<CompiledPassState>`: `None` for a
family with no compiler (T7). Catalog compilation and HUD installation use it.
`CompiledPassState` holds decoded blend, alpha-test, cull, colour/sRGB write, fill, depth,
stencil and offset rules plus unsupported-field diagnostics; its fields are private and
the authored words remain for provenance. GPU adapters read its getters and never decode
source state. IW4 and IW5 share one encoding, with `asset_iw4::alpha_test_from_state_bits`
as the alpha-test authority; T5 has its own alpha-test decoder and T6 has none. Normal
and multiply-pass blends compile together. Unknown factors or operations are
`UnsupportedState` refusals; authored words are never repaired.

Technique selection keeps `SourceTechniqueSelection` (namespace, slot and policy: `Exact`,
`DfogCompatibility`, `UnshadowedCompatibility`, `LitFallbackCompatibility`,
`EmissiveCompatibility`, `DepthToColourCompatibility`).
`render_material::prepare_program_abi` owns stage lookup, SM3/DXBC decoding, declaration
routing and ABI bindings; DXBC stage mismatches refuse before lowering. The frontend
lowers prepared programs, validates WGSL and derives the GPU layout. ABI success does not
imply texture residency or WGSL support.

Stable material shells keep generation, material/technique/vertex identity, provenance,
state and local bindings. Rebinding takes the current catalog, refuses a stale shell and
resolves dynamic constants and textures again. HUD chrome compiles state when its menu
catalog installs and refuses unsupported state there. Family compilation also publishes
camera routing, smodel colour admission, unlit sky routing and IW4 host-postfx admission.

`compile_material_bindings` prepares a generation-bound plan from the snapshot's technique
arguments: demanded frame and local-light producers plus fixed saturation, SH, water, wind
and host-default rows. Unmapped required sources report `MissingCodeConstant`. World
preparation supplies lighting, exposure, sky, fog and sun inputs; material adapters own
shader packing. Stale plans refuse before writes; catalog replacement clears caches.

T6 materials use captured headers, technique sets, state, constants and textures; missing
captures refuse, and common weapons do not inherit IW4 surfaces. T6 static models use
their light-grid palette with full SH. Corrupt mip cache records are refused.
