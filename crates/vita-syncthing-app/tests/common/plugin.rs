//! Background sync plugin fixtures: a `PluginHost` rooted in a test directory, with the
//! bundled modules and a taiHEN `config.txt` in place.

use vita_syncthing_app::infrastructure::{FixedLauncherProbe, PluginHost};

/// Contents of the bundled launcher (`app0:plugins/syncthing_launcher.suprx`).
pub const LAUNCHER_BYTES: &[u8] = b"\x7fELF launcher";
/// Contents of the bundled worker (`app0:plugins/syncthing_worker.suprx`).
pub const WORKER_BYTES: &[u8] = b"\x7fELF worker";

/// Stock `ur0:tai/config.txt` of HENkaku Ensō.
pub const STOCK_CONFIG: &str = "*KERNEL\nur0:tai/nonpdrm.skprx\n*main\nur0:tai/henkaku.suprx\n";

/// Writes `bytes` at the device path `path` below `root` (`ux0:tai/x` -> `<root>/ux0/tai/x`).
pub fn put(root: &str, path: &str, bytes: &[u8]) {
    let local = local(root, path);
    std::fs::create_dir_all(std::path::Path::new(&local).parent().unwrap()).unwrap();
    std::fs::write(local, bytes).unwrap();
}

/// Filesystem path of the device path `path` below `root`.
pub fn local(root: &str, path: &str) -> String {
    let (device, rest) = path.split_once(':').unwrap();
    format!("{root}/{device}/{rest}")
}

/// Text of the device file `path` below `root` (`None` if missing).
pub fn text(root: &str, path: &str) -> Option<String> {
    std::fs::read_to_string(local(root, path)).ok()
}

pub fn exists(root: &str, path: &str) -> bool {
    std::path::Path::new(&local(root, path)).exists()
}

/// Host rooted in `<dir>/root`, `plugin.cfg` in `<dir>/data`, launcher loaded if `alive`.
pub fn host(dir: &str, alive: bool) -> PluginHost {
    PluginHost::rooted(
        &format!("{dir}/root"),
        &format!("{dir}/data"),
        Box::new(FixedLauncherProbe(alive)),
    )
}

/// Root of `host(dir, _)`.
pub fn root(dir: &str) -> String {
    format!("{dir}/root")
}

/// Puts both modules in `app0:plugins/` and `config` in `ur0:tai/config.txt`.
pub fn seed(dir: &str, config: &str) {
    let root = root(dir);
    put(
        &root,
        "app0:plugins/syncthing_launcher.suprx",
        LAUNCHER_BYTES,
    );
    put(&root, "app0:plugins/syncthing_worker.suprx", WORKER_BYTES);
    put(&root, "ur0:tai/config.txt", config.as_bytes());
}
