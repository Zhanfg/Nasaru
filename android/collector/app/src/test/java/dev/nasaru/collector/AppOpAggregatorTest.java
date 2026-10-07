// SPDX-License-Identifier: GPL-3.0-or-later
package dev.nasaru.collector;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.app.AppOpsManager;

import org.junit.Test;

public final class AppOpAggregatorTest {
    @Test
    public void fineAndCoarseLocationAreAggregatedPerUid() {
        AppOpAggregator aggregator = new AppOpAggregator();

        AppOpAggregator.Transition first = aggregator.update(
                AppOpsManager.OPSTR_FINE_LOCATION, 42, "a", 0, true);
        assertTrue(first.active);

        assertNull(aggregator.update(
                AppOpsManager.OPSTR_COARSE_LOCATION, 42, "a", 0, true));
        assertNull(aggregator.update(
                AppOpsManager.OPSTR_FINE_LOCATION, 42, "a", 0, false));

        AppOpAggregator.Transition last = aggregator.update(
                AppOpsManager.OPSTR_COARSE_LOCATION, 42, "a", 0, false);
        assertFalse(last.active);
    }

    @Test
    public void sharedUidPackagesDoNotCauseFalseInactiveTransition() {
        AppOpAggregator aggregator = new AppOpAggregator();

        aggregator.update(AppOpsManager.OPSTR_CAMERA, 77, "pkg.a", 0, true);
        assertNull(aggregator.update(
                AppOpsManager.OPSTR_CAMERA, 77, "pkg.b", 0, true));
        assertNull(aggregator.update(
                AppOpsManager.OPSTR_CAMERA, 77, "pkg.a", 0, false));

        AppOpAggregator.Transition last = aggregator.update(
                AppOpsManager.OPSTR_CAMERA, 77, "pkg.b", 0, false);
        assertFalse(last.active);
    }
}
