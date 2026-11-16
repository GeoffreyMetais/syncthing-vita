//! The per-process instance ID announced by the sync worker: stable for the whole process and
//! never the zero placeholder, so peers can tell this run apart from a previous one.

use vita_syncthing_app::infrastructure::sync_worker::process_instance_id;

#[test]
fn test_instance_id_is_stable_within_the_process() {
    let first = process_instance_id();
    assert_eq!(process_instance_id(), first);
    assert_eq!(process_instance_id(), first);
}

#[test]
fn test_instance_id_is_not_the_zero_placeholder() {
    assert_ne!(process_instance_id(), 0);
}
