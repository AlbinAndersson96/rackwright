mod common;

use common::*;
use glam::{Vec2, Vec3};
use rackwright_render::camera::{OrbitCamera, TWEEN_SECONDS};
use rackwright_render::layout::FaceRect;
use rackwright_render::view::ViewState;

fn start() -> OrbitCamera {
    OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.5,
        pitch: 0.2,
        distance: 3000.0,
    }
}

fn face() -> FaceRect {
    FaceRect {
        min: Vec2::new(0.0, 0.0),
        max: Vec2::new(482.6, 44.45),
        z: 0.0,
    }
}

#[test]
fn camera_tween_runs_to_completion() {
    let d = doc(vec![]);
    let mut v = ViewState::new(start());
    let to = OrbitCamera {
        distance: 1000.0,
        ..start()
    };
    v.animate_to(to, 1.0);
    assert!(v.advance(&d, &limits(), 1.0 + TWEEN_SECONDS / 2.0, 0.016));
    assert!(!v.advance(&d, &limits(), 1.0 + TWEEN_SECONDS, 0.016));
    assert_eq!(*v.camera(), to);
}

#[test]
fn port_mode_returns_to_the_saved_camera() {
    let d = doc(vec![]);
    let mut v = ViewState::new(start());
    v.enter_port_mode(&face(), 1.5, 0.0);
    assert!(v.in_port_mode());
    v.advance(&d, &limits(), 1.0, 0.016);
    assert_eq!(*v.camera(), OrbitCamera::face_view(&face(), 1.5));
    v.exit_port_mode(2.0);
    assert!(!v.in_port_mode());
    v.advance(&d, &limits(), 3.0, 0.016);
    assert_eq!(*v.camera(), start());
}

#[test]
fn entering_port_mode_mid_tween_saves_the_tween_destination() {
    let d = doc(vec![]);
    let mut v = ViewState::new(start());
    let destination = OrbitCamera {
        distance: 1000.0,
        ..start()
    };
    v.animate_to(destination, 0.0);
    v.enter_port_mode(&face(), 1.5, 0.1);
    v.exit_port_mode(0.2);
    v.advance(&d, &limits(), 5.0, 0.016);
    assert_eq!(*v.camera(), destination);
}

#[test]
fn user_camera_control_cancels_a_tween() {
    let mut v = ViewState::new(start());
    v.animate_to(
        OrbitCamera {
            distance: 1000.0,
            ..start()
        },
        0.0,
    );
    v.camera_mut().zoom(0.0);
    assert!(!v.is_tweening());
}
