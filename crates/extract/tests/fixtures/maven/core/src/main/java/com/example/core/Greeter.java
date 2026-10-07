/*
 * Licensed under the Apache License, Version 2.0.
 */
package com.example.core;

import java.util.List;

import com.example.core.util.Strings;
import org.osgi.service.component.annotations.Component;

import static com.example.core.util.Numbers.clamp;

/**
 * Greets people by name.
 *
 * # Module contract
 *
 * ```json
 * {"purpose":"Greet people by name.","criteria":[{"id":"greets","statement":"A greeting names the person greeted."},{"id":"loud","statement":"A loud greeting is in capitals."}]}
 * ```
 */
@Component(service = Greeter.class)
public class Greeter {
    private final int volume;

    public Greeter(int volume) {
        this.volume = clamp(volume, 0, 10);
    }

    public String greet(String name) {
        return "Hello, " + name;
    }

    public List<String> greetAll(List<String> names, boolean loud) {
        return names.stream().map(n -> loud ? Strings.shout(greet(n)) : greet(n)).toList();
    }

    public enum Tone {
        SOFT,
        LOUD
    }
}
