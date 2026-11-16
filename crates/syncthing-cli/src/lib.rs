//! `syncthing-cli`: a command-line Syncthing client for Linux and macOS built on
//! `syncthing-core` (LAN discovery, direct TCP, Global Discovery and relays).
//!
//! This file only declares modules.

pub mod cli;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod https_env;
pub mod identity;
pub mod paths;
