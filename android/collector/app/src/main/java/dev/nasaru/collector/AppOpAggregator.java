// SPDX-License-Identifier: GPL-3.0-or-later
package dev.nasaru.collector;

import android.app.AppOpsManager;

import java.util.HashSet;
import java.util.Objects;
import java.util.Set;

final class AppOpAggregator {
    static final int ACTIVE_CAMERA = 1;
    static final int ACTIVE_MICROPHONE = 2;
    static final int ACTIVE_LOCATION = 3;

    private final Set<Key> camera = new HashSet<>();
    private final Set<Key> microphone = new HashSet<>();
    private final Set<Key> location = new HashSet<>();

    Transition update(
            String op,
            int uid,
            String packageName,
            int virtualDeviceId,
            boolean active) {
        int resource = resourceFor(op);
        if (resource == 0) {
            return null;
        }

        Set<Key> set = setFor(resource);
        boolean before = hasUid(set, uid);
        Key key = new Key(uid, packageName, op, virtualDeviceId);
        if (active) {
            set.add(key);
        } else {
            set.remove(key);
        }
        boolean after = hasUid(set, uid);
        if (before == after) {
            return null;
        }
        return new Transition(uid, resource, after);
    }

    private static int resourceFor(String op) {
        if (AppOpsManager.OPSTR_CAMERA.equals(op)) {
            return ACTIVE_CAMERA;
        }
        if (AppOpsManager.OPSTR_RECORD_AUDIO.equals(op)) {
            return ACTIVE_MICROPHONE;
        }
        if (AppOpsManager.OPSTR_FINE_LOCATION.equals(op)
                || AppOpsManager.OPSTR_COARSE_LOCATION.equals(op)) {
            return ACTIVE_LOCATION;
        }
        return 0;
    }

    private Set<Key> setFor(int resource) {
        return switch (resource) {
            case ACTIVE_CAMERA -> camera;
            case ACTIVE_MICROPHONE -> microphone;
            case ACTIVE_LOCATION -> location;
            default -> throw new IllegalArgumentException("unknown active op");
        };
    }

    private static boolean hasUid(Set<Key> set, int uid) {
        for (Key key : set) {
            if (key.uid == uid) {
                return true;
            }
        }
        return false;
    }

    static final class Transition {
        final int uid;
        final int activeOp;
        final boolean active;

        Transition(int uid, int activeOp, boolean active) {
            this.uid = uid;
            this.activeOp = activeOp;
            this.active = active;
        }
    }

    private static final class Key {
        final int uid;
        final String packageName;
        final String op;
        final int virtualDeviceId;

        Key(int uid, String packageName, String op, int virtualDeviceId) {
            this.uid = uid;
            this.packageName = packageName;
            this.op = op;
            this.virtualDeviceId = virtualDeviceId;
        }

        @Override
        public boolean equals(Object other) {
            if (this == other) {
                return true;
            }
            if (!(other instanceof Key key)) {
                return false;
            }
            return uid == key.uid
                    && virtualDeviceId == key.virtualDeviceId
                    && packageName.equals(key.packageName)
                    && op.equals(key.op);
        }

        @Override
        public int hashCode() {
            return Objects.hash(uid, packageName, op, virtualDeviceId);
        }
    }
}
