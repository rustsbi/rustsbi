//! Device descriptions and hardware drivers.

mod description;
pub(crate) mod device;
mod matching;
pub(crate) mod platform;

pub(crate) use description::Description;
pub(crate) use matching::match_desc;
