use crate::error::Result;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct ServiceHealth {
    /// Whether or not the service reports as healthy.
    pub(crate) is_healthy: bool,
    /// In the event of an unhealthy service, the service's failure exit code goes here.
    pub(crate) exit_code: Option<u32>,
}

pub(crate) trait ServiceCheck {
    /// Checks the given service to see if it is healthy.
    fn check(&self, service_name: &str) -> Result<ServiceHealth>;
}

pub(crate) struct SystemdCheck {}

impl ServiceCheck for SystemdCheck {
    fn check(&self, service_name: &str) -> Result<ServiceHealth> {
        unimplemented!();
    }
}
