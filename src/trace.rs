use std::{marker::{PhantomData, PhantomPinned}, rc::Rc, sync::Arc, collections::{VecDeque, LinkedList}};

use crate::{Gc, Handle};

pub struct TraceContext<'a> {
    gc_visitor: &'a dyn Fn(Handle),
}

impl TraceContext<'_> {
    pub fn accept<T: ?Sized>(&self, gc: Gc<T>) {
        (self.gc_visitor)(gc.handle);
    }
}

/// A trait for types that can implement tracing functionality.
///
/// # Expectations on GC Object Types
/// This trait may be implemented on a variety of types, including non-`'static`, non-`Send`, and non-`Sync`
/// types. However, not all types can implement this trait. At a minimum, all GC object types must allow the
/// GC shared access to all nested GC objects at all times. This prevents types that give exclusive access
/// to nested GC objects, such as mutexes, from being GC objects. Implementors should carefully consider how
/// this requirement affects types that are not thread-safe (non-`Sync`) and types that hold borrows (non-`'static`).
/// That being said, types that contain only `Trace` types will generally be able to implement this trait.
///
/// # Finalization
/// GC object types should not rely on timely destruction. The drop glue of a GC object, if it exists, is called a
/// _finalizer_, and may (or may not!) be invoked at any point after it becomes unreachable. The GC does not directly
/// invoke the finalizer of any GC object. Instead, the GC notifies an object's associated finalization queue that
/// the object may be finalized. If a GC object is not registered with a finalization queue, then that object's finalizer
/// will *not* be run before the object's storage is reclaimed.
///
/// A GC object must not access nested GC objects from within its finalizer. If a finalizer attempts to root, pin, or otherwise
/// access the data of any nested GC object, it will result in a panic.
///
/// # Safety
/// Implementations *must* uphold the contracts of all methods. Failure to do so
/// may result in memory corruption or other undefined behavior.
pub unsafe trait Trace {
    /// Mark all GC objects directly reachable from this object.
    ///
    /// # Warning
    /// This method is invoked by the GC in order to determine the reachability of GC objects.
    /// The GC may invoke this method while other threads are concurrently accessing `self` or its fields,
    /// **even if the type of this object is not `Sync`**. It may also invoke this method after any borrows this type
    /// contains have expired.
    fn trace(&self, ctx: &TraceContext<'_>);
}

macro_rules! empty_trace {
    ($($ty:ty)*) => {
        $(
            unsafe impl Trace for $ty {
                fn trace(&self, _: &TraceContext<'_>) {}
            }
        )*
    };
}

empty_trace! {
    u8 u16 u32 u64 u128
    i8 i16 i32 i64 i128
    f32 f64
    char str std::ffi::CStr std::path::Path std::ffi::OsStr
    String std::ffi::CString std::path::PathBuf std::ffi::OsString
    std::any::TypeId
    PhantomPinned
}
empty_trace! { () }

unsafe impl<T: ?Sized> Trace for PhantomData<T> {
    fn trace(&self, _: &TraceContext<'_>) {}
}

/// `Gc<T>` is itself `Trace`! It just forwards itself to the context.
unsafe impl<T: ?Sized> Trace for Gc<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        ctx.accept(*self);
    }
}

/// SAFETY: The referent's trace method is safe to call, and Box imposes no extra requirements.
unsafe impl<T: Trace + ?Sized> Trace for Box<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        (**self).trace(ctx);
    }
}

/// SAFETY: We don't touch the reference counts and only invoke T's trace method.
unsafe impl<T: Trace + ?Sized> Trace for Rc<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        (**self).trace(ctx);
    }
}

/// SAFETY: We only invoke T's trace method and do not touch any of Arc's state.
unsafe impl<T: Trace + ?Sized> Trace for Arc<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        (**self).trace(ctx);
    }
}

/// SAFETY: arrays impose no additional requirements for accessing elements.
unsafe impl<T: Trace, const N: usize> Trace for [T; N] {
    fn trace(&self, ctx: &TraceContext<'_>) {
        for elem in self {
            elem.trace(ctx);
        }
    }
}

/// SAFETY: slices impose no additional requirements for accessing elements.
unsafe impl<T: Trace> Trace for [T] {
    fn trace(&self, ctx: &TraceContext<'_>) {
        for elem in self {
            elem.trace(ctx);
        }
    }
}

/// SAFETY: vec imposes no additional requirements for accessing elements.
unsafe impl<T: Trace> Trace for Vec<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        for elem in self {
            elem.trace(ctx);
        }
    }
}

unsafe impl<T: Trace> Trace for VecDeque<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        for elem in self {
            elem.trace(ctx);
        }
    }
}

unsafe impl<T: Trace> Trace for LinkedList<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        for elem in self {
            elem.trace(ctx);
        }
    }
}

unsafe impl<T: Trace> Trace for Option<T> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        if let Some(v) = self {
            v.trace(ctx);
        }
    }
}

unsafe impl<T: Trace, E: Trace> Trace for Result<T, E> {
    fn trace(&self, ctx: &TraceContext<'_>) {
        match self {
            Ok(t) => t.trace(ctx),
            Err(e) => e.trace(ctx),
        }
    }
}

macro_rules! tuple_trace {
    ($($ty:ident)*) => {
        // SAFETY: tuples have no additional invariants
        unsafe impl<$($ty: Trace),*> Trace for ($($ty),* ,) {
            #[expect(non_snake_case)]
            fn trace(&self, ctx: &TraceContext<'_>) {
                match self {
                    ($($ty),* ,) => {
                        $($ty.trace(ctx);)*
                    }
                }
            }
        }
    };
}

macro_rules! tuples_trace {
    ($(($($x:tt)+))*) => {
        $(tuple_trace!($($x)*);)*
    };
}

tuples_trace! {
    (T0)
    (T0 T1)
    (T0 T1 T2)
    (T0 T1 T2 T3)
    (T0 T1 T2 T3 T4)
    (T0 T1 T2 T3 T4 T5)
    (T0 T1 T2 T3 T4 T5 T6)
    (T0 T1 T2 T3 T4 T5 T6 T7)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15)
    (T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16)
}

macro_rules! fn_trace {
    ($($arg:ident)* => $ret:ident) => {
        // SAFETY: function pointers store nothing
        unsafe impl<$ret: ?Sized, $($arg: ?Sized),*> Trace for fn($($arg),*) -> $ret {
            fn trace(&self, _: &TraceContext<'_>) {}
        }
    };
}

macro_rules! fns_trace {
    ($(($($x:tt)+))*) => {
        $(fn_trace!($($x)*);)*
    };
}

fns_trace! {
    (=> R1)
    (A => R1)
    (A B => R1)
    (A B C => R1)
    (A B C D => R1)
    (A B C D E => R1)
    (A B C D E F => R1)
    (A B C D E F G => R1)
    (A B C D E F G H => R1)
    (A B C D E F G H I => R1)
    (A B C D E F G H I J => R1)
    (A B C D E F G H I J K => R1)
    (A B C D E F G H I J K L => R1)
    (A B C D E F G H I J K L M => R1)
    (A B C D E F G H I J K L M N => R1)
    (A B C D E F G H I J K L M N O => R1)
    (A B C D E F G H I J K L M N O P => R1)
    (A B C D E F G H I J K L M N O P Q => R1)
    (A B C D E F G H I J K L M N O P Q R => R1)
    (A B C D E F G H I J K L M N O P Q R S => R1)
    (A B C D E F G H I J K L M N O P Q R S T => R1)
    (A B C D E F G H I J K L M N O P Q R S T U => R1)
    (A B C D E F G H I J K L M N O P Q R S T U V => R1)
    (A B C D E F G H I J K L M N O P Q R S T U V W => R1)
    (A B C D E F G H I J K L M N O P Q R S T U V W X => R1)
    (A B C D E F G H I J K L M N O P Q R S T U V W X Y => R1)
    (A B C D E F G H I J K L M N O P Q R S T U V W X Y Z => R1)
}
