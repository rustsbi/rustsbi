cfg_if::cfg_if! {
    if #[cfg(feature = "ramdisk_cpio")] {
        mod ramdisk_cpio;
        pub use ramdisk_cpio::*;
    } else if #[cfg(feature = "virtiodisk")] {
        mod virtio_disk;
        pub use virtio_disk::*;
    } else {
        info!("Boot media feature is not enabled.");
    }
}
