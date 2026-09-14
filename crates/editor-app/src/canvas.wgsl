struct Uniforms {
    canvas_px: vec2<f32>,
    viewport_min_px: vec2<f32>,
    pixels_per_point: f32,
    zoom: f32,
    pan: vec2<f32>,
    object_count: u32,
    _padding: u32,
    _padding2: vec2<u32>,
    objects: array<vec4<f32>, 48>,
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn object_coverage(index: u32, point: vec2<f32>) -> bool {
    let a = uniforms.objects[index * 3u];
    let b = uniforms.objects[index * 3u + 1u];
    if (a.z < 0.5) {
        let radius = distance(point, a.xy);
        return radius <= b.x + 0.000001 && (b.y == 0.0 || radius >= b.y);
    }
    let direction = b.xy - a.xy;
    let length_squared = dot(direction, direction);
    // Same <= EPSILON_MM squared degeneracy rule as editor-core.
    var amount = 0.0;
    if (length_squared > 0.000000000001) {
        amount = clamp(dot(point - a.xy, direction) / length_squared, 0.0, 1.0);
    }
    let nearest = a.xy + amount * direction;
    return distance(point, nearest) <= b.z + 0.000001;
}

fn scene_coverage(point: vec2<f32>) -> bool {
    var layers = array<bool, 4>(false, false, false, false);
    var index = 0u;
    loop {
        if (index >= uniforms.object_count) { break; }
        let base = index * 3u;
        let header = uniforms.objects[base];
        let exposure = uniforms.objects[base + 2u].x > 0.5;
        let covered = object_coverage(index, point);
        let layer = u32(header.w);
        if (layer < 4u && covered) {
            layers[layer] = exposure;
        }
        index = index + 1u;
    }
    // Each layer was composed independently above. Combining their results
    // cannot let a Clear operation erase a different layer.
    return layers[0] || layers[1] || layers[2] || layers[3];
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let local_px = position.xy - uniforms.viewport_min_px;
    let point = vec2<f32>(
        (local_px.x / uniforms.pixels_per_point - uniforms.canvas_px.x / uniforms.pixels_per_point / 2.0 - uniforms.pan.x) / uniforms.zoom,
        -(local_px.y / uniforms.pixels_per_point - uniforms.canvas_px.y / uniforms.pixels_per_point / 2.0 - uniforms.pan.y) / uniforms.zoom,
    );
    let covered = scene_coverage(point);
    if (covered) { return vec4<f32>(0.18, 0.72, 1.0, 1.0); }
    return vec4<f32>(0.10, 0.12, 0.16, 1.0);
}
