use std::error::Error;
use crate::shapes::Triangle;


pub fn load_model(path: &str) -> Result<Vec<Triangle>, Box<dyn Error>> {
    if path.ends_with(".obj") {
        load_obj(path)
    } else if path.ends_with(".glb") {
        load_glb(path)
    } else {
        Err("Unsupported file format".into())
    }
}

fn load_obj(path: &str) -> Result<Vec<Triangle>, Box<dyn Error>> {
    let (models, materials) = tobj::load_obj(
        path,
        &tobj::LoadOptions { triangulate: true, ..Default::default() },
    )?;

    let mat_list = materials?;

    let mut triangles = Vec::new();
    for model in &models {
        let mesh = &model.mesh;
        let positions = &mesh.positions;

            // Use provided indices to build triangles
            for chunk in mesh.indices.chunks(3) {
                if let &[i, j, k] = chunk {
                    let pos_a = extract_position(positions, i as usize);
                    let pos_b = extract_position(positions, j as usize);
                    let pos_c = extract_position(positions, k as usize);
                    if let Some(mat_id) = mesh.material_id {
                        if let Some(ambient) = mat_list[mat_id].ambient {
                            triangles.push(create_triangle(pos_a, pos_b, pos_c, [ambient[0], ambient[1], ambient[2], 1.0]));
                        }
                    } else {
                        triangles.push(create_triangle(pos_a, pos_b, pos_c, [1.0; 4]));
                    }

                }
            }

    }
    Ok(triangles)
}

fn load_glb(path: &str) -> Result<Vec<Triangle>, Box<dyn Error>> {
    let data = std::fs::read(path)?;
    let (document, buffers, _) = gltf::import_slice(&data)?;

    let mut triangles = Vec::new();
    for node in document.nodes() {
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }

                let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|b| &b.0[..]));
                let positions: Vec<[f32; 3]> = reader.read_positions()
                    .map(|p| p.collect())
                    .unwrap_or_default();

                // Try to get vertex colors first
                let colors: Vec<[f32; 4]> = reader.read_colors(0)
                    .map(|colors| colors.into_rgba_f32().map(|c| [c[0], c[1], c[2], c[3]]).collect())
                    .unwrap_or_default();

                // Get material color
                let material_color = primitive.material()
                    .pbr_metallic_roughness()
                    .base_color_factor();

                if let Some(indices) = reader.read_indices() {
                    let indices: Vec<u32> = indices.into_u32().collect();
                    for triangle in indices.chunks(3) {
                        if let &[i, j, k] = triangle {
                            // Use vertex colors if available, otherwise material color
                            let color = if !colors.is_empty() {
                                let c1 = colors[i as usize];
                                let c2 = colors[j as usize];
                                let c3 = colors[k as usize];
                                [
                                    (c1[0] + c2[0] + c3[0]) / 3.0,
                                    (c1[1] + c2[1] + c3[1]) / 3.0,
                                    (c1[2] + c2[2] + c3[2]) / 3.0,
                                    1.0
                                ]
                            } else {
                                material_color
                            };

                            triangles.push(create_triangle(
                                positions[i as usize],
                                positions[j as usize],
                                positions[k as usize],
                                color,
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(triangles)
}

fn extract_position(positions: &[f32], index: usize) -> [f32; 3] {
    [
        positions[3 * index],
        positions[3 * index + 1],
        positions[3 * index + 2],
    ]
}

fn create_triangle(a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 4]) -> Triangle {
    Triangle::new(a,0.0, b, 0.0, c, 0.0, color)
}