use core::{ptr::NonNull, alloc::Layout};

use crate::trace::{TraceContext, Trace};



/// Functionality implemented by individual GC objects. This includes the finalizer and tracing methods.
pub struct GcVtable {
    /// The size and alignment of the GC allocation.
    layout: Layout,
    /// Marking functionality for a GC type.
    /// # Safety
    /// This function must be called on a value of compatible that is valid for shared access.
    trace: unsafe fn(NonNull<()>, &TraceContext<'_>),
}

impl GcVtable {
    pub const fn for_type<T: Trace>() -> &'static Self {
        const {
            &Self {
                layout: Layout::new::<T>(),
                trace: |ptr, ctx| /* Safety: caller */ unsafe { ptr.cast::<T>().as_ref().trace(ctx) },
            }
        }
    }
}

pub type Handle = usize;

/// A GC strategy. This trait defines how to allocate and manage a GC heap and the objects contained within it.
///
/// # The GC Object Lifecycle
/// Allocations into the GC heap are known as GC objects. The lifecycle of a GC object is characterized by several
/// states.
/// - Uninitialized: this is the state of a freshly allocated GC object, before a value has been stored into it.
/// - Initialized: this is the state of a GC object that holds a value that is reachable or has not yet been finalized.
/// - Finalized: this is the state of a GC object that has been determined unreachable and has been finalized.
///
/// GC objects may also have one of the following properties.
/// - Rooted: the object is directly reachable through a value outside of the GC heap, such as the stack or foreign heaps.
/// - Pinned: the object has a stable address within the GC heap - that is, a compacting GC will not move the allocation.
///
/// A typical GC allocation starts out uninitialized, then becomes initialized once a value
/// is assigned to it. After some time, the object becomes unreachable and is registered for finalization. After the object
/// is finalized, its memory can be reclaimed.
///
/// In general, the GC heap should not directly access the values it holds, except to invoke methods of the [`Trace`] trait.
/// This includes the value's drop glue. Dropping should be handled using the object's associated finalization queue, if any.
///
/// # Heap Ownership
/// The GC heap uniquely owns and manages all GC objects. When the heap is destroyed, all GC allocations controlled by the
/// heap must also be destroyed. It is not permissible to share GC allocations among several heaps or store GC allocations
/// in borrowed storage. At the very least, implementations must not invoke [`Trace::trace`] on GC objects allocated
/// within the heap after the heap has been destroyed.
///
/// # Safety
/// Implementations of this trait must uphold the contracts of all defined methods, as well as the trait documentation.
pub unsafe trait GcStrategy {
    /// Allocate memory on the GC heap for a GC node with the given vtable. The returned GC allocation is rooted and
    /// pinned, and in an Uninitialized state in preparation for a value to be written.
    ///
    /// Returns None if there is not enough heap space for the node, otherwise returns a handle representing the node.
    fn allocate(&self, vtable: &'static GcVtable) -> Option<FreshAllocation>;

    /// Marks the given GC allocation initialized. This unpins the allocation (but keeps it rooted) and sets its state
    /// to Initialized.
    ///
    /// # Safety
    /// The GC allocation must be in the Uninitialized state (that is, having just been returned from [`GcStrategy::allocate`]).
    unsafe fn set_initialized(&self, obj: Handle);

    /// Marks the given GC allocation as finalized. This notifies the GC that the allocation may be reclaimed.
    ///
    /// # Safety
    /// The GC allocation must have been previously determined to be finalizable (e.g. by being passed to a finalization queue).
    unsafe fn set_finalized(&self, obj: Handle);

    fn pin(&self, obj: Handle) -> *const ();

    fn unpin(&self, obj: Handle);

    /// Adds a root that references the given GC handle.
    fn root(&self, obj: Handle);

    /// Removes a root referencing the given GC handle.
    fn unroot(&self, obj: Handle);
}

pub struct FreshAllocation {
    /// A handle to the GC allocation.
    pub handle: Handle,
    /// The address where the value will be stored.
    pub ptr: *mut (),
}