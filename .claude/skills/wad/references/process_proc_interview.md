# Running the interview

## Purpose

Settle every decision an implementation agent could stumble on - by the user, in writing - before anything else happens.

## Prerequisites

- A plan/ folder that checks clean

## Flowchart

![diagram](../_wdoc/process_proc_interview-diagram-1.svg)

## Steps

### Step 1: Ask in structured rounds

Group related questions; one round at a time. Cover at minimum: platforms/targets, language and toolchain, project name and repo location, licensing, external services, non-goals, and what done means for v1. Include a round that enumerates every surface (screen, CLI command, endpoint) the user expects - the list feeds the PRD's surface definitions.

### Step 2: Record each question when asked

```wcl
question q_platforms {
  question = "What platforms must be supported?"
  status = :answered
  answer = "Linux x86_64 only for v1."
}
```

One `question` block per question in questions.wcl, default :open. Write the user's answer verbatim and set :answered. Questions the user waves off get :dropped plus a why in answer. Never fill in an answer yourself; if the user says you decide, record the question, your decision, and delegated-by-user in the answer.

### Step 2: More rounds?

The phase ends when you can think of no question whose unknown answer could change a spec.

> [!TIP]
> **Verification**
>
> The questions_closed gate passes and the user has confirmed the interview feels complete.

[← Back to SKILL.md](../SKILL.md)
