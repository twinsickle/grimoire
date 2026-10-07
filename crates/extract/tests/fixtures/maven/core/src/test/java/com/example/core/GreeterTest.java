package com.example.core;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.example.core.util.*;
import org.junit.jupiter.api.Test;

class GreeterTest {
    /**
     * # Test contract
     *
     * ```json
     * {"test": "greets-by-name", "checks": ["core/src/main/java/com/example/core/Greeter#greets"]}
     * ```
     */
    @Test
    void greetsByName() {
        assertEquals("Hello, Ada", new Greeter(1).greet("Ada"));
    }

    // # Test contract
    // ```json
    // {"test": "loud-is-capitals", "checks": ["core/src/main/java/com/example/core/Greeter#loud", "core/src/main/java/com/example/core/util/Strings#shout"]}
    // ```
    @Test
    void loudIsCapitals() {
        assertEquals("HELLO, ADA", Strings.shout(new Greeter(1).greet("Ada")));
    }
}
