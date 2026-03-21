package io.github.wiltaylor.wcl.eval;

import java.util.LinkedHashMap;

public record DecoratorValue(String name, LinkedHashMap<String, WclValue> args) {
}
