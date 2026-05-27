@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {

    var x: f32 = frag_coord.x;
    var y: f32 = frag_coord.y;
    var z: f32 = frag_coord.z;

    for (var i = 0u; i < 100; i++) {
        x = abs(tanh(sin((f32(i) + x) * cos(f32(i) - x))));
        y = abs(tanh(sin((f32(i) + y) * cos(f32(i) - y))));
        z = abs(tanh(sin((f32(i) + z) * cos(f32(i) - z))));
    }

    x = fract(x);
    y = fract(y);
    z = fract(z);

    return vec4<f32>(x, y, z, 1.0);
}
