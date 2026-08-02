# @block

`@block("kind")` makes a type a nestable block of a named kind. The kind string is the keyword
you write to author an instance, so `@block("server")` lets you write `server web { ... }`.


```wcl
@block("server") type Server {
  @inline(0) name: identifier
  port: u16
}

server web {
  port = 8080
}
```

[← Back to SKILL.md](../SKILL.md)
