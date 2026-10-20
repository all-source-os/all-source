pub mod bing;
pub mod crux;
pub mod ga4;
pub mod gsc;
pub mod psi;
pub mod render;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Source {
    Gsc,
    Ga4,
    Psi,
    Crux,
    Bing,
    Render,
}

impl Source {
    pub const ALL: &'static [Self] = &[
        Self::Gsc,
        Self::Ga4,
        Self::Psi,
        Self::Crux,
        Self::Bing,
        Self::Render,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gsc => "Google Search Console",
            Self::Ga4 => "GA4 Data API",
            Self::Psi => "PageSpeed Insights",
            Self::Crux => "Chrome UX Report",
            Self::Bing => "Bing Webmaster Tools",
            Self::Render => "Headless Chrome rendered DOM",
        }
    }

    /// Env vars this source needs. Empty means it needs no credential.
    pub fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Gsc => gsc::KEYS,
            Self::Ga4 => ga4::KEYS,
            Self::Psi => psi::KEYS,
            Self::Crux => crux::KEYS,
            Self::Bing => bing::KEYS,
            Self::Render => &[],
        }
    }
}
