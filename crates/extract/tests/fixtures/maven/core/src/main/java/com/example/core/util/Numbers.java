package com.example.core.util;

/** Numbers, with no contract yet. */
public final class Numbers {
    private Numbers() {}

    public static int clamp(int value, int low, int high) {
        return Math.max(low, Math.min(high, value));
    }
}
