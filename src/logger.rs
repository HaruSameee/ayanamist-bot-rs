use crate::Error;
use std::sync::{LazyLock, Mutex};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt};

static LOG_GUARD: LazyLock<Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>> =
    LazyLock::new(Default::default);

pub fn init_tracing_subscriber() -> Result<(), Error> {
    let file_appender = tracing_appender::rolling::daily("logs", "app.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    *LOG_GUARD.lock().expect("failed to lock") = Some(guard);

    let crate_name = env!("CARGO_CRATE_NAME");

    #[cfg(debug_assertions)]
    let file_layer_filter = format!("{crate_name}=trace,warn");
    #[cfg(not(debug_assertions))]
    let file_layer_filter = format!("{crate_name}=debug,warn");

    #[cfg(debug_assertions)]
    let console_layer_filter = format!("{crate_name}=trace,warn");
    #[cfg(not(debug_assertions))]
    let console_layer_filter = format!("{crate_name}=info,warn");

    let file_layer = fmt::Layer::default()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_filter(EnvFilter::new(&file_layer_filter));

    let console_layer = fmt::Layer::default()
        .with_writer(std::io::stdout)
        .with_filter(EnvFilter::new(&console_layer_filter));

    let subscriber = Registry::default().with(file_layer).with(console_layer);

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}
