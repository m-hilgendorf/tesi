# `tesi-util`

Utilities for implementing the rest of tesi, and other projects/experiments.

## Motivation

As the project evolves different utilities are helpful. However pulling in 3p dependencies causes
long (clean) compile times, API breakages on updates, and so on. This subcrate provides some data
structures and algorithms that are generally useful for hacking/experimenting with design choices,
as well as standardizing some practices.

## Style/Guidelines

### `unsafe`

Unsafe code is preferable to safe code when the implementing code can uphold the proper memory
safety invariants.

### Indexing with `usize`

All code that acts like array indexing should `impl TryInto<usize>` and cast using `cast_usize!`

```rs
fn get(index: impl TryInto<usize>) {
  let index = util::cast_usize!(index);
}
```

It compiles to a no-op and removes excessive `as` casts or `try_into()`s and `unwraps` throughout the codebase. Callers can now index into the collection using the appropriate type.

### `Array`

`Array` is a wrapper around `Box<[T]>` that elides bounds checks at runtime. Only use this if you guarantee the owner provides all indexing and never trusts indices provided by callers. Don't create [CVES in audio code](https://blog.noahhw.dev/posts/cve-2025-31200/).

