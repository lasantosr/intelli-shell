use std::{
    path::{Path, PathBuf},
    thread,
};

use crossbeam_channel::{Sender, bounded, unbounded};
use rusqlite::{Connection, OpenFlags};
use tokio::sync::oneshot;

/// A `SqliteClientBuilder` can be used to create a [`Client`] with custom configuration.
#[derive(Clone, Debug, Default)]
pub struct SqliteClientBuilder {
    pub(crate) path: Option<PathBuf>,
    pub(crate) flags: OpenFlags,
}

impl SqliteClientBuilder {
    /// Returns a new [`SqliteClientBuilder`] with the default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify the path of the sqlite3 database to open.
    ///
    /// By default, an in-memory database is used.
    pub fn path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.path = Some(path.as_ref().into());
        self
    }

    /// Specify the [`OpenFlags`] to use when opening a new connection.
    ///
    /// By default, [`OpenFlags::default()`] is used.
    pub fn flags(mut self, flags: OpenFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Returns a new [`Client`] that uses the `SqliteClientBuilder` configuration.
    pub async fn open(self) -> Result<SqliteClient, Error> {
        SqliteClient::open(self).await
    }
}

/// Represents a single sqlite connection that can be used from async contexts.
pub struct SqliteClient {
    conn_tx: Sender<Command>,
}

impl SqliteClient {
    async fn open(mut builder: SqliteClientBuilder) -> Result<Self, Error> {
        let path = builder.path.take().unwrap_or_else(|| ":memory:".into());
        let (open_tx, open_rx) = oneshot::channel();

        thread::spawn(move || {
            let (conn_tx, conn_rx) = unbounded();

            let mut conn = match Connection::open_with_flags(path, builder.flags) {
                Ok(conn) => conn,
                Err(err) => {
                    if let Err(Err(err)) = open_tx.send(Err(err)) {
                        tracing::error!("Error sending sqlite connection error: {err:?}");
                    }
                    return;
                }
            };

            let client = Self { conn_tx };
            if open_tx.send(Ok(client)).is_err() {
                tracing::error!("Error sending sqlite connection");
            }

            while let Ok(cmd) = conn_rx.recv() {
                match cmd {
                    Command::Func(func) => func(&mut conn),
                    Command::Shutdown(func) => match conn.close() {
                        Ok(()) => {
                            func(Ok(()));
                            return;
                        }
                        Err((c, e)) => {
                            conn = c;
                            func(Err(e.into()));
                        }
                    },
                }
            }
        });

        Ok(open_rx.await??)
    }
}

impl SqliteClient {
    /// Invokes the provided function with a [`rusqlite::Connection`].
    pub async fn conn<F, T, E>(&self, func: F) -> Result<T, E>
    where
        F: FnOnce(&Connection) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: From<Error> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.conn_tx
            .send(Command::Func(Box::new(move |conn| {
                if tx.send(func(conn)).is_err() {
                    tracing::error!("Error sending sqlite response");
                }
            })))
            .map_err(Error::from)?;
        rx.await.map_err(Error::from)?
    }

    /// Invokes the provided function with a mutable [`rusqlite::Connection`]
    pub async fn conn_mut<F, T, E>(&self, func: F) -> Result<T, E>
    where
        F: FnOnce(&mut Connection) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: From<Error> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.conn_tx
            .send(Command::Func(Box::new(move |conn| {
                if tx.send(func(conn)).is_err() {
                    tracing::error!("Error sending sqlite response");
                }
            })))
            .map_err(Error::from)?;
        rx.await.map_err(Error::from)?
    }

    /// Closes the underlying sqlite connection, blocking the current thread until complete.
    pub fn close_blocking(&self) -> Result<(), Error> {
        let (tx, rx) = bounded(1);
        let func = Box::new(move |res| _ = tx.send(res));
        if self.conn_tx.send(Command::Shutdown(func)).is_err() {
            return Ok(());
        }
        // If receiving fails, the connection is already closed.
        rx.recv().unwrap_or(Ok(()))
    }
}

impl Drop for SqliteClient {
    fn drop(&mut self) {
        if let Err(err) = self.close_blocking() {
            tracing::error!("Error closing sqlite client: {err:?}");
        }
    }
}

enum Command {
    Func(Box<dyn FnOnce(&mut Connection) + Send>),
    Shutdown(Box<dyn FnOnce(Result<(), Error>) + Send>),
}

#[derive(Debug)]
pub enum Error {
    /// Indicates that the connection to the sqlite database is closed.
    Closed,
    /// Represents a [`rusqlite::Error`].
    Rusqlite(rusqlite::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Closed => write!(f, "connection to sqlite database closed"),
            Error::Rusqlite(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Rusqlite(err) => Some(err),
            _ => None,
        }
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(_value: crossbeam_channel::SendError<T>) -> Self {
        Error::Closed
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(_value: crossbeam_channel::RecvError) -> Self {
        Error::Closed
    }
}

impl From<oneshot::error::RecvError> for Error {
    fn from(_value: oneshot::error::RecvError) -> Self {
        Error::Closed
    }
}

impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Error::Rusqlite(value)
    }
}
