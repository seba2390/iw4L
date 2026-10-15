# GSC post effects

GSC owns the presented post effects. Script presets and dvars pass through
snapshot metadata to the GPU; user brightness, DoF, bloom and tweak settings do
not suppress explicit script effects. State resets with the match. Grading keeps
disabled presets and parse refusals distinct; enable/disable transitions blend
with ungraded color. The natives below are engine services
(`NativeRegistry::engine_services`); Black Ops adds `setBlur`/`startFadingBlur`.
BO2's own scripts and BO3 bind their own registries: BO2 sets the naked vision
and checks, but does not draw, `setExpFog`/`setVolFog`.

* `VisionSetNaked/Night/Pain/Thermal/MissileCam(name, seconds)` and the
  `self ...ForPlayer(name, seconds)` methods select `vision/<name>.vision`.
  An empty name restores the map preset; global changes clear player overrides;
  Night follows the night vision flag; Pain blends in below half health.
* `self SetBlurForPlayer(radius, seconds)` eases Gaussian blur from its current
  radius; 0 clears it.
* `self SetDepthOfField(nearStart, nearEnd, farStart, farEnd, nearBlur, farBlur)`
  controls focus and blur; zero distance ranges return to normal autofocus.
* `SetDvar(name, value)` applies globally; `self SetClientDvar(s)` overrides
  that client's values. Film supports enable, brightness, contrast, (dark)
  desaturation, invert and light/medium/dark RGB tints, as `r_film...` or
  `r_filmTweak...`.
* Bloom supports `r_glow`, radius0, bloom intensity0, cutoff and desaturation,
  including the `r_glowTweak...` spellings. DoF supports `r_dof_enable`,
  viewmodel/near/far start/end, near/far blur, and bias.
* IW4L also accepts the dvars (not natives) `r_hue`/`r_filmHue` (degrees),
  `r_gamma` (>0), `r_exposure` (stops), `r_saturation` (1 is neutral), `r_blur`,
  `r_brightness`, `r_contrast`.
* `SetExpFog(start, halfway, r, g, b, [opacity,] seconds)` changes map fog.
  The 14-argument sun-fog form is supported. Transitions blend density and
  packed color from the current state; the first call applies immediately.
* `Earthquake(scale, seconds, origin, radius)` adds a timed, presentation-only
  camera shake. Calls overlap, attenuate linearly with distance and fade; zero
  radius is global, and scale clamps to 1 for camera angles.
```c
self SetClientDvars("r_filmBrightness", 0.1, "r_hue", 90, "r_gamma", 1.2);
self SetClientDvar("r_filmLightTint", (1, 0.5, 0.25)); self SetBlurForPlayer(6, 0.5);
```
The render chain grades color, blurs the scene, then applies film, DoF and bloom;
HUD stays readable and bloom keeps authored sRGB writes. Blurred shellshock
accumulates frames (`bg_shock_screenBlurBlendTime`, `...FadeTime`), desaturates
the saved image, and resets on stop, respawn and map change.
Scoped thermal vision switches instantly with the scope overlay for every
weapon family. `SetThermalBodyMaterial(name)` selects the global body camera
material (empty: `thermalbody_default`; Cold-Blooded bodies keep theirs).
`thermalBlurFactorNoScope` sets unscoped thermal accumulation in ms (default
250, range 0–10000); scopes bypass `thermalBlurFactorScope`. `cg_drawShellshock`
controls profile effects; thermal accumulation works while it is off. Host and
client must share `net::PROTOCOL_VERSION`.
