//! useful structures

// Ref: once_cell

use alloc::boxed::Box;
use core::{
    arch::asm,
    cell::UnsafeCell,
    fmt::{self, Debug},
    marker::PhantomData,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::{self, Pointee},
};

/// A thread-safe fat pointer cell which can be written to only once.
pub struct OnceFatBox<T: ?Sized> {
    thin_ptr: UnsafeCell<*mut ()>,
    lock: UnsafeCell<u32>,
    meta: MaybeUninit<<T as Pointee>::Metadata>,
    _marker: PhantomData<Option<Box<T>>>,
}

impl<T: ?Sized> Default for OnceFatBox<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> Drop for OnceFatBox<T> {
    #[inline]
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
    <T as Pointee>::Metadata: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OnceFatBox")
            .field("address", &self.thin_ptr)
            .field("meta", &self.meta)
            .finish()
    }
}

impl<T: ?Sized> OnceFatBox<T> {
    /// Creates a new empty cell.
    #[inline]
    pub const fn new() -> OnceFatBox<T> {
        OnceFatBox {
            thin_ptr: UnsafeCell::new(ptr::null_mut()),
            lock: UnsafeCell::new(0),
            meta: MaybeUninit::uninit(), // value meaning ignored when thin_ptr is null
            _marker: PhantomData,
        }
    }

    /// Gets a reference to the underlying value.
    #[inline]
    pub fn get(&self) -> Option<&T> {
        let data_address = self.thin_ptr.get();
        if data_address.is_null() {
            return None;
        }
        let metadata = unsafe { self.meta.assume_init() };
        let fat_ptr: *const T = ptr::from_raw_parts(unsafe { *data_address }, metadata);
        Some(unsafe { &*fat_ptr })
    }

    /// Sets the contents of this cell to `value`.
    ///
    /// Returns `Ok(())` if the cell was empty and `Err(value)` if it was full.
    #[inline]
    pub fn set(&self, value: Box<T>) -> Result<(), Box<T>> {
        let fat_ptr = Box::into_raw(value);
        let data_address = fat_ptr as *mut ();
        // compare-exchange using amo operations
        let exchange = unsafe {
            let mut ans = Err(());
            asm!(
                "li     {one}, 1",
                "1: lw  {tmp}, ({lock})", // check if lock is held
                // "call   {relax}", // spin loop hint
                "bnez   {tmp}, 1b", // retry if held
                "amoswap.w.aq {tmp}, {one}, ({lock})", // attempt to acquire lock
                "bnez   {tmp}, 1b", // retry if held
                lock = in(reg) self.lock.get(),
                tmp = out(reg) _,
                one = out(reg) _,
                // relax = sym pause,
                options(nostack)
            );
            // critical section begin
            if (*self.thin_ptr.get()).is_null() {
                // not 'self.thin_ptr.get().is_null()'
                *self.thin_ptr.get() = data_address;
                *(self.meta.as_ptr() as *mut _) = ptr::metadata(fat_ptr);
                ans = Ok(())
            }
            // critical section end
            asm!(
                "amoswap.w.rl x0, x0, ({lock})", // release lock by storing 0
                lock = in(reg) self.lock.get(),
            );
            ans
        };
        if exchange.is_err() {
            let value = unsafe { Box::from_raw(fat_ptr) };
            return Err(value);
        }
        Ok(())
    }
}

unsafe impl<T: Sync + Send + ?Sized> Sync for OnceFatBox<T> {}

/// Use only amo instructions on mutex; no lr/sc instruction is used
pub struct AmoMutex<T: ?Sized> {
    lock: UnsafeCell<u32>,
    data: UnsafeCell<T>,
}

pub struct AmoMutexGuard<'a, T: ?Sized> {
    lock: *mut u32,
    data: &'a mut T,
}

impl<T> AmoMutex<T> {
    /// Create a new AmoMutex
    #[inline]
    pub const fn new(data: T) -> Self {
        AmoMutex {
            lock: UnsafeCell::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Locks the mutex and returns a guard that permits access to the inner data.
    #[inline]
    pub fn lock(&self) -> AmoMutexGuard<T> {
        let lock = self.lock.get();
        unsafe {
            asm!(
                "1: lw           {tmp}, ({lock})",        // check if lock is held
                "   bnez         {tmp}, 1b",              // retry if held
                "   amoswap.w.aq {tmp}, {one}, ({lock})", // attempt to acquire lock
                "   bnez         {tmp}, 1b",              // retry if held
                tmp  = out(reg) _,
                lock =  in(reg) lock,
                one  =  in(reg) 1,
                options(nostack)
            );
        }
        AmoMutexGuard {
            lock,
            data: unsafe { &mut *self.data.get() },
        }
    }
}

unsafe impl<T: ?Sized + Send> Sync for AmoMutex<T> {}
unsafe impl<T: ?Sized + Send> Send for AmoMutex<T> {}

impl<'a, T: ?Sized> Deref for AmoMutexGuard<'a, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        self.data
    }
}

impl<'a, T: ?Sized> DerefMut for AmoMutexGuard<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

impl<'a, T: ?Sized> Drop for AmoMutexGuard<'a, T> {
    /// The dropping of the mutex guard will release the lock it was created from.
    #[inline]
    fn drop(&mut self) {
        unsafe {
            asm!(
                "amoswap.w.rl zero, zero, ({lock})", // release lock by storing 0
                lock = in(reg) self.lock,
            );
        }
    }
}

/// 只使用 AMO 指令的一次初始化指针。
pub struct AmoOncePtr<T: ?Sized> {
    /// As atomic bool, to check if it is the first time to set `ptr`.
    lock: UnsafeCell<u32>,
    ptr: UnsafeCell<*const ()>,
    meta: UnsafeCell<MaybeUninit<<T as Pointee>::Metadata>>,
}

unsafe impl<T: ?Sized + Send> Send for AmoOncePtr<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for AmoOncePtr<T> {}

impl<T: ?Sized> AmoOncePtr<T> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            lock: UnsafeCell::new(0),
            ptr: UnsafeCell::new(core::ptr::null()),
            meta: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub fn try_call_once(&self, r#ref: &'static T) -> bool {
        let ptr = r#ref as *const T;
        let locked: u32;
        unsafe {
            asm!(
                "
                lw           {locked}, ({lock})
                bnez         {locked}, 1f
                amoswap.w.aq {locked}, {one}, ({lock})
                1: ",
                lock   =  in(reg) self.lock.get(),
                one    =  in(reg) 1,
                locked = out(reg) locked,
            );
        }
        if locked == 0 {
            // 取得锁，初始化对象
            unsafe {
                // amo rl 保证先初始化 meta 后设置指针
                (*self.meta.get()) = MaybeUninit::new(ptr::metadata(ptr));
                #[cfg(target_pointer_width = "32")]
                asm!(
                    "amoswap.d.rl zero, {src}, ({dst})",
                    src = in(reg) ptr as *const (),
                    dst = in(reg) self.ptr.get(),
                );
                #[cfg(target_pointer_width = "64")]
                asm!(
                    "amoswap.d.rl zero, {src}, ({dst})",
                    src = in(reg) ptr as *const (),
                    dst = in(reg) self.ptr.get(),
                );
            }
            true
        } else {
            // 未取得锁，对象已被初始化过
            false
        }
    }

    #[allow(unused)]
    pub fn call_once(&self, r#ref: &'static T) -> Result<&T, &T> {
        if self.try_call_once(r#ref) {
            Ok(r#ref)
        } else {
            Err(self.wait())
        }
    }

    pub fn wait(&self) -> &T {
        loop {
            // 反复读直到非空。
            let ptr = unsafe { *self.ptr.get() };
            if !ptr.is_null() {
                return unsafe { self.build_ref_unchecked(ptr) };
            }
        }
    }

    pub fn get(&self) -> Option<&T> {
        let ptr: *const ();
        unsafe {
            // 先获取指针。如果指针非空，元数据一定存在。
            // FIXME AMO 设的值是否一定对 LD 可见？如果确定就不需要 AMO 读了。
            #[cfg(target_pointer_width = "32")]
            asm!(" lw          {dst}, ({src})
                   bnez        {dst}, 1f
                   amoadd.w.aq {dst}, zero, ({src})
                1: ",
                src =  in(reg) self.ptr.get(),
                dst = out(reg) ptr,
            );
            #[cfg(target_pointer_width = "64")]
            asm!(" ld          {dst}, ({src})
                   bnez        {dst}, 1f
                   amoadd.d.aq {dst}, zero, ({src})
                1: ",
                src =  in(reg) self.ptr.get(),
                dst = out(reg) ptr,
            );
        }
        if !ptr.is_null() {
            Some(unsafe { self.build_ref_unchecked(ptr) })
        } else {
            None
        }
    }

    /// 利用指针和元数据生成引用。需要保证传入的指针非空。如果能传入非空指针，meta 也一定存在。
    #[inline]
    unsafe fn build_ref_unchecked(&self, ptr: *const ()) -> &T {
        &*ptr::from_raw_parts(ptr, (*self.meta.get()).assume_init())
    }
}
