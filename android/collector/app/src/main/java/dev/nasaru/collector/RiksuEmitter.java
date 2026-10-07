// SPDX-License-Identifier: GPL-3.0-or-later
package dev.nasaru.collector;

import android.net.LocalSocket;
import android.net.LocalSocketAddress;
import android.util.Log;

import java.io.IOException;
import java.io.OutputStream;

final class RiksuEmitter implements AutoCloseable {
    private static final String TAG = "NasaruCollector";
    static final String ABSTRACT_SOCKET_NAME = "nasaru.collector";

    private LocalSocket socket;
    private OutputStream output;
    private long sequence = 1;

    void send(SignalEvent event) {
        try {
            ensureConnected();
            writeFrame(event);
        } catch (IOException first) {
            closeQuietly();
            try {
                ensureConnected();
                writeFrame(event);
            } catch (IOException retry) {
                Log.w(TAG, "Riksu event dropped after reconnect failure", retry);
                closeQuietly();
            }
        }
    }

    private void ensureConnected() throws IOException {
        if (socket != null && socket.isConnected()) {
            return;
        }
        LocalSocket next = new LocalSocket(LocalSocket.SOCKET_SEQPACKET);
        next.connect(new LocalSocketAddress(
                ABSTRACT_SOCKET_NAME,
                LocalSocketAddress.Namespace.ABSTRACT));
        socket = next;
        output = next.getOutputStream();
        sequence = 1;
    }

    private void writeFrame(SignalEvent event) throws IOException {
        byte[] frame = RiksuSignalEncoder.encode(sequence, event);
        output.write(frame);
        output.flush();
        sequence++;
    }

    private void closeQuietly() {
        if (socket != null) {
            try {
                socket.close();
            } catch (IOException ignored) {
                // Closing is best-effort.
            }
        }
        socket = null;
        output = null;
        sequence = 1;
    }

    @Override
    public void close() {
        closeQuietly();
    }
}
