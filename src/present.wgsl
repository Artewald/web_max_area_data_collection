struct VertOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vert_idx: u32) -> VertOut {
    var out: VertOut;

    let x = f32(i32(vert_idx) % 2) * 2.0 - 1.0;
    let y = f32(i32(vert_idx) / 2) * 2.0 - 1.0;

    out.pos = vec4(x, y, 0.0, 1.0);
    out.tex_coords = vec2((x + 1.0) / 2.0, (1.0 - y) / 2);

    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var sam: sampler;

@fragment
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    return textureSample(tex, sam, in.tex_coords);
}
