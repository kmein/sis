//! zbus proxies for the interfaces of `org.freedesktop.systemd1`.
//!
//! Property-returning methods are deliberately kept to what the header and
//! table need; the detail view reads everything through `Properties.GetAll`.

use std::collections::HashMap;

use zbus::{
    proxy,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use super::types::{EnableChange, ListedJob, ListedUnit, Process};

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Manager {
    fn list_units(&self) -> zbus::Result<Vec<ListedUnit>>;

    fn list_unit_files(&self) -> zbus::Result<Vec<(String, String)>>;

    fn list_jobs(&self) -> zbus::Result<Vec<ListedJob>>;

    fn load_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;

    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;

    fn get_unit_file_state(&self, name: &str) -> zbus::Result<String>;

    fn get_unit_processes(&self, name: &str) -> zbus::Result<Vec<Process>>;

    fn subscribe(&self) -> zbus::Result<()>;

    #[zbus(allow_interactive_auth)]
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    #[zbus(allow_interactive_auth)]
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    #[zbus(allow_interactive_auth)]
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    #[zbus(allow_interactive_auth)]
    fn reload_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    #[zbus(allow_interactive_auth)]
    fn reload_or_restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    #[zbus(allow_interactive_auth)]
    fn enable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
        force: bool,
    ) -> zbus::Result<(bool, Vec<EnableChange>)>;

    #[zbus(allow_interactive_auth)]
    fn disable_unit_files(&self, files: &[&str], runtime: bool) -> zbus::Result<Vec<EnableChange>>;

    #[zbus(allow_interactive_auth)]
    fn mask_unit_files(&self, files: &[&str], runtime: bool, force: bool) -> zbus::Result<Vec<EnableChange>>;

    #[zbus(allow_interactive_auth)]
    fn unmask_unit_files(&self, files: &[&str], runtime: bool) -> zbus::Result<Vec<EnableChange>>;

    #[zbus(allow_interactive_auth)]
    fn reset_failed_unit(&self, name: &str) -> zbus::Result<()>;

    #[zbus(allow_interactive_auth)]
    fn kill_unit(&self, name: &str, whom: &str, signal: i32) -> zbus::Result<()>;

    #[zbus(allow_interactive_auth)]
    fn cancel_job(&self, id: u32) -> zbus::Result<()>;

    #[zbus(allow_interactive_auth)]
    fn reload(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn system_state(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn n_names(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn n_failed_units(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn n_jobs(&self) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn unit_new(&self, id: String, unit: OwnedObjectPath) -> zbus::Result<()>;

    #[zbus(signal)]
    fn unit_removed(&self, id: String, unit: OwnedObjectPath) -> zbus::Result<()>;

    #[zbus(signal)]
    fn job_new(&self, id: u32, job: OwnedObjectPath, unit: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn job_removed(&self, id: u32, job: OwnedObjectPath, unit: String, result: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn reloading(&self, active: bool) -> zbus::Result<()>;
}

/// Properties shared by every unit type. Built per call with property caching
/// disabled; see [`super::Backend::unit`].
#[proxy(interface = "org.freedesktop.systemd1.Unit", default_service = "org.freedesktop.systemd1")]
pub trait Unit {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn active_enter_timestamp(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn state_change_timestamp(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn unit_file_state(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn fragment_path(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn drop_in_paths(&self) -> zbus::Result<Vec<String>>;

    #[zbus(property)]
    fn need_daemon_reload(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn can_reload(&self) -> zbus::Result<bool>;
}

#[proxy(interface = "org.freedesktop.systemd1.Service", default_service = "org.freedesktop.systemd1")]
pub trait Service {
    #[zbus(property)]
    fn main_pid(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn n_restarts(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn memory_current(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn tasks_current(&self) -> zbus::Result<u64>;

    #[zbus(property, name = "CPUUsageNSec")]
    fn cpu_usage_nsec(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn control_group(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn status_text(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn result(&self) -> zbus::Result<String>;
}

#[proxy(interface = "org.freedesktop.systemd1.Timer", default_service = "org.freedesktop.systemd1")]
pub trait Timer {
    #[zbus(property, name = "NextElapseUSecRealtime")]
    fn next_elapse_usec_realtime(&self) -> zbus::Result<u64>;

    #[zbus(property, name = "LastTriggerUSec")]
    fn last_trigger_usec(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn unit(&self) -> zbus::Result<String>;
}

#[proxy(interface = "org.freedesktop.systemd1.Socket", default_service = "org.freedesktop.systemd1")]
pub trait Socket {
    #[zbus(property)]
    fn listen(&self) -> zbus::Result<Vec<(String, String)>>;

    #[zbus(property, name = "NConnections")]
    fn n_connections(&self) -> zbus::Result<u32>;

    #[zbus(property, name = "NAccepted")]
    fn n_accepted(&self) -> zbus::Result<u32>;
}

/// Convenience alias for a `Properties.GetAll` result.
pub type PropertyMap = HashMap<String, OwnedValue>;
