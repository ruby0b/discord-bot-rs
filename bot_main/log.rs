use itertools::Itertools as _;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::{EnvFilter, Targets};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// set up logging to stderr
pub fn init_tracing() {
    // WORKSPACE_CRATES is generated in build.rs to know which crates are in our workspace
    let mut crates = env!("WORKSPACE_CRATES").split(',').collect_vec();
    crates.push(env!("CARGO_CRATE_NAME"));

    // default formatting (targets everything except for the crates in this workspace)
    let default_format = fmt::format().compact().without_time();
    let registry = tracing_subscriber::registry().with(fmt::layer().event_format(default_format.clone()).with_filter(
        Targets::new().with_default(LevelFilter::TRACE).with_targets(crates.iter().map(|&c| (c, LevelFilter::OFF))),
    ));

    // workspace formatting (targets only the crates in this workspace)
    let workspace_format = default_format.clone().with_source_location(true).with_target(false);
    let registry = registry.with(
        fmt::layer()
            .event_format(workspace_format)
            .with_filter(Targets::new().with_targets(crates.iter().map(|&c| (c, LevelFilter::TRACE)))),
    );

    // take log level filters from RUST_LOG or use these hardcoded defaults
    let workspace_filter = crates.iter().map(|c| format!("{c}=debug")).join(",");
    let filter = EnvFilter::new(format!("warn,{workspace_filter}"));
    let registry = registry.with(EnvFilter::try_from_default_env().unwrap_or(filter));

    registry.init();
}

/// set up global eyre hook for errors
pub fn init_eyre() {
    eyre::set_hook(Box::new(eyre::DefaultHandler::default_with)).unwrap();
}
