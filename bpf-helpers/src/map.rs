// Copyright 2019 Authors of Red Sift
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

/*!
eBPF maps.

Maps are a generic data structure for storage of different types of data.
They allow sharing of data between eBPF kernel programs, and also between
kernel and user-space code.
 */
use core::convert::TryInto;
use core::ffi::c_void;
use core::marker::PhantomData;
use core::mem;
use cty::c_int;

/// Hash table map.
///
/// High level API for BPF_MAP_TYPE_HASH maps.
#[repr(transparent)]
pub struct HashMap<K, V> {
    def: bpf_helpers_sys::bpf_map_def,
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<K, V> HashMap<K, V> {
    /// Creates a map with the specified maximum number of elements.
    pub const fn with_max_entries(max_entries: u32) -> Self {
        Self {
            def: bpf_helpers_sys::bpf_map_def {
                type_: bpf_helpers_sys::bpf_map_type_BPF_MAP_TYPE_HASH,
                key_size: mem::size_of::<K>() as u32,
                value_size: mem::size_of::<V>() as u32,
                max_entries,
                map_flags: 0,
            },
            _k: PhantomData,
            _v: PhantomData,
        }
    }
}

impl<K, V: Clone> HashMap<K, V> {
    /// Returns a reference to the value corresponding to the key.
    #[inline]
    pub fn get(&self, key: &K) -> Option<V> {
        let ptr = unsafe {
            bpf_helpers_sys::bpf_map_lookup_elem(
                &self.def as *const _ as *mut c_void,
                key as *const _ as *const c_void,
            )
        } as *const V;
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { (&*ptr).clone() })
        }
    }

    /// Set the `value` in the map for `key`
    #[inline]
    pub fn insert(&self, key: &K, value: &V) {
        unsafe {
            bpf_helpers_sys::bpf_map_update_elem(
                &self.def as *const _ as *mut c_void,
                key as *const _ as *const c_void,
                value as *const _ as *const c_void,
                bpf_helpers_sys::BPF_ANY.into(),
            );
        }
    }

    /// Delete the entry indexed by `key`
    #[inline]
    pub fn remove(&self, key: &K) {
        unsafe {
            bpf_helpers_sys::bpf_map_delete_elem(
                &self.def as *const _ as *mut c_void,
                key as *const _ as *const c_void,
            );
        }
    }
}

/// Flags that can be passed to `PerfMap::insert_with_flags`.
#[derive(Debug, Copy, Clone)]
pub struct PerfMapFlags {
    index: Option<u32>,
    pub(crate) xdp_size: u32,
}

impl Default for PerfMapFlags {
    #[inline]
    fn default() -> Self {
        PerfMapFlags {
            index: None,
            xdp_size: 0,
        }
    }
}

impl PerfMapFlags {
    /// Create new default flags.
    ///
    /// Events inserted with default flags are keyed by the current CPU number
    /// and don't include any extra payload data.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Create flags for events carrying `size` extra bytes of `XDP` payload data.
    #[inline]
    pub fn with_xdp_size(size: u32) -> Self {
        *PerfMapFlags::new().xdp_size(size)
    }

    /// Set the index key for the event to insert.
    #[inline]
    pub fn index(&mut self, index: u32) -> &mut PerfMapFlags {
        self.index = Some(index);
        self
    }

    /// Set the number of bytes of the `XDP` payload data to append to the event.
    #[inline]
    pub fn xdp_size(&mut self, size: u32) -> &mut PerfMapFlags {
        self.xdp_size = size;
        self
    }
}

impl From<PerfMapFlags> for u64 {
    #[inline]
    fn from(flags: PerfMapFlags) -> u64 {
        (flags.xdp_size as u64) << 32
            | (flags
                .index
                .unwrap_or_else(|| bpf_helpers_sys::BPF_F_CURRENT_CPU.try_into().unwrap())
                as u64)
    }
}

/// Perf events map.
///
/// Perf events map that allows eBPF programs to store data in mmap()ed shared
/// memory accessible by user-space. This is a wrapper for
/// `BPF_MAP_TYPE_PERF_EVENT_ARRAY`.
///
/// If you're writing an `XDP` probe, you should use `xdp::PerfMap` instead which
/// exposes `XDP`-specific functionality.
#[repr(transparent)]
pub struct PerfMap<T> {
    def: bpf_helpers_sys::bpf_map_def,
    _event: PhantomData<T>,
}

impl<T> PerfMap<T> {
    /// Creates a perf map with the specified maximum number of elements.
    pub const fn with_max_entries(max_entries: u32) -> Self {
        Self {
            def: bpf_helpers_sys::bpf_map_def {
                type_: bpf_helpers_sys::bpf_map_type_BPF_MAP_TYPE_PERF_EVENT_ARRAY,
                key_size: mem::size_of::<u32>() as u32,
                value_size: mem::size_of::<u32>() as u32,
                max_entries,
                map_flags: 0,
            },
            _event: PhantomData,
        }
    }

    /// Insert a new event in the perf events array keyed by the current CPU number.
    ///
    /// Each array can hold up to `max_entries` events, see `with_max_entries`.
    /// If you want to use a key other than the current CPU, see
    /// `insert_with_flags`.
    #[inline]
    pub fn insert<C>(&mut self, ctx: *mut C, data: &T) {
        self.insert_with_flags(ctx, data, PerfMapFlags::default())
    }

    /// Insert a new event in the perf events array keyed by the index and with
    /// the additional xdp payload data specified in the given `PerfMapFlags`.
    #[inline]
    pub fn insert_with_flags<C>(&mut self, ctx: *mut C, data: &T, flags: PerfMapFlags) {
        unsafe {
            bpf_helpers_sys::bpf_perf_event_output(
                ctx as *mut _ as *mut c_void,
                &mut self.def as *mut _ as *mut c_void,
                flags.into(),
                data as *const _ as *mut c_void,
                mem::size_of::<T>() as u64,
            );
        }
    }
}

// TODO Use PERF_MAX_STACK_DEPTH
const BPF_MAX_STACK_DEPTH: usize = 127;

#[repr(transparent)]
pub struct StackTrace {
    def: bpf_helpers_sys::bpf_map_def,
}

#[repr(C)]
struct BpfStackFrames {
    ip: [u64; BPF_MAX_STACK_DEPTH],
}

impl StackTrace {
    pub const SKIP_FIELD_MASK: u64 = bpf_helpers_sys::BPF_F_SKIP_FIELD_MASK as _;
    pub const USER_STACK: u64 = bpf_helpers_sys::BPF_F_USER_STACK as _;
    pub const KERNEL_STACK: u64 = 0;
    pub const FAST_STACK_CMP: u64 = bpf_helpers_sys::BPF_F_FAST_STACK_CMP as _;
    pub const REUSE_STACKID: u64 = bpf_helpers_sys::BPF_F_REUSE_STACKID as _;

    pub const fn with_max_entries(cap: u32) -> Self {
        StackTrace {
            def: bpf_helpers_sys::bpf_map_def {
                type_: bpf_helpers_sys::bpf_map_type_BPF_MAP_TYPE_STACK_TRACE,
                key_size: mem::size_of::<u32>() as u32,
                value_size: mem::size_of::<BpfStackFrames>() as u32,
                max_entries: cap,
                map_flags: 0,
            },
        }
    }

    pub fn stack_id(&self, ctx: *const c_void, flag: u64) -> Result<u32, c_int> {
        let ret = unsafe {
            bpf_helpers_sys::bpf_get_stackid(
                ctx as *mut _,
                &self.def as *const _ as *mut c_void,
                flag,
            )
        };
        if ret >= 0 {
            Ok(ret as _)
        } else {
            Err(ret)
        }
    }
}

/// Program array map.
///
/// An array of eBPF programs that can be used as a jump table.
///
/// To configure the map use
/// [`redbpf::ProgramArray`](../../redbpf/struct.ProgramArray.html)
/// from user-space.
///
/// To jump to a program, see the `tail_call` method.
#[repr(transparent)]
pub struct ProgramArray {
    def: bpf_helpers_sys::bpf_map_def,
}

impl ProgramArray {
    /// Creates a program map with the specified maximum number of programs.
    pub const fn with_max_entries(max_entries: u32) -> Self {
        Self {
            def: bpf_helpers_sys::bpf_map_def {
                type_: bpf_helpers_sys::bpf_map_type_BPF_MAP_TYPE_PROG_ARRAY,
                key_size: mem::size_of::<u32>() as u32,
                value_size: mem::size_of::<u32>() as u32,
                max_entries,
                map_flags: 0,
            },
        }
    }

    /// Jump to the eBPF program referenced at `index`, passing `ctx` as context.
    ///
    /// This special method is used to trigger a "tail call", or in other words,
    /// to jump into another eBPF program.  The same stack frame is used (but
    /// values on stack and in registers for the caller are not accessible to
    /// the callee). This mechanism allows for program chaining, either for
    /// raising the maximum number of available eBPF instructions, or to execute
    /// given programs in conditional blocks. For security reasons, there is an
    /// upper limit to the number of successive tail calls that can be
    /// performed.
    ///
    /// If the call succeeds the kernel immediately runs the first instruction
    /// of the new program. This is not a function call, and it never returns to
    /// the previous program. If the call fails, then the helper has no effect,
    /// and the caller continues to run its subsequent instructions.
    ///
    /// A call can fail if the destination program for the jump does not exist
    /// (i.e. index is superior to the number of entries in the array), or
    /// if the maximum number of tail calls has been reached for this chain of
    /// programs.
    pub unsafe fn tail_call<C>(&mut self, ctx: *mut C, index: u32) -> Result<(), i32> {
        let ret = bpf_helpers_sys::bpf_tail_call(
            ctx as *mut _,
            &mut self.def as *mut _ as *mut c_void,
            index,
        );
        if ret < 0 {
            return Err(ret);
        }

        Ok(())
    }
}
