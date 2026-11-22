use std::cell::Cell;

/// Refcounted heap cell shared by all pointer payloads.
///
/// Mirrors QuickJS refcounted heap objects (`JSString`, `JSObject`, ...):
/// the cell owns `data` and is freed when the refcount drops to zero.
#[repr(C)]
pub struct RefCount<T> {
    refcount: Cell<u32>,
    pub data: T,
}

impl<T> RefCount<T> {
    /// Box `data` into a heap cell holding a single reference.
    pub fn new(data: T) -> *mut Self {
        Box::into_raw(Box::new(Self {
            refcount: Cell::new(1),
            data,
        }))
    }

    /// Acquire one reference (QuickJS `JS_DupValue`).
    ///
    /// # Safety
    /// `ptr` must have been returned by [`RefCount::new`] and the caller
    /// must own one reference.
    pub unsafe fn inc(ptr: *mut Self) {
        let rc = &*ptr;
        rc.refcount.set(rc.refcount.get() + 1);
    }

    /// Release one reference, freeing the cell when it reaches zero
    /// (QuickJS `JS_FreeValue`).
    ///
    /// # Safety
    /// `ptr` must have been returned by [`RefCount::new`] and the caller
    /// must own one reference.
    pub unsafe fn dec(ptr: *mut Self) {
        let rc = &*ptr;
        let remaining = rc.refcount.get() - 1;
        rc.refcount.set(remaining);
        if remaining == 0 {
            drop(Box::from_raw(ptr));
        }
    }

    /// # Safety
    /// `ptr` must be a valid, live cell pointer.
    pub unsafe fn data(ptr: *mut Self) -> &T {
        &(*ptr).data
    }

    /// # Safety
    /// `ptr` must be a valid, live cell pointer, and no other reference
    /// may be used while the returned borrow is alive.
    pub unsafe fn data_mut(ptr: *mut Self) -> &mut T {
        &mut (*ptr).data
    }
}
