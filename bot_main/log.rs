use eyre::Result;
use itertools::Itertools as _;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::{EnvFilter, Targets};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

pub fn init_tracing() {
    // WORKSPACE_CRATES is generated in build.rs
    let mut crates = env!("WORKSPACE_CRATES").split(',').collect::<Vec<_>>();
    crates.push(env!("CARGO_CRATE_NAME"));

    let workspace_filter = crates.iter().map(|c| format!("{c}=debug")).join(",");
    let filter = EnvFilter::new(format!("warn,{workspace_filter}"));

    let default_format = fmt::format().compact().without_time();
    let workspace_format = default_format.clone().with_source_location(true).with_target(false);

    tracing_subscriber::registry()
        .with(
            fmt::layer().event_format(default_format).with_filter(
                Targets::new()
                    .with_default(LevelFilter::TRACE)
                    .with_targets(crates.iter().map(|&c| (c, LevelFilter::OFF))),
            ),
        )
        .with(fmt::layer().event_format(workspace_format).with_filter(
            Targets::new().with_targets(crates.iter().map(|&c| (c, LevelFilter::TRACE))),
        ))
        .with(EnvFilter::try_from_default_env().unwrap_or(filter))
        .init();
}

pub unsafe fn init_eyre() -> Result<()> {
    if std::env::var("RUST_LIB_BACKTRACE").is_err() {
        unsafe { std::env::set_var("RUST_LIB_BACKTRACE", "1") }
    }

    color_eyre::config::HookBuilder::default()
        .add_frame_filter(Box::new(move |frames| {
            frames.retain(|frame| frame.name.as_ref().is_some_and(|f| f.starts_with("bot_")));
        }))
        .display_env_section(false)
        .install()
}
