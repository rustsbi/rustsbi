use core::{marker::PhantomData, ptr};

pub struct Uart16550U8<'a> {
    pub(crate) base: *const uart16550::Uart16550<u8>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Uart16550U8<'a> {
    pub const unsafe fn new(addr: usize) -> Self {
        Self {
            base: ptr::without_provenance(addr),
            _marker: PhantomData,
        }
    }
}

unsafe impl<'a> Sync for Uart16550U8<'a> {}
