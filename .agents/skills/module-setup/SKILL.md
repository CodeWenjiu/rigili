---
name: module-setup
description: Create or reorganize Rust modules following the project's module declaration convention. Use when adding a new .rs file, moving files, or fixing bare mod violations.
---

## Module Declaration Constitution

Every crate declares its modules through the project's module-declaration macros.
Manual `mod` / `pub mod` / `pub use` for module plumbing is forbidden.

- Pick the macro for the module's shape (same-directory file, sub-directory
  module, or a file that also needs path access) and visibility. The macro crate
  is the source of truth for names, arguments, and expansions.
- Inline modules such as `mod tests { ... }` are exempt.
- Only the macro crate itself may use bare `mod` for bootstrap; keep that
  exception minimal and explicit.
- Control exported symbols with visibility inside the module; avoid redundant
  re-exports.

When adding or moving a `.rs` file, update the owning module declaration in the
same change. If the macro API is unclear, read the macro crate source instead of
guessing.
