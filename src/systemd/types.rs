//! Wire types for the systemd D-Bus API and the enums derived from them.

use std::fmt;

use serde::Deserialize;
use zbus::zvariant::{OwnedObjectPath, Type};

/// One row of `Manager.ListUnits`: `(ssssssouso)`.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct ListedUnit(
    /// Unit name.
    pub String,
    /// Description.
    pub String,
    /// Load state.
    pub String,
    /// Active state.
    pub String,
    /// Sub state.
    pub String,
    /// Unit this one is following, or empty.
    pub String,
    /// Object path of the unit.
    pub OwnedObjectPath,
    /// Queued job id, 0 if none.
    pub u32,
    /// Queued job type, empty if none.
    pub String,
    /// Queued job object path, `/` if none.
    pub OwnedObjectPath,
);

/// One row of `Manager.ListJobs`: `(usssoo)`.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct ListedJob(
    /// Job id.
    pub u32,
    /// Unit name.
    pub String,
    /// Job type (start, stop, restart, ...).
    pub String,
    /// Job state (waiting, running).
    pub String,
    /// Job object path.
    pub OwnedObjectPath,
    /// Unit object path.
    pub OwnedObjectPath,
);

/// One row of `Manager.GetUnitProcesses`: `(sus)`.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct Process(
    /// Control group path.
    pub String,
    /// PID.
    pub u32,
    /// Command line.
    pub String,
);

/// One row of the change lists returned by `*UnitFiles`: `(sss)`.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct EnableChange(
    /// Change type (symlink, unlink).
    pub String,
    /// File name of the symlink.
    pub String,
    /// Destination of the symlink.
    pub String,
);

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $s:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($variant,)*
            Other(String),
        }

        impl $name {
            pub fn parse(s: &str) -> Self {
                match s {
                    $($s => Self::$variant,)*
                    other => Self::Other(other.to_owned()),
                }
            }

            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $s,)*
                    Self::Other(s) => s,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// The `ActiveState` of a unit.
    ActiveState {
        Active = "active",
        Reloading = "reloading",
        Inactive = "inactive",
        Failed = "failed",
        Activating = "activating",
        Deactivating = "deactivating",
        Maintenance = "maintenance",
        Refreshing = "refreshing",
    }
}

string_enum! {
    /// The `LoadState` of a unit.
    LoadState {
        Loaded = "loaded",
        NotFound = "not-found",
        BadSetting = "bad-setting",
        Error = "error",
        Masked = "masked",
        Stub = "stub",
        Merged = "merged",
    }
}

string_enum! {
    /// The unit type, derived from the name suffix.
    UnitKind {
        Service = "service",
        Socket = "socket",
        Target = "target",
        Device = "device",
        Mount = "mount",
        Automount = "automount",
        Swap = "swap",
        Timer = "timer",
        Path = "path",
        Slice = "slice",
        Scope = "scope",
    }
}

impl UnitKind {
    /// Derive the kind from a unit name such as `nginx.service`.
    pub fn of(unit_name: &str) -> Self {
        match unit_name.rsplit_once('.') {
            Some((_, suffix)) => Self::parse(suffix),
            None => Self::Other(String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_kind_from_name() {
        assert_eq!(UnitKind::of("nginx.service"), UnitKind::Service);
        assert_eq!(UnitKind::of("dev-disk-by\\x2duuid-foo.device"), UnitKind::Device);
        assert_eq!(UnitKind::of("weird.thing"), UnitKind::Other("thing".into()));
    }

    #[test]
    fn listed_unit_signature() {
        assert_eq!(<ListedUnit as Type>::SIGNATURE.to_string(), "(ssssssouso)");
        assert_eq!(<ListedJob as Type>::SIGNATURE.to_string(), "(usssoo)");
        assert_eq!(<Process as Type>::SIGNATURE.to_string(), "(sus)");
    }
}
