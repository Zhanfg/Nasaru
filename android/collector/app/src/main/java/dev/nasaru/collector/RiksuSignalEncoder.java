// SPDX-License-Identifier: GPL-3.0-or-later
package dev.nasaru.collector;

import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

final class RiksuSignalEncoder {
    static final int RIKSU_HEADER_LEN = 32;
    static final int MAX_SIGNAL_PAYLOAD = 512;

    private static final int TLV_EVENT_KIND = 1;
    private static final int TLV_UID = 2;
    private static final int TLV_IMPORTANCE = 3;
    private static final int TLV_FGS_TYPES = 4;
    private static final int TLV_ACTIVE_OP = 5;
    private static final int TLV_ACTIVE = 6;
    private static final int TLV_PRESENT = 7;
    private static final int TLV_THIRD_PARTY = 8;
    private static final int TLV_REPLACING = 9;

    private RiksuSignalEncoder() {}

    static byte[] encode(long sequence, SignalEvent event) {
        ByteArrayOutputStream payload = new ByteArrayOutputStream();
        putU8(payload, TLV_EVENT_KIND, event.kind);
        putU32(payload, TLV_UID, event.uid);

        switch (event.kind) {
            case SignalEvent.KIND_UID_IMPORTANCE ->
                    putU8(payload, TLV_IMPORTANCE, event.value);
            case SignalEvent.KIND_FGS_TYPES ->
                    putU16(payload, TLV_FGS_TYPES, event.value);
            case SignalEvent.KIND_APPOP_ACTIVE -> {
                putU8(payload, TLV_ACTIVE_OP, event.value);
                putBool(payload, TLV_ACTIVE, event.flag);
            }
            case SignalEvent.KIND_COMPANION_PRESENCE ->
                    putBool(payload, TLV_PRESENT, event.flag);
            case SignalEvent.KIND_PACKAGE_ADDED ->
                    putBool(payload, TLV_THIRD_PARTY, event.flag);
            case SignalEvent.KIND_PACKAGE_REMOVED ->
                    putBool(payload, TLV_REPLACING, event.flag);
            default -> throw new IllegalArgumentException("unknown event kind");
        }

        byte[] payloadBytes = payload.toByteArray();
        if (payloadBytes.length > MAX_SIGNAL_PAYLOAD) {
            throw new IllegalArgumentException("signal payload too large");
        }

        ByteBuffer frame = ByteBuffer
                .allocate(RIKSU_HEADER_LEN + payloadBytes.length)
                .order(ByteOrder.LITTLE_ENDIAN);
        frame.put((byte) 'R');
        frame.put((byte) 'K');
        frame.put((byte) 'S');
        frame.put((byte) 'U');
        frame.put((byte) 1);
        frame.put((byte) 0);
        frame.putShort((short) 0x0010);
        frame.putInt(0);
        frame.putInt(payloadBytes.length);
        frame.putLong(sequence);
        frame.putLong(event.monotonicNs);
        frame.put(payloadBytes);
        return frame.array();
    }

    private static void putU8(ByteArrayOutputStream out, int kind, int value) {
        putHeader(out, kind, 1);
        out.write(value & 0xff);
    }

    private static void putBool(ByteArrayOutputStream out, int kind, boolean value) {
        putU8(out, kind, value ? 1 : 0);
    }

    private static void putU16(ByteArrayOutputStream out, int kind, int value) {
        putHeader(out, kind, 2);
        out.write(value & 0xff);
        out.write((value >>> 8) & 0xff);
    }

    private static void putU32(ByteArrayOutputStream out, int kind, int value) {
        putHeader(out, kind, 4);
        out.write(value & 0xff);
        out.write((value >>> 8) & 0xff);
        out.write((value >>> 16) & 0xff);
        out.write((value >>> 24) & 0xff);
    }

    private static void putHeader(ByteArrayOutputStream out, int kind, int length) {
        out.write(kind & 0xff);
        out.write((kind >>> 8) & 0xff);
        out.write(length & 0xff);
        out.write((length >>> 8) & 0xff);
    }
}
