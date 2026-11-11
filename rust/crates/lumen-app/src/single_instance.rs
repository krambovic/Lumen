//! Launcher-facing Lumen activation and deep-link delivery.

use lumen_core::deeplinks::{
    MAX_DEEP_LINK_LENGTH, encode_instance_message, find_lumen_deep_link, parse_instance_message,
    should_start_in_tray,
};
use lumen_platform::single_instance::{InstanceLaunch, PrimaryInstance, acquire_or_forward};

pub use lumen_platform::single_instance::{activate_window, show_unavailable_warning};

pub enum Launch {
    Primary {
        activations: ActivationReceiver,
        initial_deep_link: String,
        start_in_tray: bool,
    },
    Forwarded,
    Unavailable,
}

pub struct ActivationReceiver(PrimaryInstance);

#[derive(Debug, PartialEq, Eq)]
pub enum InstanceActivation {
    Activate,
    Tray,
    DeepLink(String),
}

impl InstanceActivation {
    fn from_payload(payload: &[u8]) -> Self {
        match parse_instance_message(payload) {
            (command, link) if command == "deeplink" => Self::DeepLink(link),
            (command, _) if command == "tray" => Self::Tray,
            _ => Self::Activate,
        }
    }

    pub fn should_activate_window(&self) -> bool {
        !matches!(self, Self::Tray)
    }
}

impl ActivationReceiver {
    pub fn try_recv(&mut self) -> Option<InstanceActivation> {
        self.0.try_recv().map(|message| InstanceActivation::from_payload(&message))
    }

    pub async fn recv(&mut self) -> Option<InstanceActivation> {
        self.0
            .recv()
            .await
            .map(|message| InstanceActivation::from_payload(&message))
    }

    pub async fn close(self) {
        self.0.close().await;
    }
}

pub async fn launch(arguments: &[String]) -> Launch {
    let initial_deep_link = find_lumen_deep_link(arguments);
    let start_in_tray = should_start_in_tray(arguments, &initial_deep_link);
    let relaunching = arguments
        .iter()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "--relaunch-as-admin" | "--relaunched"));
    match acquire_or_forward(
        &encode_instance_message(arguments),
        relaunching,
        !start_in_tray,
        MAX_DEEP_LINK_LENGTH + 1024,
    )
    .await
    {
        InstanceLaunch::Primary(primary) => Launch::Primary {
            activations: ActivationReceiver(primary),
            initial_deep_link,
            start_in_tray,
        },
        InstanceLaunch::Forwarded => Launch::Forwarded,
        InstanceLaunch::Unavailable => Launch::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_policy_matches_tray_relaunch_and_deep_link_messages() {
        let link = "lumen://add?url=https%3A%2F%2Fexample.test";
        for (args, expected, foreground) in [
            (vec!["Lumen.exe", "--tray"], InstanceActivation::Tray, false),
            (vec!["--tray"], InstanceActivation::Tray, false),
            (
                vec!["Lumen.exe", "--tray", "--relaunched"],
                InstanceActivation::Activate,
                true,
            ),
            (
                vec!["Lumen.exe", "--tray", "--relaunch-as-admin"],
                InstanceActivation::Activate,
                true,
            ),
            (
                vec!["Lumen.exe", "--tray", link],
                InstanceActivation::DeepLink(link.into()),
                true,
            ),
        ] {
            let args: Vec<_> = args.into_iter().map(str::to_owned).collect();
            let activation = InstanceActivation::from_payload(&encode_instance_message(&args));
            assert_eq!(activation, expected);
            assert_eq!(activation.should_activate_window(), foreground);
            assert_eq!(
                !should_start_in_tray(&args, &find_lumen_deep_link(&args)),
                foreground
            );
        }
        assert_eq!(
            InstanceActivation::from_payload(b"activate"),
            InstanceActivation::Activate
        );
    }
}
