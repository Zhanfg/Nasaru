// SPDX-License-Identifier: GPL-3.0-or-later
package android.app;

import android.os.Binder;
import android.os.IBinder;
import android.os.IInterface;
import android.os.RemoteException;

/** Compile-only shape. Never packaged into the runtime DEX. */
public interface IProcessObserver extends IInterface {
    void onProcessStarted(
            int pid, int processUid, int packageUid,
            String packageName, String processName) throws RemoteException;
    void onForegroundActivitiesChanged(
            int pid, int uid, boolean foregroundActivities) throws RemoteException;
    void onForegroundServicesChanged(
            int pid, int uid, int serviceTypes) throws RemoteException;
    void onProcessDied(int pid, int uid) throws RemoteException;

    abstract class Stub extends Binder implements IProcessObserver {
        @Override
        public IBinder asBinder() {
            return this;
        }
    }
}
