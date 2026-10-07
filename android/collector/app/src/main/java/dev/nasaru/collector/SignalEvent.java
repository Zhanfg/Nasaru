// SPDX-License-Identifier: GPL-3.0-or-later
package dev.nasaru.collector;

final class SignalEvent {
    static final int KIND_UID_IMPORTANCE = 1;
    static final int KIND_FGS_TYPES = 2;
    static final int KIND_APPOP_ACTIVE = 3;
    static final int KIND_COMPANION_PRESENCE = 4;
    static final int KIND_PACKAGE_ADDED = 5;
    static final int KIND_PACKAGE_REMOVED = 6;

    final int kind;
    final int uid;
    final long monotonicNs;
    final int value;
    final boolean flag;

    private SignalEvent(int kind, int uid, long monotonicNs, int value, boolean flag) {
        this.kind = kind;
        this.uid = uid;
        this.monotonicNs = monotonicNs;
        this.value = value;
        this.flag = flag;
    }

    static SignalEvent uidImportance(int uid, long monotonicNs, int importance) {
        return new SignalEvent(KIND_UID_IMPORTANCE, uid, monotonicNs, importance, false);
    }

    static SignalEvent fgsTypes(int uid, long monotonicNs, int bits) {
        return new SignalEvent(KIND_FGS_TYPES, uid, monotonicNs, bits, false);
    }

    static SignalEvent appOp(int uid, long monotonicNs, int op, boolean active) {
        return new SignalEvent(KIND_APPOP_ACTIVE, uid, monotonicNs, op, active);
    }

    static SignalEvent companion(int uid, long monotonicNs, boolean present) {
        return new SignalEvent(KIND_COMPANION_PRESENCE, uid, monotonicNs, 0, present);
    }

    static SignalEvent packageAdded(int uid, long monotonicNs, boolean thirdParty) {
        return new SignalEvent(KIND_PACKAGE_ADDED, uid, monotonicNs, 0, thirdParty);
    }

    static SignalEvent packageRemoved(int uid, long monotonicNs, boolean replacing) {
        return new SignalEvent(KIND_PACKAGE_REMOVED, uid, monotonicNs, 0, replacing);
    }
}
