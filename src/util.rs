//! useful structures

use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::Pointee,
};

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
                (*self.meta.get()) = MaybeUninit::new(core::ptr::metadata(ptr));
                #[cfg(target_pointer_width = "32")]
                asm!(
                    "amoswap.w.rl zero, {src}, ({dst})",
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
        &*core::ptr::from_raw_parts(ptr, (*self.meta.get()).assume_init())
    }
}
