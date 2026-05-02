use super::gpu::GridVertex;

/// Compute grid extent and step from the model bbox.
/// Use the default (extent=100, step=5) as the lower bound, and only enlarge for huge models.
pub fn compute_grid_params(bbox_min: glam::Vec3, bbox_max: glam::Vec3) -> (f32, f32) {
    const DEFAULT_EXTENT: f32 = 100.0;
    const DEFAULT_STEP: f32 = 5.0;

    let size = bbox_max - bbox_min;
    let max_dim = size.x.abs().max(size.y.abs()).max(size.z.abs());
    // Account for the model center offset as well.
    let center = (bbox_min + bbox_max) * 0.5;
    let max_offset = center.x.abs().max(center.z.abs());
    let needed = max_dim * 0.5 + max_offset;

    // Within the default range, keep the original grid.
    if needed <= DEFAULT_EXTENT {
        return (DEFAULT_EXTENT, DEFAULT_STEP);
    }

    // Huge model: round extent up to a clean value.
    let nice_values = [200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0];
    let extent = nice_values
        .iter()
        .find(|&&v| v >= needed)
        .copied()
        .unwrap_or(needed.ceil());

    // step: keep around 40 grid lines (20 per side).
    let raw_step = extent / 20.0;
    let nice_steps = [10.0, 20.0, 50.0, 100.0, 200.0, 500.0];
    let step = nice_steps
        .iter()
        .find(|&&v| v >= raw_step)
        .copied()
        .unwrap_or(raw_step.ceil());

    (extent, step)
}

/// Build vertex data for the grid floor.
/// Drawn on the Y=0 plane in PMX scale.
pub fn build_grid_vertices() -> (Vec<GridVertex>, u32) {
    build_grid_vertices_with_params(100.0, 5.0)
}

/// Build grid vertices with the given extent / step.
pub fn build_grid_vertices_with_params(extent: f32, step: f32) -> (Vec<GridVertex>, u32) {
    let lines_per_axis = (2.0 * extent / step) as usize + 1;
    let mut verts = Vec::with_capacity(lines_per_axis * 4);
    let color = [0.35, 0.35, 0.35, 1.0];
    let axis_color_x = [0.6, 0.3, 0.3, 1.0]; // X axis (reddish)
    let axis_color_z = [0.3, 0.3, 0.6, 1.0]; // Z axis (bluish)

    let line_count = (2.0 * extent / step).round() as i32;
    for i in 0..=line_count {
        let x = -extent + i as f32 * step;
        let c = if x.abs() < 0.01 { axis_color_z } else { color };
        verts.push(GridVertex {
            position: [x, 0.0, -extent],
            color: c,
        });
        verts.push(GridVertex {
            position: [x, 0.0, extent],
            color: c,
        });
    }

    for i in 0..=line_count {
        let z = -extent + i as f32 * step;
        let c = if z.abs() < 0.01 { axis_color_x } else { color };
        verts.push(GridVertex {
            position: [-extent, 0.0, z],
            color: c,
        });
        verts.push(GridVertex {
            position: [extent, 0.0, z],
            color: c,
        });
    }

    // Y axis (greenish)
    let axis_color_y = [0.3, 0.6, 0.3, 1.0];
    verts.push(GridVertex {
        position: [0.0, 0.0, 0.0],
        color: axis_color_y,
    });
    verts.push(GridVertex {
        position: [0.0, extent, 0.0],
        color: axis_color_y,
    });

    let count = verts.len() as u32;
    (verts, count)
}
