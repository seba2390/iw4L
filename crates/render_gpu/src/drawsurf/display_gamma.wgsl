struct Ramp { exponent: vec4<f32> }
@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var<uniform> ramp: Ramp;
@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}
// Each of the frame's 256 levels to the level's fraction of 255 to the
// power of the ramp's exponent.
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let colour = textureLoad(frame, vec2<i32>(position.xy), 0);
    let level = round(colour.rgb * 255.0) / 255.0;
    return vec4<f32>(pow(level, vec3<f32>(ramp.exponent.x)), colour.a);
}
