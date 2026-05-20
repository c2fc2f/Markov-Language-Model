# Markov Language Model

A command-line tool written in Rust that generates random text based on the statistical properties of a given source text. It implements an n-gram Markov chain using sliding word windows to predict and produce new word sequences.

The implementation is faster than equivalent C implementations, benefiting from Rust's zero-cost abstractions, `hashbrown`'s Swiss-table hash map, and `SmallVec`-backed successor lists that avoid heap allocation for the common case.

## Overview

The tool reads a source text, builds a frequency table mapping every n-gram of consecutive words to the set of words that follow it in the original text, then walks that table at random to emit new text. Increasing the order makes the output more faithful to the source; lowering it produces more creative (and chaotic) results.

The pipeline is:

1. Stream words one by one from stdin or a file, interning each unique byte sequence into an `IndexSet`.
2. Build a `HashMap` from sliding n-gram windows (stored as compact `u32` indices) to their observed successors.
3. Seed a `StdRng`, pick a start word from the empty-key entry, and walk the table until the word limit is reached or a dead end is hit.

## Requirements

- Rust toolchain (edition 2024, stable)
- Or: Nix with flakes enabled

## Installation

### From source

```bash
git clone https://github.com/c2fc2f/Markov-Language-Model
cd Markov-Language-Model
cargo build --release
```

The compiled binary will be at `target/release/mlm`.

### With Nix

A Nix flake is provided:

```bash
nix run github:c2fc2f/Markov-Language-Model -- --help
# or
nix build
# or, to enter a development shell:
nix develop
```

## Usage

```
mlm [OPTIONS] <FILE>
```

Pass `-` as `<FILE>` to read from standard input.

| Flag | Short | Description | Default |
|---|---|---|---|
| `--seed <N>` | `-s` | Seed for the pseudorandom number generator | `1` |
| `--limit <N>` | `-l` | Maximum number of words to generate | `100` |
| `--order <N>` | `-o` | Markov order (length of the look-back window) | `1` |
| `--size <N>` | `-S` | Maximum byte length of a single word | `63` |
| `--keep-end-of-lines` | `-L` | Preserve newline characters that follow words | off |
| `--width <N>` | `-W` | Output column width in bytes; `0` disables wrapping | `0` |
| `--table` | `-t` | Print the raw Markov table instead of generating text | off |
| `--word` | `-w` | Print the list of unique words from the source, one per line | off |

### Examples

Generate 100 words from a text file using a first-order chain:

```bash
mlm corpus.txt
```

Use a third-order chain for output that more closely mirrors the source, generating 200 words:

```bash
mlm --order 3 --limit 200 corpus.txt
```

Pipe text directly from another command:

```bash
cat corpus.txt | mlm -
```

Generate reproducible output by fixing the seed:

```bash
mlm --seed 42 corpus.txt
```

Wrap output at 80 columns:

```bash
mlm --width 80 corpus.txt
```

Inspect the raw transition table — useful for understanding what the model has learned:

```bash
mlm --table corpus.txt
```

List all unique words seen in the source:

```bash
mlm --word corpus.txt
```

Preserve line structure (useful when line breaks are meaningful, e.g. poetry):

```bash
mlm --keep-end-of-lines --order 2 poem.txt
```

## Performance

`mlm` outperforms equivalent C implementations on the same workload. This is primarily due to:

- **`hashbrown`** — a Rust port of Abseil's Swiss table, which provides faster lookup and insertion than a typical C `hash_map` or `uthash`-style table.
- **`SmallVec<[u32; 2]>`** — successor lists store their first two entries inline, avoiding a heap allocation for the overwhelmingly common case where a given n-gram is followed by only one or two distinct words.
- **Word interning** — words are stored once in an `IndexSet` and referenced everywhere else by a `u32` index, keeping both the table keys and successor lists compact and cache-friendly.
- **Buffer reuse** — `read_word_buf` reuses a single `Vec<u8>` across every word read, eliminating per-word allocation during the table-building phase.

## Implementation Notes

Words are stored internally as raw byte sequences (`Vec<u8>`) rather than `String`. This avoids UTF-8 validation overhead on every word read and allows the tool to handle arbitrary byte-level input without error.

The empty key (`[]`) in the transition table serves as the set of valid starting words: the first `order` words of the source text are recorded there, and generation always begins by sampling from that entry.

Successor lists intentionally contain duplicates: a word that follows a given key *n* times in the source appears *n* times in the list, so random sampling naturally reproduces the original frequency distribution without any separate weight bookkeeping.

## License

This project is licensed under the [MIT License](LICENSE).
