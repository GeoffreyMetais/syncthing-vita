//! `run` options: the configuration overridden by the command-line flags.

use syncthing_cli::cli::RunArgs;
use syncthing_cli::commands::run::daemon_options;
use syncthing_cli::config::Config;

fn args() -> RunArgs {
    RunArgs {
        listen: None,
        no_local_discovery: false,
        no_global_discovery: false,
        no_relays: false,
        exit_after: None,
        exit_when_synced: false,
    }
}

#[test]
fn configuration_applies_when_no_flag_is_given() {
    let mut config = Config::default();
    config.listen = "0.0.0.0:22005".into();
    let options = daemon_options(&config, &args()).expect("options");
    assert_eq!(options.listen, Some("0.0.0.0:22005".parse().unwrap()));
    assert!(options.local_discovery);
    assert!(options.global_discovery);
    assert!(options.relays);
}

#[test]
fn each_disable_flag_turns_off_only_its_feature() {
    let config = Config::default();
    let mut a = args();
    a.no_local_discovery = true;
    let options = daemon_options(&config, &a).expect("options");
    assert!(!options.local_discovery);
    assert!(options.global_discovery && options.relays);

    let mut a = args();
    a.no_global_discovery = true;
    let options = daemon_options(&config, &a).expect("options");
    assert!(options.local_discovery && !options.global_discovery && options.relays);

    let mut a = args();
    a.no_relays = true;
    let options = daemon_options(&config, &a).expect("options");
    assert!(options.local_discovery && options.global_discovery && !options.relays);
}

#[test]
fn a_feature_disabled_in_the_configuration_stays_off_without_flags() {
    let mut config = Config::default();
    config.local_discovery = false;
    config.global_discovery = false;
    config.relays = false;
    let options = daemon_options(&config, &args()).expect("options");
    assert!(!options.local_discovery && !options.global_discovery && !options.relays);
}

#[test]
fn the_listen_flag_overrides_the_configuration() {
    let mut config = Config::default();
    config.listen = "0.0.0.0:22005".into();
    let mut a = args();
    a.listen = Some("127.0.0.1:22006".into());
    let options = daemon_options(&config, &a).expect("options");
    assert_eq!(options.listen, Some("127.0.0.1:22006".parse().unwrap()));
}

#[test]
fn an_empty_or_none_listen_address_means_no_listener() {
    let config = Config::default();
    for value in ["", "none", "NONE"] {
        let mut a = args();
        a.listen = Some(value.into());
        let options = daemon_options(&config, &a).expect("options");
        assert_eq!(options.listen, None, "listen {value:?}");
    }
}

#[test]
fn an_invalid_listen_address_is_rejected() {
    let mut a = args();
    a.listen = Some("not-an-address".into());
    let err = daemon_options(&Config::default(), &a).expect_err("invalid listen address");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}
