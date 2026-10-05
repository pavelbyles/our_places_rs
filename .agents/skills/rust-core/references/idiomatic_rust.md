# Rust Core: Idiomatic Patterns & Philosophy

## Core Philosophy

1.  **Safety First**: `unsafe` is forbidden unless the user explicitly requests it and provides a rationale. Even then, you must wrap it in a `// SAFETY:` comment.
2.  **Expression-Oriented**: Rust is an expression language. Use this.
    - _Bad_: `let mut x = 0; if condition { x = 1; } else { x = 2; }`
    - _Good_: `let x = if condition { 1 } else { 2 };`
3.  **Type-Driven Design**: Make invalid states unrepresentable. Use `enum`s to encode state machines.

## Idiomatic Patterns

### Error Handling

- **Libraries**: Use `thiserror`.
  ```rust
  #[derive(thiserror::Error, Debug)]
  pub enum MyError {
      #[error("IO failed: {0}")]
      Io(#[from] std::io::Error),
      #[error("Invalid data: {0}")]
      InvalidData(String),
  }
  ```
- **Applications**: Use `anyhow::Result`.
  ```rust
  fn run() -> anyhow::Result<()> {
      let content = std::fs::read_to_string("config.ron")?;
      Ok(())
  }
  ```

### Iterators vs Loops

- Prefer `Iterator` combinators for transformation and filtering.
- _Bad_:
  ```rust
  let mut results = Vec::new();
  for item in items {
      if item.is_valid() {
          results.push(item.process());
      }
  }
  ```
- _Good_:
  ```rust
  let results: Vec<_> = items.iter()
      .filter(|i| i.is_valid())
      .map(|i| i.process())
      .collect();
  ```

### Option & Result Combinators (Monadic Chaining)

- **Monadic Railway Pattern**: Prefer `.map()`, `.and_then()`, `.map_err()`, `.or_else()`, `.transpose()`, `.inspect()`, and `?` over nested imperative branching.
- **Avoid Imperative Nesting**:
  - _Bad (Nested Match Pyramid)_:
    ```rust
    if let Some(user) = fetch_user(id) {
        if let Some(profile) = user.get_profile() {
            if let Ok(data) = profile.load_data() {
                // ...
            }
        }
    }
    ```
  - _Good (Monadic Composition)_:
    ```rust
    let data = fetch_user(id)
        .and_then(|u| u.get_profile())
        .ok_or(MyError::NotFound)?
        .load_data()?;
    ```

- **Monadic Error Mapping**: Use `.map_err(MyError::from)` or `thiserror` `#[from]` for seamless monadic error propagation across layer boundaries.

## Project Strictness

- **Async/Await**: Use `tokio` as the default runtime.
- **Formatting**: Strictly adhere to `rustfmt`. Code must pass `cargo fmt --check`.
- **Modules**: Keep `main.rs` small. Move logic to `lib.rs` or submodules (`src/my_module/mod.rs` or `src/my_module.rs`).
- **Visibility**: All fields in structs are private by default. Use `pub(crate)` for internal sharing, `pub` only for API surface.