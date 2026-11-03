struct UiVertexOutput {
    @location(0) uv: vec2<f32>,
    @location(1) border_widths: vec4<f32>,
    @location(2) border_radius_x: vec4<f32>,
    @location(3) border_radius_y: vec4<f32>,
    @location(4) @interpolate(flat) size: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@group(1) @binding(0) var<uniform> color: vec4<f32>;
@group(1) @binding(1) var<uniform> rotation: vec4<f32>;
@group(1) @binding(2) var map: texture_2d<f32>;
@group(1) @binding(3) var map_sampler: sampler;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let point = in.uv * 2.0 - 1.0;
    let distance = dot(point, point);
    if distance >= 1.0 { discard; }
    let normal = vec3<f32>(point.x, -point.y, sqrt(1.0 - distance));
    let axial = vec3<f32>(normal.x * cos(rotation.z) - normal.y * sin(rotation.z), normal.x * sin(rotation.z) + normal.y * cos(rotation.z), normal.z);
    let tilt = rotation.x;
    let latitude = vec3<f32>(axial.x, axial.y * cos(tilt) + axial.z * sin(tilt), -axial.y * sin(tilt) + axial.z * cos(tilt));
    let longitude = atan2(latitude.x, latitude.z) + rotation.y;
    let uv = vec2<f32>(fract(longitude / 6.283185307 + 0.5), acos(clamp(latitude.y, -1.0, 1.0)) / 3.141592654);
    let surface = textureSampleLevel(map, map_sampler, uv, 0.0);
    let light = 0.3 + 0.7 * max(dot(normal, normalize(vec3<f32>(-0.4, 0.3, 1.0))), 0.0);
    let edge = 1.0 - smoothstep(1.0 - 4.0 / min(in.size.x, in.size.y), 1.0, distance);
    return vec4<f32>(surface.rgb * color.rgb * light, color.a * edge);
}
