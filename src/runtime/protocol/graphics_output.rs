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

    width: u32,
    height: u32,
    fb_base_vaddr: usize,
    fb_size: usize,
    stride_pixels: u32,
}

impl GraphicsOutput {
    pub fn new(
        width: u32,
        height: u32,
        fb_base_vaddr: usize,
        fb_size: usize,
        stride_pixels: u32,
    ) -> Self {
        let info = GraphicsOutputModeInformation {
            version: 0,
            horizontal_resolution: width,
            vertical_resolution: height,
            pixel_format: GraphicsPixelFormat::PIXEL_BLUE_GREEN_RED_RESERVED_8_BIT_PER_COLOR,
            pixel_information: PixelBitmask {
                red: 0,
                green: 0,
                blue: 0,
                reserved: 0,
            },
            pixels_per_scan_line: stride_pixels,
        };
        let info_box = Box::into_raw(Box::new(info));

        let mode = GraphicsOutputProtocolMode {
            max_mode: 1,
            mode: 0,
            info: info_box,
            size_of_info: size_of::<GraphicsOutputModeInformation>(),
            frame_buffer_base: fb_base_vaddr as u64,
            frame_buffer_size: fb_size,
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
            width,
            height,
            fb_base_vaddr,
            fb_size,
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
            drop(Box::from_raw(self.protocol_raw));
        }
    }
}

#[inline]
fn with_go<F, R>(_this: *const GraphicsOutputProtocol, f: F) -> Option<R>
where
    F: FnOnce(&GraphicsOutput) -> R,
{
    GRAPHICS_OUTPUT.get().map(|m| {
        let lock = m.lock();
        f(&*lock)
    })
}

pub fn init_graphics_output() {
    #[cfg(feature = "display")]
    {
        let display_info = axdisplay::framebuffer_info();
        info!("Graphics Output Protocol initialized: {:?}", display_info);

        let frame_buffer_base = display_info.fb_base_vaddr;
        let frame_buffer_size = display_info.fb_size;

        unsafe {
            core::ptr::write_bytes(frame_buffer_base as *mut u8, 0xFF, frame_buffer_size);
        }

        axdisplay::framebuffer_flush();

        GRAPHICS_OUTPUT.init_once(Mutex::new(GraphicsOutput::new(
            display_info.width,
            display_info.height,
            display_info.fb_base_vaddr,
            display_info.fb_size,
            // FIXME: in most real device environments, pixels are not equal to width.
            // refer: https://docs.rs/bootloader_api/latest/bootloader_api/info/struct.FrameBufferInfo.html#structfield.stride
            display_info.width,
        )));
    }
}

pub unsafe extern "efiapi" fn query_mode(
    this: *const GraphicsOutputProtocol,
    mode_number: u32,
    size_of_info: *mut usize,
    info: *mut *const GraphicsOutputModeInformation,
) -> Status {
    if mode_number != 0 {
        return Status::UNSUPPORTED;
    }
    if size_of_info.is_null() || info.is_null() {
        return Status::INVALID_PARAMETER;
    }

    match with_go(this, |go| (go.mode_box, go.info_box)) {
        Some((mode_box, info_box)) => unsafe {
            let mode = &*mode_box;
            *size_of_info = mode.size_of_info;
            *info = info_box as *const GraphicsOutputModeInformation;
            Status::SUCCESS
        },
        None => Status::DEVICE_ERROR,
    }
}

pub unsafe extern "efiapi" fn set_mode(
    this: *mut GraphicsOutputProtocol,
    mode_number: u32,
) -> Status {
    if mode_number != 0 {
        return Status::UNSUPPORTED;
    }
    match with_go(this, |go| go.mode_box) {
        Some(mode_box) => {
            let mode = unsafe { &mut *mode_box };
            mode.mode = 0;
            // TODO: if resolution switching is supported in the future,
            // info / stride / fb will be updated here, etc.
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
    width: usize,
    height: usize,
    delta: usize,
) -> Status {
    let Some((fb_base, fb_size, w, h, stride_px)) = with_go(this, |go| {
        (
            go.fb_base_vaddr,
            go.fb_size,
            go.width as usize,
            go.height as usize,
            go.stride_pixels as usize,
        )
    }) else {
        return Status::DEVICE_ERROR;
    };

    if width == 0 || height == 0 {
        return Status::SUCCESS;
    }

    if destination_x >= w
        || destination_y >= h
        || destination_x + width > w
        || destination_y + height > h
    {
        return Status::INVALID_PARAMETER;
    }

    let bytes_per_pixel = 4usize;
    let fb_pitch = stride_px * bytes_per_pixel;
    let fb = fb_base as *mut u8;

    unsafe {
        match blt_operation {
            GraphicsOutputBltOperation::BLT_VIDEO_FILL => {
                // fill (dest_x, dest_y, width, height) with the color of blt_buffer[0]
                if blt_buffer.is_null() {
                    return Status::INVALID_PARAMETER;
                }
                let px = *blt_buffer;
                let color = [px.blue, px.green, px.red, 0]; // BGRA
                for row in 0..height {
                    let row_ptr =
                        fb.add((destination_y + row) * fb_pitch + destination_x * bytes_per_pixel);
                    for col in 0..width {
                        let p = row_ptr.add(col * bytes_per_pixel);
                        // 写 B,G,R,A
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

                // Each row in BLT buffer: if Delta == 0, tightly packed (Width * 4 bytes).
                let dst_pitch = if delta == 0 {
                    width * bytes_per_pixel
                } else {
                    delta
                };

                for row in 0..height {
                    // Source: read pixels from framebuffer at (source_x, source_y + row).
                    let src = fb.add((source_y + row) * fb_pitch + source_x * bytes_per_pixel);

                    // Destination: write into BLT buffer at (destination_x, destination_y + row).
                    let dst = (blt_buffer as *mut u8)
                        .add((destination_y + row) * dst_pitch + destination_x * bytes_per_pixel);

                    // Copy one scan line (Width * 4 bytes).
                    core::ptr::copy_nonoverlapping(src, dst, width * bytes_per_pixel);
                }

                Status::SUCCESS
            }
            GraphicsOutputBltOperation::BLT_BUFFER_TO_VIDEO => {
                if blt_buffer.is_null() {
                    return Status::INVALID_PARAMETER;
                }
                // in UEFI GOP spec, Delta = bytes per scan line in the BLT buffer.
                // if Delta == 0, it means the buffer is tightly packed: Width * sizeof(BltPixel) = Width * 4.
                let src_pitch = if delta == 0 {
                    width * bytes_per_pixel
                } else {
                    delta
                };

                for row in 0..height {
                    let dst =
                        fb.add((destination_y + row) * fb_pitch + destination_x * bytes_per_pixel);
                    let src = (blt_buffer as *const u8)
                        .add((source_y + row) * src_pitch + source_x * bytes_per_pixel);
                    core::ptr::copy_nonoverlapping(src, dst, width * bytes_per_pixel);
                }
                axdisplay::framebuffer_flush();
                Status::SUCCESS
            }
            GraphicsOutputBltOperation::BLT_VIDEO_TO_VIDEO => {
                // internal memory transfer, supporting overlap (memmove semantics)
                let src_x = source_x;
                let src_y = source_y;
                if src_x >= w || src_y >= h || src_x + width > w || src_y + height > h {
                    return Status::INVALID_PARAMETER;
                }
                for row in 0..height {
                    let dst =
                        fb.add((destination_y + row) * fb_pitch + destination_x * bytes_per_pixel);
                    let src = fb.add((src_y + row) * fb_pitch + src_x * bytes_per_pixel);
                    core::ptr::copy(src, dst, width * bytes_per_pixel);
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
    _width: usize,
    _height: usize,
    _delta: usize,
) -> Status {
    Status::UNSUPPORTED
}
