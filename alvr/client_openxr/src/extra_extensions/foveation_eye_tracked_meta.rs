use openxr::{
    self as xr,
    sys::{self, Handle},
};
use std::ptr;

// `openxr` 0.21 exposes the META eye-tracked foveation extension fps but does not let callers
// chain `XrFoveationEyeTrackedProfileCreateInfoMETA` into `create_foveation_profile`. Build the
// raw call ourselves and hand back a high-level `xr::FoveationProfileFB` so the rest of the
// client doesn't need to know which extension produced it.
pub fn create_eye_tracked_profile<G>(
    session: &xr::Session<G>,
    level: xr::FoveationLevelFB,
    vertical_offset: f32,
    dynamic: xr::FoveationDynamicFB,
) -> xr::Result<xr::FoveationProfileFB> {
    let exts = session.instance().exts();
    let fb_fns = exts
        .fb_foveation
        .ok_or(sys::Result::ERROR_EXTENSION_NOT_PRESENT)?;
    // Presence of the META extension is what makes the chained struct legal.
    exts.meta_foveation_eye_tracked
        .ok_or(sys::Result::ERROR_EXTENSION_NOT_PRESENT)?;

    let eye_tracked = sys::FoveationEyeTrackedProfileCreateInfoMETA {
        ty: sys::FoveationEyeTrackedProfileCreateInfoMETA::TYPE,
        next: ptr::null(),
        flags: sys::FoveationEyeTrackedProfileCreateFlagsMETA::EMPTY,
    };
    let level_profile = sys::FoveationLevelProfileCreateInfoFB {
        ty: sys::FoveationLevelProfileCreateInfoFB::TYPE,
        next: (&raw const eye_tracked).cast::<std::ffi::c_void>().cast_mut(),
        level,
        vertical_offset,
        dynamic,
    };
    let create_info = sys::FoveationProfileCreateInfoFB {
        ty: sys::FoveationProfileCreateInfoFB::TYPE,
        next: (&raw const level_profile)
            .cast::<std::ffi::c_void>()
            .cast_mut(),
    };

    let mut handle = sys::FoveationProfileFB::NULL;
    unsafe {
        super::xr_res((fb_fns.create_foveation_profile)(
            session.as_raw(),
            &create_info,
            &mut handle,
        ))?;
    }

    // SAFETY: handle was just created against this session's instance.
    Ok(unsafe { xr::FoveationProfileFB::from_raw(session.instance().clone(), handle) })
}
