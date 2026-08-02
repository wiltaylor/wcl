# EARS requirement patterns

House style for `requirement` text: EARS (Easy Approach to Requirements Syntax) phrasing makes
requirements testable and maps them directly onto accept commands and scenario steps. Free
prose is allowed, but reach for a pattern first.


| Pattern | Shape | Use for |
| --- | --- | --- |
| Ubiquitous | THE SYSTEM SHALL \[behaviour\] | Always-true properties |
| Event-driven | WHEN \[event\] THE SYSTEM SHALL \[behaviour\] | Responses to triggers - the workhorse pattern |
| Unwanted behaviour | IF \[failure condition\] THEN THE SYSTEM SHALL \[behaviour\] | Error handling and edge cases |
| State-driven | WHILE \[state\] THE SYSTEM SHALL \[behaviour\] | Mode-dependent behaviour |
| Feature-conditional | WHERE \[feature enabled\] THE SYSTEM SHALL \[behaviour\] | Optional/configurable features |
| Regression (brownfield) | WHEN \[condition\] THE SYSTEM SHALL CONTINUE TO \[existing behaviour\] | Pinning behaviour a change must not break |

A requirement whose SHALL clause names an observable behaviour converts mechanically into an
accept command or walkthrough expect; one that cannot be phrased this way is usually a goal,
not a requirement.


[← Back to SKILL.md](../SKILL.md)
