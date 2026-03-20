# rds-core

Shared data types for all crates in the rustdirstat workspace.

## Invariants

- **Zero dependencies beyond `serde`**: rds-core must not depend on anything
  beyond `std` and `serde`. This keeps core types fast to compile and
  independently testable. Enforced by CI via `cargo tree` check.
