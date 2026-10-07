// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_bridge::TrustedSignal;
use std::collections::BTreeMap;

pub const RECENT_USER_WINDOW_NS: u64 = 15_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UidImportance {
    Foreground,
    ForegroundService,
    Visible,
    Perceptible,
    Service,
    Cached,
    Gone,
}

impl UidImportance {
    pub fn is_directly_user_visible(self) -> bool {
        matches!(self, Self::Foreground | Self::Visible)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FgsTypes(u16);

impl FgsTypes {
    pub const NONE: Self = Self(0);
    pub const CAMERA: Self = Self(1 << 0);
    pub const CONNECTED_DEVICE: Self = Self(1 << 1);
    pub const DATA_SYNC: Self = Self(1 << 2);
    pub const HEALTH: Self = Self(1 << 3);
    pub const LOCATION: Self = Self(1 << 4);
    pub const MICROPHONE: Self = Self(1 << 5);
    pub const PHONE_CALL: Self = Self(1 << 6);

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Default for FgsTypes {
    fn default() -> Self {
        Self::NONE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveOp {
    Camera,
    Microphone,
    Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalEvent {
    UidImportanceChanged {
        uid: u32,
        importance: UidImportance,
        monotonic_ns: u64,
    },
    ForegroundServiceTypesChanged {
        uid: u32,
        types: FgsTypes,
        monotonic_ns: u64,
    },
    AppOpActiveChanged {
        uid: u32,
        op: ActiveOp,
        active: bool,
        monotonic_ns: u64,
    },
    CompanionPresenceChanged {
        uid: u32,
        present: bool,
        monotonic_ns: u64,
    },
    PackageAdded {
        uid: u32,
        third_party: bool,
        monotonic_ns: u64,
    },
    PackageRemoved {
        uid: u32,
        replacing: bool,
        monotonic_ns: u64,
    },
}

impl SignalEvent {
    pub const fn uid(self) -> u32 {
        match self {
            Self::UidImportanceChanged { uid, .. }
            | Self::ForegroundServiceTypesChanged { uid, .. }
            | Self::AppOpActiveChanged { uid, .. }
            | Self::CompanionPresenceChanged { uid, .. }
            | Self::PackageAdded { uid, .. }
            | Self::PackageRemoved { uid, .. } => uid,
        }
    }

    pub const fn monotonic_ns(self) -> u64 {
        match self {
            Self::UidImportanceChanged { monotonic_ns, .. }
            | Self::ForegroundServiceTypesChanged { monotonic_ns, .. }
            | Self::AppOpActiveChanged { monotonic_ns, .. }
            | Self::CompanionPresenceChanged { monotonic_ns, .. }
            | Self::PackageAdded { monotonic_ns, .. }
            | Self::PackageRemoved { monotonic_ns, .. } => monotonic_ns,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalEffect {
    ApplyThirdPartyBaseline { uid: u32 },
    RemoveUidState { uid: u32 },
    TrustedSession(TrustedSignal),
    AmbiguousContextChanged { uid: u32 },
}

#[derive(Debug, Clone, Copy, Default)]
struct ActiveOps {
    camera: bool,
    microphone: bool,
    location: bool,
}

impl ActiveOps {
    fn set(&mut self, op: ActiveOp, active: bool) {
        match op {
            ActiveOp::Camera => self.camera = active,
            ActiveOp::Microphone => self.microphone = active,
            ActiveOp::Location => self.location = active,
        }
    }
}

#[derive(Debug, Clone)]
struct UidState {
    importance: UidImportance,
    fgs_types: FgsTypes,
    ops: ActiveOps,
    companion_present: bool,
    last_user_visible_ns: Option<u64>,
    navigation_session: bool,
    companion_session: bool,
    voice_session: bool,
    video_session: bool,
}

impl Default for UidState {
    fn default() -> Self {
        Self {
            importance: UidImportance::Gone,
            fgs_types: FgsTypes::NONE,
            ops: ActiveOps::default(),
            companion_present: false,
            last_user_visible_ns: None,
            navigation_session: false,
            companion_session: false,
            voice_session: false,
            video_session: false,
        }
    }
}

impl UidState {
    fn recent_user_context(&self, now_ns: u64) -> bool {
        if self.importance.is_directly_user_visible() {
            return true;
        }
        self.last_user_visible_ns
            .is_some_and(|last| now_ns.saturating_sub(last) <= RECENT_USER_WINDOW_NS)
    }
}

#[derive(Debug, Default)]
pub struct SignalState {
    by_uid: BTreeMap<u32, UidState>,
}

impl SignalState {
    pub fn ingest(&mut self, event: SignalEvent) -> Vec<SignalEffect> {
        let uid = event.uid();
        let now_ns = event.monotonic_ns();

        if let SignalEvent::PackageRemoved {
            replacing: false, ..
        } = event
        {
            self.by_uid.remove(&uid);
            return vec![SignalEffect::RemoveUidState { uid }];
        }

        let state = self.by_uid.entry(uid).or_default();
        let mut effects = Vec::new();

        match event {
            SignalEvent::UidImportanceChanged { importance, .. } => {
                state.importance = importance;
                if importance.is_directly_user_visible() {
                    state.last_user_visible_ns = Some(now_ns);
                }
            }
            SignalEvent::ForegroundServiceTypesChanged { types, .. } => {
                state.fgs_types = types;
            }
            SignalEvent::AppOpActiveChanged { op, active, .. } => {
                state.ops.set(op, active);
            }
            SignalEvent::CompanionPresenceChanged { present, .. } => {
                state.companion_present = present;
            }
            SignalEvent::PackageAdded { third_party, .. } => {
                if third_party {
                    effects.push(SignalEffect::ApplyThirdPartyBaseline { uid });
                }
            }
            SignalEvent::PackageRemoved {
                replacing: true, ..
            } => {
                effects.push(SignalEffect::AmbiguousContextChanged { uid });
            }
            SignalEvent::PackageRemoved {
                replacing: false, ..
            } => unreachable!(),
        }

        effects.extend(recompute_sessions(uid, now_ns, state));

        if effects.is_empty() {
            effects.push(SignalEffect::AmbiguousContextChanged { uid });
        }

        effects
    }

    pub fn contains_uid(&self, uid: u32) -> bool {
        self.by_uid.contains_key(&uid)
    }

    pub fn uid_count(&self) -> usize {
        self.by_uid.len()
    }
}

fn recompute_sessions(uid: u32, now_ns: u64, state: &mut UidState) -> Vec<SignalEffect> {
    let mut out = Vec::new();
    let recent_user = state.recent_user_context(now_ns);

    let navigation =
        state.fgs_types.contains(FgsTypes::LOCATION) && state.ops.location && recent_user;
    transition(
        &mut state.navigation_session,
        navigation,
        TrustedSignal::NavigationStarted { uid },
        TrustedSignal::NavigationStopped { uid },
        &mut out,
    );

    let companion = state.companion_present
        && (state.fgs_types.contains(FgsTypes::CONNECTED_DEVICE)
            || state.fgs_types.contains(FgsTypes::HEALTH));
    transition(
        &mut state.companion_session,
        companion,
        TrustedSignal::CompanionHealthSyncStarted { uid },
        TrustedSignal::CompanionHealthSyncStopped { uid },
        &mut out,
    );

    let voice = state.ops.microphone
        && recent_user
        && (state.fgs_types.contains(FgsTypes::PHONE_CALL)
            || state.fgs_types.contains(FgsTypes::MICROPHONE));
    transition(
        &mut state.voice_session,
        voice,
        TrustedSignal::VoiceSessionStarted { uid },
        TrustedSignal::VoiceSessionStopped { uid },
        &mut out,
    );

    let video = voice && state.ops.camera && state.fgs_types.contains(FgsTypes::CAMERA);
    if video != state.video_session {
        state.video_session = video;
        if video {
            out.push(SignalEffect::TrustedSession(
                TrustedSignal::VideoSessionStarted { uid },
            ));
        } else {
            out.push(SignalEffect::TrustedSession(
                TrustedSignal::VideoSessionStopped { uid },
            ));
        }
    }

    out
}

fn transition(
    current: &mut bool,
    next: bool,
    start: TrustedSignal,
    stop: TrustedSignal,
    out: &mut Vec<SignalEffect>,
) {
    if *current == next {
        return;
    }
    *current = next;
    out.push(SignalEffect::TrustedSession(if next {
        start
    } else {
        stop
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ingest_all(state: &mut SignalState, events: &[SignalEvent]) -> Vec<SignalEffect> {
        events
            .iter()
            .flat_map(|event| state.ingest(*event))
            .collect()
    }

    #[test]
    fn location_fgs_alone_does_not_create_navigation_capability() {
        let mut state = SignalState::default();
        let effects = state.ingest(SignalEvent::ForegroundServiceTypesChanged {
            uid: 1,
            types: FgsTypes::LOCATION,
            monotonic_ns: 1,
        });
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            SignalEffect::TrustedSession(TrustedSignal::NavigationStarted { .. })
        )));
    }

    #[test]
    fn navigation_requires_recent_user_context_and_active_location_op() {
        let mut state = SignalState::default();
        let effects = ingest_all(
            &mut state,
            &[
                SignalEvent::UidImportanceChanged {
                    uid: 7,
                    importance: UidImportance::Foreground,
                    monotonic_ns: 1,
                },
                SignalEvent::ForegroundServiceTypesChanged {
                    uid: 7,
                    types: FgsTypes::LOCATION,
                    monotonic_ns: 2,
                },
                SignalEvent::AppOpActiveChanged {
                    uid: 7,
                    op: ActiveOp::Location,
                    active: true,
                    monotonic_ns: 3,
                },
            ],
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            SignalEffect::TrustedSession(TrustedSignal::NavigationStarted { uid: 7 })
        )));
    }

    #[test]
    fn stale_foreground_history_cannot_authorize_new_navigation() {
        let mut state = SignalState::default();
        ingest_all(
            &mut state,
            &[
                SignalEvent::UidImportanceChanged {
                    uid: 9,
                    importance: UidImportance::Foreground,
                    monotonic_ns: 0,
                },
                SignalEvent::UidImportanceChanged {
                    uid: 9,
                    importance: UidImportance::Cached,
                    monotonic_ns: 1,
                },
            ],
        );
        let effects = ingest_all(
            &mut state,
            &[
                SignalEvent::ForegroundServiceTypesChanged {
                    uid: 9,
                    types: FgsTypes::LOCATION,
                    monotonic_ns: RECENT_USER_WINDOW_NS + 2,
                },
                SignalEvent::AppOpActiveChanged {
                    uid: 9,
                    op: ActiveOp::Location,
                    active: true,
                    monotonic_ns: RECENT_USER_WINDOW_NS + 3,
                },
            ],
        );
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            SignalEffect::TrustedSession(TrustedSignal::NavigationStarted { .. })
        )));
    }

    #[test]
    fn companion_health_sync_requires_presence_plus_fgs_evidence() {
        let mut state = SignalState::default();
        let effects = ingest_all(
            &mut state,
            &[
                SignalEvent::CompanionPresenceChanged {
                    uid: 11,
                    present: true,
                    monotonic_ns: 1,
                },
                SignalEvent::ForegroundServiceTypesChanged {
                    uid: 11,
                    types: FgsTypes::HEALTH,
                    monotonic_ns: 2,
                },
            ],
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            SignalEffect::TrustedSession(TrustedSignal::CompanionHealthSyncStarted { uid: 11 })
        )));
    }

    #[test]
    fn microphone_fgs_without_active_op_is_only_evidence() {
        let mut state = SignalState::default();
        let effects = ingest_all(
            &mut state,
            &[
                SignalEvent::UidImportanceChanged {
                    uid: 12,
                    importance: UidImportance::Foreground,
                    monotonic_ns: 1,
                },
                SignalEvent::ForegroundServiceTypesChanged {
                    uid: 12,
                    types: FgsTypes::MICROPHONE,
                    monotonic_ns: 2,
                },
            ],
        );
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            SignalEffect::TrustedSession(TrustedSignal::VoiceSessionStarted { .. })
        )));
    }

    #[test]
    fn video_upgrade_requires_camera_fgs_and_active_camera_op() {
        let mut state = SignalState::default();
        let effects = ingest_all(
            &mut state,
            &[
                SignalEvent::UidImportanceChanged {
                    uid: 13,
                    importance: UidImportance::Foreground,
                    monotonic_ns: 1,
                },
                SignalEvent::ForegroundServiceTypesChanged {
                    uid: 13,
                    types: FgsTypes::MICROPHONE.union(FgsTypes::CAMERA),
                    monotonic_ns: 2,
                },
                SignalEvent::AppOpActiveChanged {
                    uid: 13,
                    op: ActiveOp::Microphone,
                    active: true,
                    monotonic_ns: 3,
                },
                SignalEvent::AppOpActiveChanged {
                    uid: 13,
                    op: ActiveOp::Camera,
                    active: true,
                    monotonic_ns: 4,
                },
            ],
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            SignalEffect::TrustedSession(TrustedSignal::VideoSessionStarted { uid: 13 })
        )));
    }

    #[test]
    fn third_party_install_requests_baseline_but_system_package_does_not() {
        let mut state = SignalState::default();
        let third_party = state.ingest(SignalEvent::PackageAdded {
            uid: 20001,
            third_party: true,
            monotonic_ns: 1,
        });
        assert!(third_party.contains(&SignalEffect::ApplyThirdPartyBaseline { uid: 20001 }));

        let system = state.ingest(SignalEvent::PackageAdded {
            uid: 1000,
            third_party: false,
            monotonic_ns: 2,
        });
        assert!(!system.contains(&SignalEffect::ApplyThirdPartyBaseline { uid: 1000 }));
    }

    #[test]
    fn full_remove_forgets_uid_state() {
        let mut state = SignalState::default();
        state.ingest(SignalEvent::PackageAdded {
            uid: 20002,
            third_party: true,
            monotonic_ns: 1,
        });
        assert!(state.contains_uid(20002));

        let effects = state.ingest(SignalEvent::PackageRemoved {
            uid: 20002,
            replacing: false,
            monotonic_ns: 2,
        });
        assert_eq!(effects, vec![SignalEffect::RemoveUidState { uid: 20002 }]);
        assert!(!state.contains_uid(20002));
    }
}
