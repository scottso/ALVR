use alvr_common::{
    Fov,
    glam::{Quat, Vec3},
};
use alvr_session::EyeTrackedFoveationConfig;
use std::time::Instant;

// Projects a head-local gaze rotation to a normalized [-1, 1] foveation center the encoder
// can consume directly as `center_shift_x/y`. The math mirrors the FOV→tangent conversion
// the OpenXR clients already do in alvr_graphics::stream so the resulting center lines up
// with the encoder's eye-frustum geometry. Runs a small prediction + smoothing filter so
// network/encode/decode latency doesn't drag the visible warp behind the user's actual gaze.
pub struct FoveationTracker {
    last_output: [f32; 2],
    last_input: [f32; 2],
    last_gaze: Option<Quat>,
    last_input_at: Option<Instant>,
}

impl FoveationTracker {
    pub fn new() -> Self {
        Self {
            last_output: [0.0, 0.0],
            last_input: [0.0, 0.0],
            last_gaze: None,
            last_input_at: None,
        }
    }

    // Returns the next foveation center to push to the encoder. `gaze` is the head-local gaze
    // rotation from FaceData.eyes_combined; `fov` is the encoder's view-frustum FOV (use one
    // eye — both are very close in practice, and the gaze sample doesn't disambiguate per-eye
    // anyway).
    pub fn update(&mut self, gaze: Quat, fov: Fov, config: &EyeTrackedFoveationConfig) -> [f32; 2] {
        let now = Instant::now();

        // Project the gaze direction into screen-tangent space, then normalize through the
        // FOV's half-extent. The key wrinkle: for asymmetric FOVs (typical on the vertical
        // axis — e.g. fov.up=0.9, fov.down=-0.6), the lens optical axis is not centered in
        // tangent space, so we subtract the center tangent before normalizing.
        let direction = gaze * Vec3::NEG_Z;
        let raw = if direction.z < -1e-3 {
            let tan_left = f32::tan(fov.left);
            let tan_right = f32::tan(fov.right);
            let tan_down = f32::tan(fov.down);
            let tan_up = f32::tan(fov.up);
            let center_tan_x = (tan_left + tan_right) * 0.5;
            let center_tan_y = (tan_down + tan_up) * 0.5;
            let half_x = (tan_right - tan_left) * 0.5;
            let half_y = (tan_up - tan_down) * 0.5;
            let gaze_tan_x = direction.x / -direction.z;
            let gaze_tan_y = direction.y / -direction.z;
            [
                ((gaze_tan_x - center_tan_x) / half_x).clamp(-1.0, 1.0),
                ((gaze_tan_y - center_tan_y) / half_y).clamp(-1.0, 1.0),
            ]
        } else {
            // Gaze pointing backwards or sideways — nothing sensible to encode; hold center.
            self.last_input
        };

        // Detect saccades from the rotation delta between gaze samples. Angular velocity in
        // rad/s is the unit the settings UI advertises, and it's the standard biometric
        // measure of saccades (~4–12 rad/s = 200°/s–700°/s).
        let saccade = self
            .last_gaze
            .zip(self.last_input_at)
            .map(|(prev_gaze, prev_at)| {
                let dt = now
                    .saturating_duration_since(prev_at)
                    .as_secs_f32()
                    .max(1e-4);
                gaze.angle_between(prev_gaze) / dt
            })
            .map(|angular_velocity| angular_velocity > config.saccade_velocity_threshold)
            .unwrap_or(false);

        let predicted = if saccade {
            // Mid-saccade: foveation is invisible to the viewer during the saccade itself,
            // and overshooting after the saccade is the worst case (high-detail region
            // briefly lands somewhere the user *isn't* looking). Snap to center.
            [0.0, 0.0]
        } else if let Some(prev_at) = self.last_input_at {
            let dt = now
                .saturating_duration_since(prev_at)
                .as_secs_f32()
                .max(1e-4);
            let vx = (raw[0] - self.last_input[0]) / dt;
            let vy = (raw[1] - self.last_input[1]) / dt;
            let lead = (config.prediction_ms as f32) * 1e-3;
            [
                (raw[0] + vx * lead).clamp(-1.0, 1.0),
                (raw[1] + vy * lead).clamp(-1.0, 1.0),
            ]
        } else {
            raw
        };

        let alpha = config.smoothing_alpha.clamp(0.0, 0.999);
        let smoothed = [
            alpha * self.last_output[0] + (1.0 - alpha) * predicted[0],
            alpha * self.last_output[1] + (1.0 - alpha) * predicted[1],
        ];

        self.last_input = raw;
        self.last_gaze = Some(gaze);
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

#[cfg(test)]
mod tests {
    use super::*;
    use alvr_common::glam::EulerRot;

    fn cfg(saccade: f32) -> EyeTrackedFoveationConfig {
        EyeTrackedFoveationConfig {
            // Disable smoothing and prediction so the tests assert on the raw projection.
            prediction_ms: 0,
            smoothing_alpha: 0.0,
            saccade_velocity_threshold: saccade,
        }
    }

    fn symmetric_fov(half_angle: f32) -> Fov {
        Fov {
            left: -half_angle,
            right: half_angle,
            up: half_angle,
            down: -half_angle,
        }
    }

    // With a symmetric FOV and gaze straight-forward (identity quaternion), the projected
    // raw center must be the lens axis (0, 0). Sanity check that didn't regress when we
    // rewrote the projection to subtract the FOV center tangent.
    #[test]
    fn straight_forward_gaze_maps_to_zero() {
        let mut tracker = FoveationTracker::new();
        let center = tracker.update(Quat::IDENTITY, symmetric_fov(0.8), &cfg(100.0));
        assert!(center[0].abs() < 1e-5, "expected x≈0, got {}", center[0]);
        assert!(center[1].abs() < 1e-5, "expected y≈0, got {}", center[1]);
    }

    // Asymmetric vertical FOV (up=0.9, down=-0.6 — typical of Quest-style frustums). The
    // pre-fix projection assumed a symmetric frustum and just normalized `tan(gaze)/half_y`,
    // which made lens-axis gaze (gaze=identity → direction (0, 0, -1)) map to screen
    // center (0, 0). That was wrong: with an asymmetric frustum, the lens optical axis is
    // NOT at the screen's geometric center — it sits offset by `center_tan_y / half_y`. The
    // foveation high-res region should follow the user's actual on-screen fixation point,
    // which is that offset, not zero.
    //
    // For up=0.9, down=-0.6: tan_up≈1.260, tan_down≈-0.684, center_tan_y≈0.288,
    // half_y≈0.972 → expected normalized_y ≈ -0.296.
    #[test]
    fn asymmetric_fov_projects_lens_axis_to_geometric_offset() {
        let mut tracker = FoveationTracker::new();
        let fov = Fov {
            left: -0.8,
            right: 0.8,
            up: 0.9,
            down: -0.6,
        };
        let center = tracker.update(Quat::IDENTITY, fov, &cfg(100.0));
        let tan_up = (0.9_f32).tan();
        let tan_down = (-0.6_f32).tan();
        let expected_y = -(tan_down + tan_up) / (tan_up - tan_down);
        assert!(center[0].abs() < 1e-5, "expected x≈0, got {}", center[0]);
        assert!(
            (center[1] - expected_y).abs() < 1e-4,
            "expected y≈{expected_y}, got {}",
            center[1]
        );
    }

    // A fast rotation between consecutive samples should trip the saccade threshold and
    // snap the output toward (0, 0). The rotation here is ~1 rad in ~16 ms ≈ 62.5 rad/s,
    // far above the default 8 rad/s threshold.
    #[test]
    fn fast_rotation_triggers_saccade_snap() {
        let mut tracker = FoveationTracker::new();
        let fov = symmetric_fov(0.8);

        // Seed with a gaze that maps to a non-zero center so the saccade snap is observable.
        let off_axis = Quat::from_euler(EulerRot::YXZ, 0.3, 0.0, 0.0);
        let first = tracker.update(off_axis, fov, &cfg(8.0));
        assert!(
            first[0].abs() > 0.1,
            "seed gaze should produce a non-zero raw center"
        );

        // Force enough wall-clock to pass so dt is realistic but the angular velocity is
        // still high. We rely on Instant::now()'s tick — sleep a frame.
        std::thread::sleep(std::time::Duration::from_millis(16));

        // Rotate ~1 radian in ~16 ms ≈ 62.5 rad/s — well over threshold.
        let snapped_gaze = Quat::from_euler(EulerRot::YXZ, 0.3 + 1.0, 0.0, 0.0);
        let snapped = tracker.update(snapped_gaze, fov, &cfg(8.0));
        assert!(
            snapped[0].abs() < 1e-5,
            "saccade should snap x to 0, got {}",
            snapped[0]
        );
        assert!(
            snapped[1].abs() < 1e-5,
            "saccade should snap y to 0, got {}",
            snapped[1]
        );
    }
}
