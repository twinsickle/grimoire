package com.example.core.util;

/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Shape strings for display.","criteria":[{"id":"shout","statement":"Shouting a text gives it in capitals."}]}
 * ```
 */
public final class Strings {
    private Strings() {}

    public static String shout(String text) {
        return text.toUpperCase();
    }

    public static String indent(String text, int width) {
        return " ".repeat(Numbers.clamp(width, 0, 8)) + text;
    }
}
