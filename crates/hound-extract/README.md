# hound-extract

Rust source extractor used by AllSource Prime Hound. It walks a source tree,
respects ignore rules, and turns Rust definitions and calls into a small,
language-neutral graph representation using Tree-sitter.

Extraction runs on-device and makes no network or LLM calls.

## Status

Version 0.1 supports Rust functions, types, traits, modules, and call
references. AllSource Prime maps this intermediate representation into durable
memory events.

Project: [AllSource Prime](https://all-source.xyz/prime)
