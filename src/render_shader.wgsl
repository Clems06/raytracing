
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // Manually handle each vertex index
    var position: vec2<f32>;
    switch vertex_index {
        case 0u: { position = vec2<f32>(-1.0, -1.0); } // Bottom-left
        case 1u: { position = vec2<f32>(1.0, -1.0); }  // Bottom-right
        case 2u: { position = vec2<f32>(-1.0, 1.0); }  // Top-left
        case 3u: { position = vec2<f32>(-1.0, 1.0); }  // Top-left
        case 4u: { position = vec2<f32>(1.0, -1.0); }  // Bottom-right
        case 5u: { position = vec2<f32>(1.0, 1.0); }   // Top-right
        default: { position = vec2<f32>(0.0, 0.0); }  // Fallback (should not happen)
    }

    // Return the position for the current vertex
    return vec4<f32>(position, 0.0, 1.0);
}

// Fragment shader
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let tex_size = textureDimensions(tex);
    let uv = pos.xy / vec2<f32>(tex_size);
    return textureSample(tex, samp, uv);

}