cfg_if::cfg_if! {
    if #[cfg(feature = "ramdisk_cpio")] {
        mod ramdisk_cpio;
        pub use ramdisk_cpio::*;
    } else {
        info!("Boot media feature is not enabled.");
    }
}