//! `id`: prints this device's ID.

use std::io;
use std::path::Path;

use crate::identity::load_or_create_identity;

pub fn run(home: &Path) -> io::Result<()> {
    let identity = load_or_create_identity(home)?;
    println!("{}", identity.device_id.to_syncthing_string());
    Ok(())
}
