use alvr_common::{
    Fov,
    glam::{Quat, Vec3},
};
use alvr_session::EyeTrackedFoveationConfig;
use std::time::{Duration, Instant};

// Projects a head-local gaze rotation to a normalized [-1, 1] foveation center the encoder
// can consume directly as `center_shift_x/y`. The math mirrors the FOV→tangent conversion
// the OpenXR clients already do in alvr_graphics::stream so the resulting center lines up
// with the encoder's eye-frustum geometry. Runs a small prediction + smoothing filter so
// network/encode/decode latency doesn't drag the visible warp behind the user's actual gaze.
pub struct FoveationTracker {
    last_output: [f32; 2],
    last_input: [f32; 2],
    last_input_at: Option<Instant>,
}

impl FoveationTracker {
    pub fn new() -> Self {
        Self {
            last_output: [0.0, 0.0],
            last_input: [0.0, 0.0],
            last_input_at: None,
        }
    }

    // Returns the next foveation center to push to the encoder. `gaze` is the head-local gaze
    // rotation from FaceData.eyes_combined; `fov` is the encoder's view-frustum FOV (use one
    // eye — both are very close in practice, and the gaze sample doesn't disambiguate per-eye
    // anyway). Returns None if the result is essentially zero and not worth a wire round-trip
    // (e.g. extension reports gaze at lens axis).
    pub fn update(&mut self, gaze: Quat, fov: Fov, config: &EyeTrackedFoveationConfig) -> [f32; 2] {
        let now = Instant::now();

        let direction = gaze * Vec3::NEG_Z;
        let raw = if direction.z < -1e-3 {
            let half_x = (f32::tan(fov.left).abs() + f32::tan(fov.right)) * 0.5;
            let half_y = (f32::tan(fov.up) + f32::tan(fov.down).abs()) * 0.5;
            [
                (direction.x / -direction.z / half_x).clamp(-1.0, 1.0),
                (direction.y / -direction.z / half_y).clamp(-1.0, 1.0),
            ]
        } else {
            // Gaze pointing backwards or sideways — nothing sensible to encode; hold center.
            self.last_input
        };

        let predicted = if let Some(prev_at) = self.last_input_at {
            let dt = now
                .saturating_duration_since(prev_at)
                .as_secs_f32()
                .max(1e-4);
            let vx = (raw[0] - self.last_input[0]) / dt;
            let vy = (raw[1] - self.last_input[1]) / dt;
            let speed = (vx * vx + vy * vy).sqrt();

            if speed > config.saccade_velocity_threshold {
                // Eye is mid-saccade. Snap toward (0, 0) — foveation during a saccade is not
                // perceptible, and overshooting it is the worst case (high-detail region
                // briefly lands somewhere the user *isn't* looking).
                [0.0, 0.0]
            } else {
                let lead = (config.prediction_ms as f32) * 1e-3;
                [
                    (raw[0] + vx * lead).clamp(-1.0, 1.0),
                    (raw[1] + vy * lead).clamp(-1.0, 1.0),
                ]
            }
        } else {
            raw
        };

        let alpha = config.smoothing_alpha.clamp(0.0, 0.999);
        let smoothed = [
            alpha * self.last_output[0] + (1.0 - alpha) * predicted[0],
            alpha * self.last_output[1] + (1.0 - alpha) * predicted[1],
        ];

        self.last_input = raw;
        self.last_input_at = Some(now);
        self.last_output = smoothed;

        smoothed
    }
}

impl Default for FoveationTracker {
    fn default() -> Self {
        Self::new()
    }
}
