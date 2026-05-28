use clap::ValueEnum;

/// Selects which git transport `tldr fetch` should use for the upstream repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum TldrConnectionMode {
    /// Fetch the public repository over HTTPS.
    #[default]
    Https,
    /// Fetch the repository over SSH using the local git/SSH configuration.
    Ssh,
}
