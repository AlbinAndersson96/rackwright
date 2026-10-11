mod common;

use common::assert_close;
use glam::{Vec2, Vec3};
use rackwright_render::camera::*;
use rackwright_render::layout::{Aabb, FaceRect};

fn front(distance: f32) -> OrbitCamera {
    OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        distance,
    }
}

#[test]
fn zero_yaw_and_pitch_looks_at_the_rack_fronts() {
    let eye = front(1000.0).eye();
    assert_close(eye.x, 0.0);
    assert_close(eye.y, 0.0);
    assert_close(eye.z, 1000.0);
}

#[test]
fn orbit_clamps_pitch() {
    let mut c = front(1000.0);
    c.orbit(0.0, 1.0e6);
    assert_close(c.pitch, MAX_PITCH);
    c.orbit(0.0, -1.0e6);
    assert_close(c.pitch, -MAX_PITCH);
}

#[test]
fn zoom_is_clamped() {
    let mut c = front(1000.0);
    c.zoom(-1.0e6);
    assert_close(c.distance, MIN_DISTANCE_MM);
    c.zoom(1.0e6);
    assert_close(c.distance, MAX_DISTANCE_MM);
}

#[test]
fn pan_moves_the_target_against_the_pointer() {
    let mut c = front(1000.0);
    c.pan(10.0, 0.0, 500.0);
    assert!(c.target.x < 0.0, "dragging right moves the target left");
    assert_close(c.target.y, 0.0);
}

#[test]
fn centre_ray_points_at_the_target() {
    let c = front(1000.0);
    let ray = c.ray(Vec2::ZERO, 1.5);
    assert_close(ray.dir.z, -1.0);
    let p = ray.hit_plane_z(0.0).unwrap();
    assert_close(p.x, 0.0);
    assert_close(p.y, 0.0);
}

#[test]
fn pixel_to_ndc_maps_corners() {
    let v = Vec2::new(200.0, 100.0);
    assert_eq!(pixel_to_ndc(Vec2::ZERO, v), Vec2::new(-1.0, 1.0));
    assert_eq!(pixel_to_ndc(v, v), Vec2::new(1.0, -1.0));
}

#[test]
fn framing_keeps_the_whole_box_in_view() {
    let b = Aabb::new(Vec3::new(0.0, 0.0, -800.0), Vec3::new(3000.0, 2000.0, 0.0));
    let c = OrbitCamera::front_view(&b, 16.0 / 9.0);
    assert_eq!(c.target, b.center());
    assert_close(c.yaw, 0.0);
    assert_close(c.pitch, FRONT_VIEW_PITCH);
    let vp = c.view_proj(16.0 / 9.0);
    for x in [b.min.x, b.max.x] {
        for y in [b.min.y, b.max.y] {
            for z in [b.min.z, b.max.z] {
                let ndc = vp.project_point3(Vec3::new(x, y, z));
                assert!(
                    ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
                    "{x},{y},{z} → {ndc}"
                );
            }
        }
    }
}

#[test]
fn face_view_looks_straight_at_the_face() {
    let face = FaceRect {
        min: Vec2::new(100.0, 200.0),
        max: Vec2::new(582.6, 288.9),
        z: 0.0,
    };
    let c = OrbitCamera::face_view(&face, 1.0);
    assert_eq!(c.target, face.center());
    assert_close(c.yaw, 0.0);
    assert_close(c.pitch, 0.0);
    let ndc = c
        .view_proj(1.0)
        .project_point3(Vec3::new(582.6, 288.9, 0.0));
    assert!(
        ndc.x <= 1.0 && ndc.x > 0.7,
        "face fills most of the width: {ndc}"
    );
}

#[test]
fn tween_eases_from_start_to_end() {
    let from = front(1000.0);
    let to = OrbitCamera {
        target: Vec3::new(100.0, 0.0, 0.0),
        yaw: 1.0,
        pitch: 0.5,
        distance: 2000.0,
    };
    let tween = Tween {
        from,
        to,
        start_s: 10.0,
    };
    assert_eq!(tween.sample(10.0), (from, false));
    let (mid, done) = tween.sample(10.0 + TWEEN_SECONDS / 2.0);
    assert!(!done);
    assert_close(mid.distance, 1500.0);
    assert_eq!(tween.sample(10.0 + TWEEN_SECONDS), (to, true));
    assert!(tween.sample(99.0).1);
}

#[test]
fn tween_turns_the_short_way_round() {
    let from = OrbitCamera {
        yaw: 3.0,
        ..front(1000.0)
    };
    let to = OrbitCamera {
        yaw: -3.0,
        ..front(1000.0)
    };
    let (mid, _) = Tween {
        from,
        to,
        start_s: 0.0,
    }
    .sample(TWEEN_SECONDS / 2.0);
    assert!(mid.yaw > 3.0, "passes through π, not 0: {}", mid.yaw);
}

#[test]
fn largest_document_fits_the_front_view() {
    use rackwright_core::ids::RackId;
    use rackwright_core::model::{Document, Rack};
    use rackwright_core::name::{DocumentName, Name};
    use rackwright_render::layout::scene_bounds;
    let limits = rackwright_core::limits::Limits::default();
    let racks = (1..=5)
        .map(|i| Rack {
            id: RackId::new(),
            name: Name::parse(&format!("R{i}"), &limits).unwrap(),
            height_u: limits.max_rack_height_u,
            devices: vec![],
        })
        .collect();
    let doc = Document {
        name: DocumentName::parse("Big").unwrap(),
        racks,
        cables: vec![],
        catalog: vec![],
    };
    let b = scene_bounds(&doc);
    let c = OrbitCamera::front_view(&b, 16.0 / 9.0);
    assert!(c.distance < MAX_DISTANCE_MM, "not clamped: {}", c.distance);
    let ndc = c.view_proj(16.0 / 9.0).project_point3(b.max);
    assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0);
}

#[test]
fn depth_view_proj_reverses_depth_only() {
    let cam = front(1000.0);
    let normal = cam.view_proj(1.5);
    let reversed = cam.depth_view_proj(1.5);
    for z in [-500.0, 0.0, 400.0] {
        let p = Vec3::new(120.0, -80.0, z);
        let (a, b) = (normal.project_point3(p), reversed.project_point3(p));
        assert_close(a.x, b.x);
        assert_close(a.y, b.y);
    }
    let near = cam.eye() - Vec3::Z * NEAR_MM;
    let far = cam.eye() - Vec3::Z * FAR_MM;
    assert_close(reversed.project_point3(near).z, 1.0);
    assert_close(reversed.project_point3(far).z, 0.0);
}
