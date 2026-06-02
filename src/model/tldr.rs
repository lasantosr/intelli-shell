use clap::ValueEnum;

/// Selects which git transport `tldr fetch` should use for the upstream repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum TldrConnectionMode {
    /// Detect the transport automatically: reuse the existing clone's remote (honoring git `insteadOf`
    /// rewrites), otherwise fall back to HTTPS.
    #[default]
    Auto,
    /// Fetch the public repository over HTTPS.
    Https,
    /// Fetch the repository over SSH using the local git/SSH configuration.
    Ssh,
}
