// SPDX-License-Identifier: GPL-3.0-or-later

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Camera,
    Microphone,
    Location,
    SensitiveFile,
    NetworkEgress,
    Identity,
    Contacts,
    Clipboard,
    BluetoothScan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow(Reason),
    Deny(Reason),
    NeedModel(Reason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    TrustedSystemContext,
    UserInitiated,
    ForegroundUse,
    ExistingHandleOrFlow,
    ActiveCapability,
    HardBackgroundSensorInvariant,
    HardSensitiveFileInvariant,
    RecentBlockedSensitiveReadBeforeEgress,
    AmbiguousBackgroundContext,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Context {
    pub trusted_system_context: bool,
    pub foreground: bool,
    pub user_initiated: bool,
    pub existing_handle_or_flow: bool,
    pub active_capability: bool,
    pub recent_blocked_sensitive_read: bool,
}

pub fn decide(resource: Resource, ctx: Context) -> Decision {
    if ctx.trusted_system_context {
        return Decision::Allow(Reason::TrustedSystemContext);
    }
    if ctx.user_initiated {
        return Decision::Allow(Reason::UserInitiated);
    }
    if ctx.foreground {
        return Decision::Allow(Reason::ForegroundUse);
    }
    if ctx.existing_handle_or_flow {
        return Decision::Allow(Reason::ExistingHandleOrFlow);
    }
    if ctx.active_capability {
        return Decision::Allow(Reason::ActiveCapability);
    }

    match resource {
        Resource::Camera | Resource::Microphone | Resource::Location => {
            Decision::Deny(Reason::HardBackgroundSensorInvariant)
        }
        Resource::SensitiveFile => Decision::Deny(Reason::HardSensitiveFileInvariant),
        Resource::NetworkEgress if ctx.recent_blocked_sensitive_read => {
            Decision::Deny(Reason::RecentBlockedSensitiveReadBeforeEgress)
        }
        Resource::NetworkEgress
        | Resource::Identity
        | Resource::Contacts
        | Resource::Clipboard
        | Resource::BluetoothScan => Decision::NeedModel(Reason::AmbiguousBackgroundContext),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_camera_without_capability_is_denied() {
        assert_eq!(
            decide(Resource::Camera, Context::default()),
            Decision::Deny(Reason::HardBackgroundSensorInvariant)
        );
    }

    #[test]
    fn legitimate_existing_background_transfer_survives() {
        let ctx = Context { existing_handle_or_flow: true, ..Context::default() };
        assert_eq!(
            decide(Resource::SensitiveFile, ctx),
            Decision::Allow(Reason::ExistingHandleOrFlow)
        );
    }

    #[test]
    fn ambiguous_network_egress_reaches_model_only_when_needed() {
        assert_eq!(
            decide(Resource::NetworkEgress, Context::default()),
            Decision::NeedModel(Reason::AmbiguousBackgroundContext)
        );
    }

    #[test]
    fn recent_blocked_sensitive_read_hardens_new_egress() {
        let ctx = Context { recent_blocked_sensitive_read: true, ..Context::default() };
        assert_eq!(
            decide(Resource::NetworkEgress, ctx),
            Decision::Deny(Reason::RecentBlockedSensitiveReadBeforeEgress)
        );
    }
}
