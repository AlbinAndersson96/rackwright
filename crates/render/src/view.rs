//! Per-frame view state: camera, camera tweens, port-placement mode and object motion.

use rackwright_core::limits::Limits;
use rackwright_core::model::Document;

use crate::camera::{OrbitCamera, Tween};
use crate::layout::FaceRect;
use crate::scene::{Motion, animation_targets};

#[derive(Debug, Clone)]
pub struct ViewState {
    camera: OrbitCamera,
    tween: Option<Tween>,
    saved: Option<OrbitCamera>,
    pub motion: Motion,
}

impl ViewState {
    pub fn new(camera: OrbitCamera) -> ViewState {
        ViewState {
            camera,
            tween: None,
            saved: None,
            motion: Motion::default(),
        }
    }

    pub fn camera(&self) -> &OrbitCamera {
        &self.camera
    }

    /// The camera for direct user control (orbit, pan, zoom); cancels any running tween.
    pub fn camera_mut(&mut self) -> &mut OrbitCamera {
        self.tween = None;
        &mut self.camera
    }

    /// Starts an eased move from the current camera to `to`.
    pub fn animate_to(&mut self, to: OrbitCamera, now_s: f64) {
        self.tween = Some(Tween {
            from: self.camera,
            to,
            start_s: now_s,
        });
    }

    pub fn is_tweening(&self) -> bool {
        self.tween.is_some()
    }

    /// Saves the current camera (or the current tween's destination) and moves to `face`.
    pub fn enter_port_mode(&mut self, face: &FaceRect, aspect: f32, now_s: f64) {
        if self.saved.is_none() {
            self.saved = Some(self.tween.map_or(self.camera, |t| t.to));
        }
        self.animate_to(OrbitCamera::face_view(face, aspect), now_s);
    }

    /// Moves back to the camera saved by `enter_port_mode`. Does nothing outside port mode.
    pub fn exit_port_mode(&mut self, now_s: f64) {
        if let Some(saved) = self.saved.take() {
            self.animate_to(saved, now_s);
        }
    }

    pub fn in_port_mode(&self) -> bool {
        self.saved.is_some()
    }

    /// Advances the camera tween to `now_s` and object motion by `dt_s`.
    /// Returns `true` while anything is still animating (keep requesting frames).
    pub fn advance(&mut self, doc: &Document, limits: &Limits, now_s: f64, dt_s: f32) -> bool {
        if let Some(tween) = self.tween {
            let (camera, done) = tween.sample(now_s);
            self.camera = camera;
            if done {
                self.tween = None;
            }
        }
        let moving = self.motion.update(&animation_targets(doc, limits), dt_s);
        self.tween.is_some() || moving
    }
}
