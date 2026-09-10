pub mod fetch;
pub mod proxies;
pub mod types;
pub mod unit;

use std::fmt;

use eyre::{Context, Result};
use zbus::{Connection, proxy::CacheProperties};

use self::proxies::ManagerProxy;

/// Which service manager we are talking to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    System,
    User,
}

impl Scope {
    pub fn toggle(self) -> Self {
        match self {
            Self::System => Self::User,
            Self::User => Self::System,
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => "system",
            Self::User => "user",
        })
    }
}

/// A connection to one service manager plus its proxies.
#[derive(Debug, Clone)]
pub struct Backend {
    pub conn: Connection,
    pub manager: ManagerProxy<'static>,
    pub scope: Scope,
}

impl Backend {
    pub async fn connect(scope: Scope) -> Result<Self> {
        let conn = match scope {
            Scope::System => Connection::system().await.context("connecting to the system bus")?,
            Scope::User => session_connection().await?,
        };
        // The manager never announces changes to NNames & co., so a property
        // cache would go stale immediately.
        let manager = ManagerProxy::builder(&conn)
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .context("creating the systemd manager proxy")?;
        Ok(Self { conn, manager, scope })
    }
}

/// Connect to the user's session bus, falling back to `$XDG_RUNTIME_DIR/bus`
/// when `DBUS_SESSION_BUS_ADDRESS` is unset (e.g. from a plain TTY).
async fn session_connection() -> Result<Connection> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
        return Connection::session().await.context("connecting to the session bus");
    }
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        // SAFETY: getuid never fails and has no preconditions.
        let uid = unsafe { libc_getuid() };
        format!("/run/user/{uid}")
    });
    let address = format!("unix:path={runtime_dir}/bus");
    zbus::connection::Builder::address(address.as_str())
        .context("parsing the session bus address")?
        .build()
        .await
        .with_context(|| format!("connecting to the user bus at {address}"))
}

unsafe extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}
