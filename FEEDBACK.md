# User Feedback Log

## 2026-01-17: AtTraceable vs At<E> guidance

User requested clear guidance on when to use each approach:
- **At<E> wrapper**: Use when you have an existing struct/enum you don't want to modify
- **AtTraceable embedded**: Use when you want traces embedded inside your error type

Added to README.md in the "Design Philosophy" section.
