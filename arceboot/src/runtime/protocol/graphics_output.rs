use alloc::boxed::Box;
use axsync::Mutex;
use lazyinit::LazyInit;

use uefi_raw::{
    Status,
    protocol::console::{
        GraphicsOutputBltOperation, GraphicsOutputBltPixel, GraphicsOutputModeInformation,
        GraphicsOutputProtocol, GraphicsOutputProtocolMode, GraphicsPixelFormat, PixelBitmask,
    },
};

static GRAPHICS_OUTPUT: LazyInit<Mutex<GraphicsOutput>> = LazyInit::new();

#[derive(Debug)]
pub struct GraphicsOutput {
    protocol: &'static mut GraphicsOutputProtocol,
    protocol_raw: *mut GraphicsOutputProtocol,

    mode_box: *mut GraphicsOutputProtocolMode,
    info_box: *mut GraphicsOutputModeInformation,

    width_pixels: u32,
    height_pixels: u32,
    frame_buffer_base_virtual_address: usize,
    frame_buffer_size_bytes: usize,
    stride_pixels: u32,
}

impl GraphicsOutput {
    pub fn new(
        width_pixels: u32,
        height_pixels: u32,
        frame_buffer_base_virtual_address: usize,
        frame_buffer_size_bytes: usize,
        stride_pixels: u32,
    ) -> Self {
        let mode_info = GraphicsOutputModeInformation {
            version: 0,
            horizontal_resolution: width_pixels,
            vertical_resolution: height_pixels,
            pixel_format: GraphicsPixelFormat::PIXEL_BLUE_GREEN_RED_RESERVED_8_BIT_PER_COLOR,
            pixel_information: PixelBitmask {
                red: 0,
                green: 0,
                blue: 0,
                reserved: 0,
            },
            pixels_per_scan_line: stride_pixels,
        };
        let info_box = Box::into_raw(Box::new(mode_info));

        let mode = GraphicsOutputProtocolMode {
            max_mode: 1,
            mode: 0,
            info: info_box,
            size_of_info: core::mem::size_of::<GraphicsOutputModeInformation>(),
            frame_buffer_base: frame_buffer_base_virtual_address as u64,
            frame_buffer_size: frame_buffer_size_bytes,
        };
        let mode_box = Box::into_raw(Box::new(mode));

        let protocol = GraphicsOutputProtocol {
            query_mode,
            set_mode,
            blt,
            mode: mode_box,
        };

        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };

        Self {
            protocol,
            protocol_raw,
            mode_box,
            info_box,
            width_pixels,
            height_pixels,
            frame_buffer_base_virtual_address,
            frame_buffer_size_bytes,
            stride_pixels,
        }
    }

    pub fn get_protocol(&self) -> *mut GraphicsOutputProtocol {
        self.protocol_raw
    }
}

unsafe impl Send for GraphicsOutput {}
unsafe impl Sync for GraphicsOutput {}

impl Drop for GraphicsOutput {
    fn drop(&mut self) {
        unsafe {
            // Free in reverse construction order to avoid leaks.
            drop(Box::from_raw(self.protocol_raw));
            drop(Box::from_raw(self.mode_box));
            drop(Box::from_raw(self.info_box));
        }
    }
}

#[inline]
fn with_graphics_output<F, R>(_this: *const GraphicsOutputProtocol, f: F) -> Option<R>
where
    F: FnOnce(&GraphicsOutput) -> R,
{
    GRAPHICS_OUTPUT.get().map(|guard| {
        let lock = guard.lock();
        f(&*lock)
    })
}

pub fn init_graphics_output() {
    #[cfg(feature = "display")]
    {
        let display_info = axdisplay::framebuffer_info();
        info!("Graphics Output Protocol initialized: {:?}", display_info);

        let frame_buffer_base_virtual_address = display_info.fb_base_vaddr;
        let frame_buffer_size_bytes = display_info.fb_size;

        // Paint the framebuffer white for a quick visual check.
        unsafe {
            core::ptr::write_bytes(
                frame_buffer_base_virtual_address as *mut u8,
                0xFF,
                frame_buffer_size_bytes,
            );
        }
        axdisplay::framebuffer_flush();

        GRAPHICS_OUTPUT.init_once(Mutex::new(GraphicsOutput::new(
            display_info.width,
            display_info.height,
            display_info.fb_base_vaddr,
            display_info.fb_size,
            // Note: on many devices, stride (pixels per scan line) differs from width.
            // Here we temporarily set it to width.
            display_info.width,
        )));
    }
}

pub unsafe extern "efiapi" fn query_mode(
    this: *const GraphicsOutputProtocol,
    mode_number: u32,
    size_of_info_out: *mut usize,
    info_out: *mut *const GraphicsOutputModeInformation,
) -> Status {
    if mode_number != 0 {
        return Status::UNSUPPORTED;
    }
    if size_of_info_out.is_null() || info_out.is_null() {
        return Status::INVALID_PARAMETER;
    }

    unsafe {
        match with_graphics_output(this, |go| (go.mode_box, go.info_box)) {
            Some((mode_box, info_box)) => {
                let mode = &*mode_box;
                *size_of_info_out = mode.size_of_info;
                *info_out = info_box as *const GraphicsOutputModeInformation;
                Status::SUCCESS
            }
            None => Status::DEVICE_ERROR,
        }
    }
}

pub unsafe extern "efiapi" fn set_mode(
    this: *mut GraphicsOutputProtocol,
    mode_number: u32,
) -> Status {
    if mode_number != 0 {
        return Status::UNSUPPORTED;
    }
    match with_graphics_output(this, |go| go.mode_box) {
        Some(mode_box) => {
            let mode = unsafe { &mut *mode_box };
            mode.mode = 0;
            // If resolution switching is added in the future, update info/stride/framebuffer here.
            Status::SUCCESS
        }
        None => Status::DEVICE_ERROR,
    }
}

#[cfg(feature = "display")]
pub unsafe extern "efiapi" fn blt(
    this: *mut GraphicsOutputProtocol,
    blt_buffer: *mut GraphicsOutputBltPixel,
    blt_operation: GraphicsOutputBltOperation,
    source_x: usize,
    source_y: usize,
    destination_x: usize,
    destination_y: usize,
    width_pixels: usize,
    height_pixels: usize,
    blt_buffer_delta_bytes: usize,
) -> Status {
    let Some((
        frame_buffer_base_virtual_address,
        _frame_buffer_size_bytes,
        screen_width_pixels,
        screen_height_pixels,
        stride_pixels,
    )) = with_graphics_output(this, |go| {
        (
            go.frame_buffer_base_virtual_address,
            go.frame_buffer_size_bytes,
            go.width_pixels as usize,
            go.height_pixels as usize,
            go.stride_pixels as usize,
        )
    })
    else {
        return Status::DEVICE_ERROR;
    };

    if width_pixels == 0 || height_pixels == 0 {
        return Status::SUCCESS;
    }

    if destination_x >= screen_width_pixels
        || destination_y >= screen_height_pixels
        || destination_x + width_pixels > screen_width_pixels
        || destination_y + height_pixels > screen_height_pixels
    {
        return Status::INVALID_PARAMETER;
    }

    const BYTES_PER_PIXEL: usize = 4;
    let frame_buffer_pitch_bytes = stride_pixels * BYTES_PER_PIXEL;
    let frame_buffer_ptr = frame_buffer_base_virtual_address as *mut u8;

    unsafe {
        match blt_operation {
            GraphicsOutputBltOperation::BLT_VIDEO_FILL => {
                // Fill (destination_x, destination_y, width, height) with blt_buffer[0].
                if blt_buffer.is_null() {
                    return Status::INVALID_PARAMETER;
                }
                let px = *blt_buffer;
                let color = [px.blue, px.green, px.red, 0]; // BGRA
                for row in 0..height_pixels {
                    let row_ptr = frame_buffer_ptr.add(
                        (destination_y + row) * frame_buffer_pitch_bytes
                            + destination_x * BYTES_PER_PIXEL,
                    );
                    for col in 0..width_pixels {
                        let p = row_ptr.add(col * BYTES_PER_PIXEL);
                        *p.add(0) = color[0];
                        *p.add(1) = color[1];
                        *p.add(2) = color[2];
                        *p.add(3) = color[3];
                    }
                }
                axdisplay::framebuffer_flush();
                Status::SUCCESS
            }

            GraphicsOutputBltOperation::BLT_VIDEO_TO_BLT_BUFFER => {
                if blt_buffer.is_null() {
                    return Status::INVALID_PARAMETER;
                }
                // BLT buffer line stride: if Delta == 0, it is tightly packed (width * 4).
                let destination_pitch_bytes = if blt_buffer_delta_bytes == 0 {
                    width_pixels * BYTES_PER_PIXEL
                } else {
                    blt_buffer_delta_bytes
                };

                for row in 0..height_pixels {
                    // Source: read from framebuffer at (source_x, source_y + row).
                    let src = frame_buffer_ptr.add(
                        (source_y + row) * frame_buffer_pitch_bytes + source_x * BYTES_PER_PIXEL,
                    );
                    // Destination: write into BLT buffer at (destination_x, destination_y + row).
                    let dst = (blt_buffer as *mut u8).add(
                        (destination_y + row) * destination_pitch_bytes
                            + destination_x * BYTES_PER_PIXEL,
                    );
                    core::ptr::copy_nonoverlapping(src, dst, width_pixels * BYTES_PER_PIXEL);
                }
                Status::SUCCESS
            }

            GraphicsOutputBltOperation::BLT_BUFFER_TO_VIDEO => {
                if blt_buffer.is_null() {
                    return Status::INVALID_PARAMETER;
                }
                // In GOP, Delta is bytes per scan line in the BLT buffer; 0 means tightly packed.
                let source_pitch_bytes = if blt_buffer_delta_bytes == 0 {
                    width_pixels * BYTES_PER_PIXEL
                } else {
                    blt_buffer_delta_bytes
                };

                for row in 0..height_pixels {
                    let dst = frame_buffer_ptr.add(
                        (destination_y + row) * frame_buffer_pitch_bytes
                            + destination_x * BYTES_PER_PIXEL,
                    );
                    let src = (blt_buffer as *const u8)
                        .add((source_y + row) * source_pitch_bytes + source_x * BYTES_PER_PIXEL);
                    core::ptr::copy_nonoverlapping(src, dst, width_pixels * BYTES_PER_PIXEL);
                }
                axdisplay::framebuffer_flush();
                Status::SUCCESS
            }

            GraphicsOutputBltOperation::BLT_VIDEO_TO_VIDEO => {
                // Internal framebuffer transfer; overlapping is allowed (memmove semantics).
                let source_x_pixels = source_x;
                let source_y_pixels = source_y;
                if source_x_pixels >= screen_width_pixels
                    || source_y_pixels >= screen_height_pixels
                    || source_x_pixels + width_pixels > screen_width_pixels
                    || source_y_pixels + height_pixels > screen_height_pixels
                {
                    return Status::INVALID_PARAMETER;
                }
                for row in 0..height_pixels {
                    let dst = frame_buffer_ptr.add(
                        (destination_y + row) * frame_buffer_pitch_bytes
                            + destination_x * BYTES_PER_PIXEL,
                    );
                    let src = frame_buffer_ptr.add(
                        (source_y_pixels + row) * frame_buffer_pitch_bytes
                            + source_x_pixels * BYTES_PER_PIXEL,
                    );
                    core::ptr::copy(src, dst, width_pixels * BYTES_PER_PIXEL);
                }
                axdisplay::framebuffer_flush();
                Status::SUCCESS
            }

            _ => Status::UNSUPPORTED,
        }
    }
}

#[cfg(not(feature = "display"))]
pub unsafe extern "efiapi" fn blt(
    _this: *mut GraphicsOutputProtocol,
    _blt_buffer: *mut GraphicsOutputBltPixel,
    _blt_operation: GraphicsOutputBltOperation,
    _source_x: usize,
    _source_y: usize,
    _destination_x: usize,
    _destination_y: usize,
    _width_pixels: usize,
    _height_pixels: usize,
    _blt_buffer_delta_bytes: usize,
) -> Status {
    Status::UNSUPPORTED
}
