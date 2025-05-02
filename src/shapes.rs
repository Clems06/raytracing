use derive_new::new;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, new)]
pub struct Sphere {
    pos: [f32; 3],
    radius_squared: f32,
    color: [f32; 4],
    mirror: f32,
    _pad_mirror: [u8; 12],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, new)]
pub struct Triangle {
    pos_a: [f32; 3],
    mirror: f32,
    pos_b: [f32; 3],
    pad: f32,
    pos_c: [f32; 3],
    pad2: f32,
    color: [f32; 4],
}