# ef80escape

[![Documentation](https://docs.rs/ef80escape/badge.svg)](https://docs.rs/ef80escape)

Lossless conversion between UTF-8 and bytes in Rust. Optimized for UTF-8 content.

Non-UTF-8 bytes (>= 128) are encoded in a subset of Unicode Private Use Area `U+EF80`..`U+EFFF`. Conflicted Unicode characters are escaped by prefixing `U+EF00`.

This can be useful to pass mostly UTF-8 but occasionally invalid UTF-8 data as text-only format like JSON for processing, after receiving the processed text back, reconstruct the original data losslessly.

Unlike [PEP 383](https://peps.python.org/pep-0383), [wtf8](https://docs.rs/wtf8/), [cesu8](https://docs.rs/cesu8), this library produces standard-conformant UTF-8 compatible with Rust's `str`.

The name `ef80escape` is chosen because it's similar to Python's `surrogateescape` but instead of surrogates, it uses a different range starting with `U+EF80`.

Refer to the [documentation](https://docs.rs/ef80escape/latest/ef80escape/fn.bytes_to_str.html) for examples.
