package io.github.wiltaylor.wcl;

public class WclException extends RuntimeException {
    public WclException(String message) {
        super(message);
    }

    public WclException(String message, Throwable cause) {
        super(message, cause);
    }
}
