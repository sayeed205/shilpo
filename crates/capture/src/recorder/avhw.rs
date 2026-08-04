//! VAAPI hardware device and frame-context helpers for the in-process
//! recorder.
//!
//! Adapted from `wl-screenrec` (`src/avhw.rs`), licensed under the Apache
//! License, Version 2.0. Copyright (c) wl-screenrec contributors.
//! This file is a derived work distributed under the Apache-2.0 license.

use std::{ffi::CString, path::Path, ptr::null_mut};

use ffmpeg::{
    dict,
    ffi::{
        AVHWFramesContext, av_buffer_ref, av_buffer_unref, av_hwdevice_ctx_create,
        av_hwframe_ctx_alloc, av_hwframe_ctx_init, av_hwframe_get_buffer,
    },
    format::Pixel,
    frame,
};

/// `DRM_FORMAT_MOD_LINEAR`: the compositor must hand us linear surfaces.
const DRM_FORMAT_MOD_LINEAR: u64 = 0;

pub struct AvHwDevCtx {
    ptr: *mut ffmpeg::sys::AVBufferRef,
}

pub enum Tiling<'a> {
    Optimal,
    Drm(&'a [u64]),
}

impl AvHwDevCtx {
    /// Open a VAAPI device backed by a DRM render node, using the DRM
    /// connection type (matches how `ffmpeg -hwaccel vaapi` interoperates
    /// with the buffers the compositor hands us).
    pub fn new_libva(dri_device: &Path) -> Result<Self, ffmpeg::Error> {
        unsafe {
            let mut hw_device_ctx = null_mut();

            let opts = dict! {
                "connection_type" => "drm"
            };

            let dev_cstr = CString::new(dri_device.to_str().unwrap()).unwrap();
            let sts = av_hwdevice_ctx_create(
                &mut hw_device_ctx,
                ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
                dev_cstr.as_ptr(),
                opts.as_mut_ptr(),
                0,
            );

            if sts != 0 {
                Err(ffmpeg::Error::from(sts))
            } else {
                Ok(Self { ptr: hw_device_ctx })
            }
        }
    }

    pub fn create_frame_ctx(
        &mut self,
        pixfmt: Pixel,
        width: i32,
        height: i32,
        tiling: Tiling,
    ) -> Result<AvHwFrameCtx, ffmpeg::Error> {
        unsafe {
            let mut hwframe = av_hwframe_ctx_alloc(self.ptr as *mut _);
            let hwframe_casted = &mut *((*hwframe).data as *mut AVHWFramesContext);

            // ffmpeg does not expose RGB vaapi
            hwframe_casted.format = Pixel::VAAPI.into();
            hwframe_casted.sw_format = pixfmt.into();
            hwframe_casted.width = width;
            hwframe_casted.height = height;
            hwframe_casted.initial_pool_size = 5;

            // If the compositor cannot provide a linear surface for capture
            // (only tiled scanout modifiers), we still request the frame via
            // the driver; `av_hwframe_map` will copy if it must. Only reject
            // when the modifier set contains no linear candidate at all.
            if let Tiling::Drm(modifiers) = tiling
                && !modifiers.contains(&DRM_FORMAT_MOD_LINEAR)
            {
                return Err(ffmpeg::Error::InvalidData);
            }

            let sts = av_hwframe_ctx_init(hwframe);
            if sts != 0 {
                av_buffer_unref(&mut hwframe);
                return Err(ffmpeg::Error::from(sts));
            }

            let ret = Ok(AvHwFrameCtx {
                ptr: av_buffer_ref(hwframe),
            });

            av_buffer_unref(&mut hwframe);

            ret
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut ffmpeg::sys::AVBufferRef {
        self.ptr
    }
}

impl Drop for AvHwDevCtx {
    fn drop(&mut self) {
        unsafe {
            av_buffer_unref(&mut self.ptr);
        }
    }
}

pub struct AvHwFrameCtx {
    ptr: *mut ffmpeg::sys::AVBufferRef,
}

impl Drop for AvHwFrameCtx {
    fn drop(&mut self) {
        unsafe {
            av_buffer_unref(&mut self.ptr);
        }
    }
}

impl AvHwFrameCtx {
    pub fn alloc(&mut self) -> Result<frame::Video, ffmpeg::Error> {
        let mut frame = ffmpeg::frame::video::Video::empty();
        match unsafe { av_hwframe_get_buffer(self.ptr, frame.as_mut_ptr(), 0) } {
            0 => Ok(frame),
            e => Err(ffmpeg::Error::from(e)),
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut ffmpeg::sys::AVBufferRef {
        self.ptr
    }
}
