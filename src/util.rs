//! useful structures

// Ref: once_cell

use core::{
    fmt::{self, Debug}, 
    marker::PhantomData, 
    mem::MaybeUninit, 
    ptr::{self, Pointee}, 
    sync::atomic::{AtomicPtr, Ordering}
};
use alloc::boxed::Box;

/// A thread-safe fat pointer cell which can be written to only once.
pub struct OnceFatBox<T: ?Sized> {
    thin_ptr: AtomicPtr<()>,
    meta: MaybeUninit<<T as Pointee>::Metadata>,
    _marker: PhantomData<Option<Box<T>>>,
}

impl<T: ?Sized> Default for OnceFatBox<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> Drop for OnceFatBox<T> {
    fn drop(&mut self) {
        let data_address = *self.thin_ptr.get_mut();
        if !data_address.is_null() {
            let metadata = unsafe { self.meta.assume_init() };
            let fat_ptr: *mut T = ptr::from_raw_parts_mut(data_address, metadata);
            drop(unsafe { Box::from_raw(fat_ptr) })
        }
    }
}

impl<T: ?Sized + Debug> Debug for OnceFatBox<T> 
where 
    <T as Pointee>::Metadata: Debug 
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OnceFatBox")
            .field("data_address", &self.thin_ptr)
            .field("meta", &self.meta)
            .finish()
    }
}

impl<T: ?Sized> OnceFatBox<T> {
    /// Creates a new empty cell.
    pub const fn new() -> OnceFatBox<T> {
        OnceFatBox { 
            thin_ptr: AtomicPtr::new(ptr::null_mut()), 
            meta: MaybeUninit::uninit(), // value meaning ignored when thin_ptr is null
            _marker: PhantomData 
        }
    }

    /// Gets a reference to the underlying value.
    pub fn get(&self) -> Option<&T> {
        let data_address = self.thin_ptr.load(Ordering::Acquire);
        if data_address.is_null() {
            return None;
        }
        let metadata = unsafe { self.meta.assume_init() };
        let fat_ptr: *const T = ptr::from_raw_parts(data_address, metadata);
        Some(unsafe { &*fat_ptr })
    }

    /// Sets the contents of this cell to `value`.
    ///
    /// Returns `Ok(())` if the cell was empty and `Err(value)` if it was full.
    pub fn set(&self, value: Box<T>) -> Result<(), Box<T>> {
        let fat_ptr = Box::into_raw(value);
        let data_address = fat_ptr as *mut ();
        let exchange = self.thin_ptr.compare_exchange(
            ptr::null_mut(),
            data_address,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if let Err(_) = exchange {
            let value = unsafe { Box::from_raw(fat_ptr) };
            return Err(value);
        }
        // 对once_cell来说这样做是对的，因为其它的线程失败后，不会再更改元数据了。
        // 如果其它的线程仍然需要更改元数据，就不能这样做。
        unsafe {
            *(self.meta.as_ptr() as *mut _) = ptr::metadata(fat_ptr);
        }
        Ok(())
    }
}

unsafe impl<T: Sync + Send + ?Sized> Sync for OnceFatBox<T> {}
