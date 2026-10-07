// SPDX-License-Identifier: GPL-3.0-or-later
package dev.nasaru.collector;

import android.Manifest;
import android.app.AppOpsManager;
import android.app.IProcessObserver;
import android.content.Context;
import android.content.pm.PackageManager;
import android.content.pm.ServiceInfo;
import android.os.Process;

import java.io.PrintWriter;
import java.lang.reflect.Method;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executor;

public final class FrameworkObserverMain {
    private static final Object OUTPUT_LOCK = new Object();
    private static final Object FGS_LOCK = new Object();
    private static final PrintWriter OUT = new PrintWriter(System.out, true);
    private static final Map<Integer, Map<Integer, Integer>> FGS_BY_UID = new HashMap<>();
    private static final Map<Integer, Integer> LAST_FGS_BY_UID = new HashMap<>();

    private FrameworkObserverMain() {}

    public static void main(String[] args) throws Exception {
        if (Process.myUid() != Process.ROOT_UID) {
            throw new SecurityException("Nasaru framework observer must run as root");
        }

        Context context = systemContext();
        if (context.checkPermission(
                Manifest.permission.WATCH_APPOPS,
                Process.myPid(),
                Process.myUid()) != PackageManager.PERMISSION_GRANTED) {
            throw new SecurityException("WATCH_APPOPS unavailable");
        }

        registerAppOps(context);
        registerProcessObserver();
        emit("NASARU_READY");
        new CountDownLatch(1).await();
    }

    private static Context systemContext() throws Exception {
        Class<?> activityThreadClass = Class.forName("android.app.ActivityThread");
        Method systemMain = activityThreadClass.getDeclaredMethod("systemMain");
        systemMain.setAccessible(true);
        Object activityThread = systemMain.invoke(null);

        Method getSystemContext = activityThreadClass.getDeclaredMethod("getSystemContext");
        getSystemContext.setAccessible(true);
        return (Context) getSystemContext.invoke(activityThread);
    }

    private static void registerAppOps(Context context) {
        AppOpsManager appOps = context.getSystemService(AppOpsManager.class);
        if (appOps == null) {
            throw new IllegalStateException("AppOpsManager unavailable");
        }

        String[] ops = {
            AppOpsManager.OPSTR_CAMERA,
            AppOpsManager.OPSTR_RECORD_AUDIO,
            AppOpsManager.OPSTR_FINE_LOCATION,
            AppOpsManager.OPSTR_COARSE_LOCATION
        };

        Executor direct = Runnable::run;
        appOps.startWatchingActive(
                ops,
                direct,
                new AppOpsManager.OnOpActiveChangedListener() {
                    @Override
                    public void onOpActiveChanged(
                            String op, int uid, String packageName, boolean active) {
                        String stableOp = stableAppOp(op);
                        if (stableOp != null) {
                            emit("NASARU_APPOP\t" + uid + "\t" + stableOp + "\t"
                                    + (active ? "1" : "0"));
                        }
                    }
                });
    }

    private static String stableAppOp(String op) {
        if (AppOpsManager.OPSTR_CAMERA.equals(op)) return "CAMERA";
        if (AppOpsManager.OPSTR_RECORD_AUDIO.equals(op)) return "MICROPHONE";
        if (AppOpsManager.OPSTR_FINE_LOCATION.equals(op)) return "FINE_LOCATION";
        if (AppOpsManager.OPSTR_COARSE_LOCATION.equals(op)) return "COARSE_LOCATION";
        return null;
    }

    private static void registerProcessObserver() throws Exception {
        IProcessObserver observer = new IProcessObserver.Stub() {
            @Override
            public void onProcessStarted(
                    int pid, int processUid, int packageUid,
                    String packageName, String processName) {}

            @Override
            public void onForegroundActivitiesChanged(
                    int pid, int uid, boolean foregroundActivities) {}

            @Override
            public void onForegroundServicesChanged(int pid, int uid, int serviceTypes) {
                updateForegroundServices(pid, uid, serviceTypes);
            }

            @Override
            public void onProcessDied(int pid, int uid) {
                updateForegroundServices(pid, uid, 0);
            }
        };

        Class<?> activityManager = Class.forName("android.app.ActivityManager");
        Method getService = activityManager.getDeclaredMethod("getService");
        getService.setAccessible(true);
        Object service = getService.invoke(null);

        Class<?> iActivityManager = Class.forName("android.app.IActivityManager");
        Class<?> iProcessObserver = Class.forName("android.app.IProcessObserver");
        Method register =
                iActivityManager.getMethod("registerProcessObserver", iProcessObserver);
        register.setAccessible(true);
        register.invoke(service, observer);
    }

    private static void updateForegroundServices(int pid, int uid, int serviceTypes) {
        synchronized (FGS_LOCK) {
            Map<Integer, Integer> perProcess =
                    FGS_BY_UID.computeIfAbsent(uid, ignored -> new HashMap<>());

            if (serviceTypes == 0) {
                perProcess.remove(pid);
            } else {
                perProcess.put(pid, serviceTypes);
            }
            if (perProcess.isEmpty()) {
                FGS_BY_UID.remove(uid);
            }

            int aggregate = 0;
            for (int raw : perProcess.values()) {
                aggregate |= raw;
            }

            int stable = stableFgsTypes(aggregate);
            int previous = LAST_FGS_BY_UID.getOrDefault(uid, 0);
            if (stable == previous) return;

            if (stable == 0) {
                LAST_FGS_BY_UID.remove(uid);
            } else {
                LAST_FGS_BY_UID.put(uid, stable);
            }
            emit("NASARU_FGS\t" + uid + "\t" + stable);
        }
    }

    private static int stableFgsTypes(int raw) {
        int stable = 0;
        if ((raw & ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA) != 0) stable |= 1 << 0;
        if ((raw & ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE) != 0) stable |= 1 << 1;
        if ((raw & ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC) != 0) stable |= 1 << 2;
        if ((raw & ServiceInfo.FOREGROUND_SERVICE_TYPE_HEALTH) != 0) stable |= 1 << 3;
        if ((raw & ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION) != 0) stable |= 1 << 4;
        if ((raw & ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE) != 0) stable |= 1 << 5;
        if ((raw & ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL) != 0) stable |= 1 << 6;
        return stable;
    }

    private static void emit(String line) {
        synchronized (OUTPUT_LOCK) {
            OUT.println(line);
            OUT.flush();
        }
    }
}
