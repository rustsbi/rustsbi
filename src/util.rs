//! useful structures

use core::{arch::asm, cell::UnsafeCell, marker::PhantomData, mem::MaybeUninit, ptr::Pointee};

/// 只使用 AMO 指令的一次初始化引用存储。
pub struct AmoOnceRef<'a, T: ?Sized> {
    /// As atomic bool, to check if it is the first time to set `ptr`.
    lock: UnsafeCell<u32>,
    ptr: UnsafeCell<*const ()>,
    meta: UnsafeCell<MaybeUninit<<T as Pointee>::Metadata>>,
    _lifetime: PhantomData<&'a ()>,
}

/// 如果 AmoOncePtr 保存的引用是静态的，自然可以随意移动。
unsafe impl<T: ?Sized> Send for AmoOnceRef<'static, T> {}

/// AmoOncePtr 不提供锁。
unsafe impl<T: ?Sized + Sync> Sync for AmoOnceRef<'static, T> {}

impl<'a, T: ?Sized> AmoOnceRef<'a, T> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            lock: UnsafeCell::new(0),
            ptr: UnsafeCell::new(core::ptr::null()),
            meta: UnsafeCell::new(MaybeUninit::uninit()),
            _lifetime: PhantomData,
        }
    }

    pub fn try_call_once(&self, r#ref: &'a T) -> bool {
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

    #[inline]
    unsafe fn build_ref_unchecked(&self, ptr: *const ()) -> &T {
        &*core::ptr::from_raw_parts(ptr, (*self.meta.get()).assume_init())
    }
}
