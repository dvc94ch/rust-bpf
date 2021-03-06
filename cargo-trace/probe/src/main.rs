#![no_std]
#![no_main]

use bpf_helpers::{entry, map, program, sys, Array, HashMap, PidTgid};

program!(0xFFFF_FFFE, b"GPL");

// absolute maximum would be 512 byte stack size limit / 8 byte address = 64. but since
// we need some stack for other variables this needs to be lower.
const MAX_STACK_DEPTH: usize = 48;
const MAX_BIN_SEARCH_DEPTH: usize = 24;
const EHFRAME_ENTRIES: usize = 0xff_ffff;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Instruction {
    op: u64,
    offset: i64,
}

#[map]
static CONFIG: Array<u32> = Array::with_max_entries(2);
#[map]
static PC: Array<u64> = Array::with_max_entries(EHFRAME_ENTRIES);
#[map]
static RIP: Array<Instruction> = Array::with_max_entries(EHFRAME_ENTRIES);
#[map]
static RSP: Array<Instruction> = Array::with_max_entries(EHFRAME_ENTRIES);

#[map]
static USER_STACK: HashMap<[u64; MAX_STACK_DEPTH], u32> = HashMap::with_max_entries(1024);

#[entry("perf_event")]
fn perf_event(args: &bpf_perf_event_data) {
    increment_stack_counter(&args.regs);
}

#[entry("kprobe")]
fn kprobe(args: &pt_regs) {
    increment_stack_counter(args);
}

fn increment_stack_counter(regs: &sys::pt_regs) {
    if let Some(pid) = CONFIG.get(1) {
        if PidTgid::current().pid() == pid {
            let mut stack = [0; MAX_STACK_DEPTH];
            backtrace(regs, &mut stack);
            let mut count = USER_STACK.get(&stack).unwrap_or_default();
            count += 1;
            USER_STACK.insert(&stack, &count);
        }
    }
}

fn backtrace(regs: &sys::pt_regs, stack: &mut [u64; MAX_STACK_DEPTH]) {
    let mut rip = regs.rip;
    let mut rsp = regs.rsp;
    for d in 0..MAX_STACK_DEPTH {
        stack[d] = rip;
        if rip == 0 {
            break;
        }
        let i = binary_search(rip);

        let ins = if let Some(ins) = RSP.get(i) {
            ins
        } else {
            break;
        };
        let cfa = if let Some(cfa) = execute_instruction(&ins, rip, rsp, 0) {
            cfa
        } else {
            break;
        };

        let ins = if let Some(ins) = RIP.get(i) {
            ins
        } else {
            break;
        };
        rip = execute_instruction(&ins, rip, rsp, cfa).unwrap_or_default();
        rsp = cfa;
    }
}

fn binary_search(rip: u64) -> u32 {
    let mut left = 0;
    let mut right = CONFIG.get(0).unwrap_or(1) - 1;
    let mut i = 0;
    for _ in 0..MAX_BIN_SEARCH_DEPTH {
        if left > right {
            break;
        }
        i = (left + right) / 2;
        let pc = PC.get(i).unwrap_or(u64::MAX);
        if pc < rip {
            left = i;
        } else {
            right = i;
        }
    }
    i
}

fn execute_instruction(ins: &Instruction, rip: u64, rsp: u64, cfa: u64) -> Option<u64> {
    match ins.op {
        1 => {
            let unsafe_ptr = (cfa as i64 + ins.offset as i64) as *const core::ffi::c_void;
            let mut res: u64 = 0;
            if unsafe { sys::bpf_probe_read(&mut res as *mut _ as *mut _, 8, unsafe_ptr) } == 0 {
                Some(res)
            } else {
                None
            }
        }
        2 => Some((rip as i64 + ins.offset as i64) as u64),
        3 => Some((rsp as i64 + ins.offset as i64) as u64),
        _ => None,
    }
}
