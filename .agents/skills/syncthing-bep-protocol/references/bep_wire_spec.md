# Syncthing BEP/1.0 & Discovery v4 Wire Specification

## 1. Local Discovery v4 (`21027/udp`, `disco.rs`)
- **Packet Layout**:
  `[Magic: u32 BE = 0x2EA7D90B] [Protobuf Announce payload]`
- **`Announce` Protobuf Fields**:
  - Field `1` (`WireType::LengthDelimited`, `2`): `id` — 32-byte raw SHA-256 of the peer's X.509 DER certificate.
  - Field `2` (`WireType::LengthDelimited`, `2`, repeated): `addresses` — UTF-8 URI strings (e.g., `"tcp://0.0.0.0:22000"`).
  - Field `3` (`WireType::Varint`, `0`): `instance_id` — `u64` random session identifier.

## 2. Device ID Formatting (`disco.rs`)
- 32-byte SHA-256 digest of the peer's ASN.1 DER certificate -> Base32 (`A-Z2-7`, 52 chars without padding).
- Divided into **4 chunks of 13 Base32 characters**.
- Each 13-char chunk has 1 Luhn-mod-32 check character appended -> 14 chars per chunk (56 chars total).
- Formatted into **8 groups of 7 characters** separated by `-`:
  `XXXXXXX-XXXXXXX-XXXXXXX-XXXXXXX-XXXXXXX-XXXXXXX-XXXXXXX-XXXXXXX` (63 chars with hyphens).

## 3. Pre-Authentication `Hello` (`bep.rs`)
Sent immediately after the TLS 1.3 handshake (`ALPN = "bep/1.0"`) before framed messages:
```text
[Magic: u32 BE = 0x2EA7D90B]
[Length: u16 BE]
[Protobuf Hello]
  - Field 1 (String): device_name (e.g. "PlayStation Vita")
  - Field 2 (String): client_name (e.g. "vita-syncthing")
  - Field 3 (String): client_version (e.g. "v0.1.0")
```

## 4. Post-Authentication BEP Frame (`bep.rs`)
```text
[Header Length: u16 BE]
[Protobuf Header]
  - Field 1 (Varint): BepMessageType
      0 = ClusterConfig
      1 = Index
      2 = IndexUpdate
      3 = Request
      4 = Response
      5 = DownloadProgress
      6 = Ping
      7 = Close
  - Field 2 (Varint): MessageCompression (0 = None, 1 = LZ4)
[Body Length: u32 BE]
[Body Payload]
  - If compression == LZ4 (1):
    [Uncompressed Length: u32 BE][Raw LZ4 Block]
```

## 5. Core BEP Protobuf Messages (`bep.rs`)
- **`ClusterConfig` (`type = 0`)**:
  - Field `1` (repeated `Folder`):
    - `1` (String): `id`
    - `2` (String): `label`
    - `16` (repeated `Device`):
      - `1` (Bytes): `id` (32-byte `DeviceId`)
      - `2` (String): `name`
      - `6` (Varint): `index_id` (`u64`)
      - `7` (Varint): `max_sequence` (`i64`)
- **`Index` / `IndexUpdate` (`type = 1 | 2`)**:
  - Field `1` (String): `folder`
  - Field `2` (repeated `FileInfo`):
    - `1` (String): `name` (relative path with `/`)
    - `2` (Varint): `type` (`0 = File`, `1 = Directory`)
    - `3` (Varint): `size` (`i64`)
    - `4` (Varint): `permissions` (`u32`, e.g. `0o644`)
    - `5` (Varint): `modified_s` (`i64`)
    - `6` (Varint): `modified_ns` (`i32`)
    - `7` (Varint): `modified_by` (`u64` short device ID)
    - `8` (Varint): `deleted` (`bool`)
    - `9` (Varint): `invalid` (`bool`)
    - `13` (Message `Vector`): repeated Field `1` (`Counter { id: 1, value: 2 }`)
    - `15` (Varint): `sequence` (`i64`)
    - `16` (Varint): `block_size` (`i32`, `131072`)
    - `17` (repeated `BlockInfo`):
      - `1` (Varint): `offset` (`i64`)
      - `2` (Varint): `size` (`i32`)
      - `3` (Bytes): `hash` (32-byte SHA-256)
- **`Request` (`type = 3`)**:
  - `1` (Varint): `id` (`i32`)
  - `2` (String): `folder`
  - `3` (String): `name`
  - `4` (Varint): `offset` (`i64`)
  - `5` (Varint): `size` (`i32`)
  - `6` (Bytes): `hash` (32-byte SHA-256)
- **`Response` (`type = 4`)**:
  - `1` (Varint): `id` (`i32`)
  - `2` (Bytes): `data`
  - `3` (Varint): `code` (`0 = Ok`, `1 = Generic`, `2 = NoSuchFile`, `3 = InvalidFile`)
