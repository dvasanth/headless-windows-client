//! Main connlib library for gateway.
pub use connlib_shared::{get_device_id, messages::ResourceDescription, Callbacks, Error};

use crate::control::ControlSignaler;
use backoff::{backoff::Backoff, ExponentialBackoffBuilder};
use connlib_shared::control::SecureUrl;
use connlib_shared::{control::PhoenixChannel, login_url, CallbackErrorFacade, Mode, Result};
use control::ControlPlane;
use firezone_tunnel::Tunnel;
use messages::IngressMessages;
use secrecy::{Secret, SecretString};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use url::Url;

mod control;
mod messages;

struct StopRuntime;

/// A session is the entry-point for connlib, maintains the runtime and the tunnel.
///
/// A session is created using [Session::connect], then to stop a session we use [Session::disconnect].
pub struct Session<CB: Callbacks> {
    runtime_stopper: tokio::sync::mpsc::Sender<StopRuntime>,
    pub callbacks: CallbackErrorFacade<CB>,
}

macro_rules! fatal_error {
    ($result:expr, $rt:expr, $cb:expr) => {
        match $result {
            Ok(res) => res,
            Err(err) => {
                Self::disconnect_inner($rt, $cb, Some(err));
                return;
            }
        }
    };
}

impl<CB> Session<CB>
where
    CB: Callbacks + 'static,
{
    /// Starts a session in the background.
    ///
    /// This will:
    /// 1. Create and start a tokio runtime
    /// 2. Connect to the control plane to the portal
    /// 3. Start the tunnel in the background and forward control plane messages to it.
    ///
    /// The generic parameter `CB` should implement all the handlers and that's how errors will be surfaced.
    ///
    /// On a fatal error you should call `[Session::disconnect]` and start a new one.
    // TODO: token should be something like SecretString but we need to think about FFI compatibility
    pub fn connect(
        portal_url: impl TryInto<Url>,
        token: SecretString,
        device_id: String,
        callbacks: CB,
    ) -> Result<Self> {
        // TODO: We could use tokio::runtime::current() to get the current runtime
        // which could work with swift-rust that already runs a runtime. But IDK if that will work
        // in all platforms, a couple of new threads shouldn't bother none.
        // Big question here however is how do we get the result? We could block here await the result and spawn a new task.
        // but then platforms should know that this function is blocking.

        let callbacks = CallbackErrorFacade(callbacks);
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let this = Self {
            runtime_stopper: tx.clone(),
            callbacks,
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        {
            let callbacks = this.callbacks.clone();
            let default_panic_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new({
                let tx = tx.clone();
                move |info| {
                    let tx = tx.clone();
                    let err = info
                        .payload()
                        .downcast_ref::<&str>()
                        .map(|s| Error::Panic(s.to_string()))
                        .unwrap_or(Error::PanicNonStringPayload);
                    Self::disconnect_inner(tx, &callbacks, Some(err));
                    default_panic_hook(info);
                }
            }));
        }

        Self::connect_inner(
            &runtime,
            tx,
            portal_url.try_into().map_err(|_| Error::UriError)?,
            token,
            device_id,
            this.callbacks.clone(),
        );
        std::thread::spawn(move || {
            rx.blocking_recv();
            runtime.shutdown_background();
        });

        Ok(this)
    }

    fn connect_inner(
        runtime: &Runtime,
        runtime_stopper: tokio::sync::mpsc::Sender<StopRuntime>,
        portal_url: Url,
        token: SecretString,
        device_id: String,
        callbacks: CallbackErrorFacade<CB>,
    ) {
        runtime.spawn(async move {
            let (connect_url, private_key) = fatal_error!(
                login_url(Mode::Gateway, portal_url, token, device_id),
                runtime_stopper,
                &callbacks
            );

            // This is kinda hacky, the buffer size is 1 so that we make sure that we
            // process one message at a time, blocking if a previous message haven't been processed
            // to force queue ordering.
            let (control_plane_sender, mut control_plane_receiver) = tokio::sync::mpsc::channel(1);

            let mut connection = PhoenixChannel::<_, IngressMessages, IngressMessages, IngressMessages>::new(Secret::new(SecureUrl::from_url(connect_url)), move |msg, reference| {
                let control_plane_sender = control_plane_sender.clone();
                async move {
                    tracing::trace!("Received message: {msg:?}");
                    if let Err(e) = control_plane_sender.send((msg, reference)).await {
                        tracing::warn!("Received a message after handler already closed: {e}. Probably message received during session clean up.");
                    }
                }
            });

            // Used to send internal messages
            let control_signaler = ControlSignaler { control_signal: connection.sender_with_topic("gateway".to_owned()) };
            let tunnel = fatal_error!(
                Tunnel::new(private_key, control_signaler.clone(), callbacks.clone()).await,
                runtime_stopper,
                &callbacks
            );

            let mut control_plane = ControlPlane {
                tunnel: Arc::new(tunnel),
                control_signaler,
            };

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(10));
                loop {
                    tokio::select! {
                        Some((msg, _)) = control_plane_receiver.recv() => {
                            match msg {
                                Ok(msg) => control_plane.handle_message(msg).await?,
                                Err(_msg_reply) => todo!(),
                            }
                        },
                        _ = interval.tick() => control_plane.stats_event().await,
                        else => break
                    }
                }

                Result::Ok(())
            });

            tokio::spawn(async move {
                let mut exponential_backoff = ExponentialBackoffBuilder::default()
                    .with_max_elapsed_time(None)
                    .build();
                loop {
                    // `connection.start` calls the callback only after connecting
                    tracing::debug!("Attempting connection to portal...");
                    let result = connection.start(vec!["gateway".to_owned()], || exponential_backoff.reset()).await;
                    tracing::warn!("Disconnected from the portal");
                    if let Err(e) = &result {
                        tracing::warn!(error = ?e, "Portal connection error");
                    }
                    if let Some(t) = exponential_backoff.next_backoff() {
                        tracing::warn!("Error connecting to portal, retrying in {} seconds", t.as_secs());
                        let _ = callbacks.on_error(&result.err().unwrap_or(Error::PortalConnectionError(tokio_tungstenite::tungstenite::Error::ConnectionClosed)));
                        tokio::time::sleep(t).await;
                    } else {
                        tracing::error!("Connection to portal failed, giving up");
                        fatal_error!(
                            result.and(Err(Error::PortalConnectionError(tokio_tungstenite::tungstenite::Error::ConnectionClosed))),
                            runtime_stopper,
                            &callbacks
                        );
                    }
                }

            });

        });
    }

    fn disconnect_inner(
        runtime_stopper: tokio::sync::mpsc::Sender<StopRuntime>,
        callbacks: &CallbackErrorFacade<CB>,
        error: Option<Error>,
    ) {
        // 1. Close the websocket connection
        // 2. Free the device handle (Linux)
        // 3. Close the file descriptor (Linux/Android)
        // 4. Remove the mapping

        // The way we cleanup the tasks is we drop the runtime
        // this means we don't need to keep track of different tasks
        // but if any of the tasks never yields this will block forever!
        // So always yield and if you spawn a blocking tasks rewrite this.
        // Furthermore, we will depend on Drop impls to do the list above so,
        // implement them :)
        // if there's no receiver the runtime is already stopped
        // there's an edge case where this is called before the thread is listening for stop threads.
        // but I believe in that case the channel will be in a signaled state achieving the same result

        if let Err(err) = runtime_stopper.try_send(StopRuntime) {
            tracing::error!("Couldn't stop runtime: {err}");
        }

        let _ = callbacks.on_disconnect(error.as_ref());
    }

    /// Cleanup a [Session].
    ///
    /// For now this just drops the runtime, which should drop all pending tasks.
    /// Further cleanup should be done here. (Otherwise we can just drop [Session]).
    pub fn disconnect(&mut self, error: Option<Error>) {
        Self::disconnect_inner(self.runtime_stopper.clone(), &self.callbacks, error)
    }
}
