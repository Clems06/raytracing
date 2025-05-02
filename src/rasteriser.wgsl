struct InstanceData { position: vec3f, radius: f32 };
@group(0) @binding(0) var<storage> instances: array<InstanceData>;

@vertex
fn vs_sphere(
  @location(0) pos: vec3f,
  @location(1) normal: vec3f,
  @builtin(instance_index) iid: u32
) -> VSOutput {
  let instance = instances[iid];
  let world_pos = instance.position + pos * instance.radius;
  return VSOutput(
    uniforms.proj_view * vec4f(world_pos, 1.0),
    normal
  );
}

