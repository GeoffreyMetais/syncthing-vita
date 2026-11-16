//! Local Discovery `Announce` packet edge cases (`discovery::announce`): a packet shorter than
//! the magic is "too short", a packet with the magic and no fields lacks its device ID.

use syncthing_core::{Announce, DISCOVERY_MAGIC};

#[test]
fn a_packet_shorter_than_the_magic_is_too_short() {
    for len in 0..4 {
        let err = Announce::decode_packet(&DISCOVERY_MAGIC.to_be_bytes()[..len])
            .expect_err("short packet");
        assert!(err.to_string().contains("too short"), "{len} bytes: {err}");
    }
}

#[test]
fn a_bare_magic_packet_is_missing_its_device_id() {
    // Exactly the magic: long enough to be decoded, but without the `id` field.
    let err = Announce::decode_packet(&DISCOVERY_MAGIC.to_be_bytes()).expect_err("no id");
    assert!(!err.to_string().contains("too short"), "{err}");
    assert!(err.to_string().contains("Missing"), "{err}");
}
