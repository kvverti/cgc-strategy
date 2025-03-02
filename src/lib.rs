#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use core::{marker::PhantomData, ops::Deref};

use heap::{GcStrategy, GcVtable, Handle};
use trace::Trace;

pub mod heap;
pub mod trace;

pub struct GcHeap<'lifetime, S: ?Sized> {
    /// The limiting lifetime of this heap. The compiler infers as small a lifetime as necessary,
    /// but no smaller than the lifetime of the heap itself. This is contravariant - the lifetime of
    /// the heap can always be lengthened, but never shortened. This lifetime is the lower bound of
    /// all objects in the GC heap.
    _lifetime: PhantomData<fn(&'lifetime ())>,
    strategy: S,
}

impl<'lifetime, S: ?Sized + GcStrategy> GcHeap<'lifetime, S> {
    // todo: figure out how allocation should work
    // note: Send bound here because we eventually want to have dropping handled
    pub fn alloc<T: Trace + Send + 'lifetime>(&self, value: T) -> Root<'_, S, T> {
        let vtable = const { GcVtable::for_type::<T>() };
        match self.strategy.allocate(vtable) {
            // SAFETY: the GC heap ensures the allocation is uninitialized and the
            // pointer is suitable for a value of type `T`.
            Some(alloc) => unsafe {
                alloc.ptr.cast::<T>().write(value);
                self.strategy.set_initialized(alloc.handle);
                Root {
                    handle: Gc {
                        handle: alloc.handle,
                        _ph: PhantomData,
                    },
                    gc: &self.strategy,
                }
            },
            None => panic!("out of memory"),
        }
    }

    pub fn strategy(&self) -> &S {
        &self.strategy
    }
}

pub struct Gc<T: ?Sized> {
    /// Handle that represents the underlying GC allocation.
    handle: Handle,
    _ph: PhantomData<fn() -> T>,
}

// if we copy a Gc<T> out of a root, what happens when the root goes away?
// should Gc<T> be Copy at all? Maybe it should just be a storage type, and
// all access comes from Root<T>

impl<T: ?Sized> Clone for Gc<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Gc<T> {}

// Safety: Gc<T> exposes the API of a shared reference (i.e. allows shared access but not dropping)
unsafe impl<T: ?Sized + Sync> Send for Gc<T> {}
unsafe impl<T: ?Sized + Sync> Sync for Gc<T> {}

pub struct Root<'root, S: ?Sized + GcStrategy, T: ?Sized> {
    handle: Gc<T>,
    gc: &'root S,
}

impl<S: ?Sized + GcStrategy, T: ?Sized> Deref for Root<'_, S, T> {
    type Target = Gc<T>;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

/// Unroots the underlying GC when going out of scope.
impl<S: ?Sized + GcStrategy, T: ?Sized> Drop for Root<'_, S, T> {
    fn drop(&mut self) {
        self.gc.unroot(self.handle.handle)
    }
}
