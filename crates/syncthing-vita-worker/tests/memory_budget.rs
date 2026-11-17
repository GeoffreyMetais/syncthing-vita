//! Heap used by one cycle, measured with a counting allocator on the cycle's thread only (the
//! test peer runs in the same process). The PS Vita worker has a fixed newlib heap of
//! `WORKER_HEAP_BYTES` (8 MiB) in `SceShell`; this cycle (a few hundred files, a multi-block
//! file, one TLS session) must stay well within it. The host is 64-bit: pointers and
//! collections are twice their PS Vita size, so the measure is an upper bound.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use common::{data_dir_with_identity, device, fresh_temp_dir, repo, share, test_options};
use syncthing_vita_worker::defaults::CYCLE_THREAD_STACK;
use syncthing_vita_worker::{never_abort, run_cycle, CycleOutcome};

/// Half of the PS Vita worker heap.
const BUDGET_BYTES: usize = 4 * 1024 * 1024;

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static COUNTED: Cell<bool> = const { Cell::new(false) };
}

fn counted() -> bool {
    COUNTED.try_with(Cell::get).unwrap_or(false)
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() && counted() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(now, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if counted() {
            let _ = CURRENT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| {
                Some(c.saturating_sub(layout.size()))
            });
        }
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn test_cycle_heap_stays_within_the_worker_budget() {
    let data = data_dir_with_identity("memory_budget", true);
    let peer_root = fresh_temp_dir("memory_budget_peer");
    let remote = peer_root.join("saves");
    std::fs::create_dir_all(&remote).unwrap();
    for i in 0..300 {
        let dir = remote.join(format!("PCSE{:05}", i / 10));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("slot{i}.bin")), vec![i as u8; 2048]).unwrap();
    }
    // 2 MiB: 16 blocks of 128 KiB.
    std::fs::write(remote.join("big.bin"), vec![3u8; 2 * 1024 * 1024]).unwrap();
    let peer = common::ListeningPeer::spawn(0x42, &peer_root, &["saves"]);
    let saves = data.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    for i in 0..100 {
        std::fs::write(saves.join(format!("local{i}.bin")), vec![1u8; 1024]).unwrap();
    }
    let app = repo(&data);
    app.save_devices(&[device(peer.device_id, "Desktop", &peer.addr.to_string())]);
    app.save_shares(&[share("saves", &saves, true, vec![peer.device_id])]);

    let report = thread::Builder::new()
        .stack_size(CYCLE_THREAD_STACK)
        .spawn(move || {
            COUNTED.with(|c| c.set(true));
            run_cycle(&data, never_abort(), &test_options())
        })
        .unwrap()
        .join()
        .unwrap();
    peer.join();

    assert_eq!(report.outcome, CycleOutcome::Completed);
    assert_eq!(
        std::fs::read(saves.join("big.bin")).unwrap().len(),
        2 * 1024 * 1024
    );
    let peak = PEAK.load(Ordering::SeqCst);
    println!("cycle heap peak: {} KiB", peak / 1024);
    assert!(peak > 0);
    assert!(
        peak < BUDGET_BYTES,
        "the cycle used {} KiB of heap (budget {} KiB)",
        peak / 1024,
        BUDGET_BYTES / 1024
    );
}
