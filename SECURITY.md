# Security Policy

## Reporting a Vulnerability

A3SQL is a Rust DLL loaded into the Arma 3 game process. A security vulnerability
could affect users' systems. If you find one:

1. **Do not** open a public GitHub issue.
2. Email details to the project maintainer (see git log for contact), or
3. Open a [private advisory](https://github.com/lErrorl404l/a3sql/security/advisories/new).

We'll acknowledge receipt within 48 hours and aim for a fix within 7 days.

## Scope

The following areas are in-scope:

- SQL injection via the `$1`/`$2` parameter substitution mechanism
- TCP listener authentication bypass
- Buffer overflow / memory safety in the Rust layer
- Unsafe file operations (save/load paths escaping the mod directory)

The following are NOT in-scope:

- Arma 3 engine vulnerabilities (report to Bohemia Interactive instead)
- Mods using A3SQL that pass unsanitized user input (that's the mod's responsibility)
