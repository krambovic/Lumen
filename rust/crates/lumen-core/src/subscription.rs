//! Subscription fetching, header text repair and offline provider presentation.
//!
//! Port of `windows/xray_fluent/subscription_fetcher.py` (in-process part),
//! `subscription_text.py` and `subscription_presentation.py`.

pub mod fetcher;
pub mod presentation;
pub mod request;
pub mod response;
pub mod text;

pub use fetcher::{fetch_subscription_http, SubscriptionFetcherError, SubscriptionHttpPayload};
pub use presentation::present_subscription;
pub use text::repair_utf8_header;
