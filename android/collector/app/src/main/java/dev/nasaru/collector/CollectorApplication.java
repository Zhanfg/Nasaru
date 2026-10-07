// SPDX-License-Identifier: GPL-3.0-or-later
package dev.nasaru.collector;

import android.Manifest;
import android.app.AppOpsManager;
import android.app.Application;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.SystemClock;
import android.util.Log;

import java.util.HashSet;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class CollectorApplication extends Application {
    private static final String TAG = "NasaruCollector";
    private static final String WATCH_APPOPS = "android.permission.WATCH_APPOPS";

    private final ExecutorService executor = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "nasaru-collector");
        thread.setDaemon(true);
        return thread;
    });
    private final RiksuEmitter emitter = new RiksuEmitter();
    private final AppOpAggregator aggregator = new AppOpAggregator();

    private AppOpsManager appOps;
    private BroadcastReceiver packageReceiver;

    private final AppOpsManager.OnOpActiveChangedListener appOpsListener =
            new AppOpsManager.OnOpActiveChangedListener() {
                @Override
                public void onOpActiveChanged(
                        String op,
                        int uid,
                        String packageName,
                        boolean active) {
                    handleAppOp(op, uid, packageName, 0, active);
                }

                @Override
                public void onOpActiveChanged(
                        String op,
                        int uid,
                        String packageName,
                        String attributionTag,
                        int virtualDeviceId,
                        boolean active,
                        int attributionFlags,
                        int attributionChainId) {
                    handleAppOp(op, uid, packageName, virtualDeviceId, active);
                }
            };

    @Override
    public void onCreate() {
        super.onCreate();

        if (checkSelfPermission(WATCH_APPOPS) != PackageManager.PERMISSION_GRANTED) {
            Log.e(TAG, "WATCH_APPOPS missing; collector will not start");
            return;
        }

        appOps = getSystemService(AppOpsManager.class);
        if (appOps == null) {
            Log.e(TAG, "AppOpsManager unavailable");
            return;
        }

        registerPackageReceiver();
        executor.execute(this::emitInitialThirdPartyBaseline);

        appOps.startWatchingActive(
                new String[] {
                    AppOpsManager.OPSTR_CAMERA,
                    AppOpsManager.OPSTR_RECORD_AUDIO,
                    AppOpsManager.OPSTR_FINE_LOCATION,
                    AppOpsManager.OPSTR_COARSE_LOCATION
                },
                executor,
                appOpsListener);

        Log.i(TAG, "Nasaru collector started");
    }

    private void handleAppOp(
            String op,
            int uid,
            String packageName,
            int virtualDeviceId,
            boolean active) {
        AppOpAggregator.Transition transition =
                aggregator.update(op, uid, packageName, virtualDeviceId, active);
        if (transition == null) {
            return;
        }
        emitter.send(SignalEvent.appOp(
                transition.uid,
                SystemClock.elapsedRealtimeNanos(),
                transition.activeOp,
                transition.active));
    }

    private void registerPackageReceiver() {
        packageReceiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context context, Intent intent) {
                int uid = intent.getIntExtra(Intent.EXTRA_UID, -1);
                if (uid < 0) {
                    return;
                }
                String action = intent.getAction();
                if (Intent.ACTION_PACKAGE_ADDED.equals(action)) {
                    String packageName = intent.getData() == null
                            ? null
                            : intent.getData().getSchemeSpecificPart();
                    executor.execute(() -> emitPackageAdded(uid, packageName));
                } else if (Intent.ACTION_PACKAGE_REMOVED.equals(action)) {
                    boolean replacing = intent.getBooleanExtra(Intent.EXTRA_REPLACING, false);
                    executor.execute(() -> emitter.send(SignalEvent.packageRemoved(
                            uid,
                            SystemClock.elapsedRealtimeNanos(),
                            replacing)));
                }
            }
        };

        IntentFilter filter = new IntentFilter();
        filter.addAction(Intent.ACTION_PACKAGE_ADDED);
        filter.addAction(Intent.ACTION_PACKAGE_REMOVED);
        filter.addDataScheme("package");

        if (Build.VERSION.SDK_INT >= 33) {
            registerReceiver(packageReceiver, filter, Context.RECEIVER_EXPORTED);
        } else {
            registerReceiver(packageReceiver, filter);
        }
    }

    private void emitInitialThirdPartyBaseline() {
        PackageManager pm = getPackageManager();
        Set<Integer> emittedUids = new HashSet<>();
        for (ApplicationInfo info : pm.getInstalledApplications(0)) {
            if (getPackageName().equals(info.packageName)) {
                continue;
            }
            if (isThirdParty(info) && emittedUids.add(info.uid)) {
                emitter.send(SignalEvent.packageAdded(
                        info.uid,
                        SystemClock.elapsedRealtimeNanos(),
                        true));
            }
        }
    }

    private void emitPackageAdded(int uid, String packageName) {
        if (packageName == null || getPackageName().equals(packageName)) {
            return;
        }
        try {
            ApplicationInfo info = getPackageManager().getApplicationInfo(packageName, 0);
            emitter.send(SignalEvent.packageAdded(
                    uid,
                    SystemClock.elapsedRealtimeNanos(),
                    isThirdParty(info)));
        } catch (PackageManager.NameNotFoundException error) {
            Log.w(TAG, "package disappeared before classification: " + packageName);
        }
    }

    private static boolean isThirdParty(ApplicationInfo info) {
        int systemBits = ApplicationInfo.FLAG_SYSTEM | ApplicationInfo.FLAG_UPDATED_SYSTEM_APP;
        return (info.flags & systemBits) == 0;
    }
}
