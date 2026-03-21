package io.github.wiltaylor.wcl.core;

public record Diagnostic(String severity, String message, String code) {
    public boolean isError() {
        return "error".equals(severity);
    }

    @Override
    public String toString() {
        if (code != null) {
            return "[" + code + "] " + severity + ": " + message;
        }
        return severity + ": " + message;
    }
}
