include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
include!(concat!(env!("OUT_DIR"), "/generated_payload.rs"));
#[cfg(feature = "fdt")]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
