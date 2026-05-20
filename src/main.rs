//! Generates random text based on the statistical properties of a given
//! source text. It implements an n-gram Markov Language Model using n-uplets
//! to predict word sequences

use std::{
    io::{BufRead, BufReader, Stdout, StdoutLock, Write},
    num::NonZero,
    process::ExitCode,
};

use ahash::RandomState;
use clap::Parser;
use clap_stdin::FileOrStdin;
use hashbrown::HashMap;
use indexmap::IndexSet;
use rand::{Rng, SeedableRng, rngs::StdRng, seq::IndexedRandom};
use smallvec::SmallVec;

/// Generates random text based on the statistical properties of a given
/// source text. It implements an n-gram Markov Language Model using n-uplets
/// to predict word sequences
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Set the seed of the pseudorandom number generator
    #[arg(short, long, default_value_t = 1)]
    seed: u64,

    /// Hides the normal display and replaces it with the table display
    #[arg(short, long, action)]
    table: bool,

    /// Hides the normal display and replaces it with a list of words from the
    /// source text. One word is displayed per line.
    #[arg(short, long, action)]
    word: bool,

    /// Sets the limit on the number of words generated
    #[arg(short = 'l', long, default_value_t = 100)]
    limit: usize,

    /// Sets the order for the generation
    #[arg(short, long, default_value_t = NonZero::new(1).unwrap())]
    order: NonZero<usize>,

    /// Sets the maximum length of prefixes
    #[arg(short = 'S', long, default_value_t = NonZero::new(63).unwrap())]
    size: NonZero<usize>,

    /// When reading words, preserve any line-end characters that immediately
    /// follow them
    #[arg(short = 'L', long = "keep-end-of-lines", action)]
    keep_eol: bool,

    /// Sets the display width, adding right-padding and left-padding as
    /// needed. The width 0 means there is no width limit
    #[arg(short = 'W', long, default_value_t = 0)]
    width: usize,

    /// Source text or standard input with "-"
    #[arg(value_name = "FILE")]
    file: FileOrStdin,
}

/// A word represented as a sequence of raw bytes.
/// Using bytes instead of `String` avoids UTF-8 validation overhead
/// and allows handling arbitrary binary input.
type Word = Vec<u8>;

fn main() -> ExitCode {
    let args: Args = Args::parse();

    let mut reader: BufReader<_> = match args.file.into_reader() {
        Ok(r) => BufReader::new(r),
        Err(e) => {
            eprintln!("*** Error during opening of the stream\n{e}");
            return ExitCode::FAILURE;
        }
    };

    let (words, table) = match markov_table(
        args.order,
        args.keep_eol,
        args.limit,
        &mut reader,
    ) {
        Ok(wt) => wt,
        Err(e) => {
            eprintln!("*** Error during creation of the Markov Table\n{e}");
            return ExitCode::FAILURE;
        }
    };

    let stdout: Stdout = std::io::stdout();
    let mut handle: StdoutLock = stdout.lock();

    if args.table {
        for (key, value) in &table {
            let _ = handle
                .write_all(b"\t".repeat(args.order.get() - key.len()).as_ref());
            for &word in key {
                print_replacing_newlines(&words[word as usize], &mut handle);
                let _ = handle.write_all(b"\t");
            }
            let mut iter = value.iter();
            if let Some(&word) = iter.next() {
                print_replacing_newlines(&words[word as usize], &mut handle);
                for &word in iter {
                    let _ = handle.write_all(b"\t");
                    print_replacing_newlines(
                        &words[word as usize],
                        &mut handle,
                    );
                }
            }
        }

        return ExitCode::SUCCESS;
    }

    if args.word {
        let mut iter = words.iter();
        if let Some(word) = iter.next() {
            print_replacing_newlines(word, &mut handle);
            for word in iter {
                let _ = handle.write_all(b"\n");
                print_replacing_newlines(word, &mut handle);
            }
        }
    }

    let mut rng: StdRng = StdRng::seed_from_u64(args.seed);

    if let Err(e) = markov_generate(
        &words,
        &table,
        args.limit,
        args.order,
        args.width,
        &mut rng,
        &mut handle,
    ) {
        eprintln!("*** Error during generation of the text\n{e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// Prints a byte slice, replacing '\n' with 'L' on the fly.
fn print_replacing_newlines(bytes: &[u8], stream: &mut impl Write) {
    for &byte in bytes {
        let out_byte = if byte == b'\n' { b'L' } else { byte };
        let _ = stream.write_all(&[out_byte]);
    }

    let _ = stream.flush();
}

/// Reads the next whitespace-delimited word from a byte stream.
///
/// Skips any leading ASCII whitespace, then reads bytes until one of the
/// following conditions is met:
/// - A whitespace character is encountered (delimiter, not included in the
///   word).
/// - A newline (`\n`) is encountered **and** `keep_eol` is `true`
///   (the newline is included and acts as a word terminator).
/// - The word reaches `limit` bytes.
/// - The stream is exhausted.
///
/// # Parameters
///
/// - `reader`: Buffered reader over the input byte stream. Its internal
///   position is advanced in place, so the caller retains positional state
///   across successive calls.
/// - `keep_eol`: If `true`, newlines are treated as word-ending tokens and
///   appended to the returned word. Useful for preserving line structure
///   in Markov chains.
/// - `limit`: Maximum number of bytes a word can contain. Words exceeding
///   this length are silently truncated.
/// - `word`: Output buffer that is cleared then filled with the bytes of the
///   next word. Reusing a single buffer across calls avoids repeated
///   allocation.
///
/// # Returns
///
/// - `Ok(true)` — a non-empty word was successfully read into `word`.
/// - `Ok(false)` — the stream is exhausted (EOF reached before any
///   non-whitespace byte).
/// - `Err(e)` — an I/O error occurred while reading.
fn read_word_buf(
    reader: &mut impl BufRead,
    keep_eol: bool,
    limit: usize,
    word: &mut Word,
) -> std::io::Result<bool> {
    word.clear();

    loop {
        let buf: &[u8] = reader.fill_buf()?;
        let len: usize = buf.len();
        if len == 0 {
            return Ok(false);
        }
        let skip: usize =
            buf.iter().take_while(|&&b| b.is_ascii_whitespace()).count();
        let _ = buf;
        reader.consume(skip);
        if skip < len {
            break;
        }
    }

    loop {
        let buf: &[u8] = reader.fill_buf()?;
        let len: usize = buf.len();
        if len == 0 {
            break;
        }

        let mut consumed: usize = len;
        for (i, &b) in buf.iter().enumerate() {
            if word.len() >= limit {
                consumed = i;
                break;
            }
            if b.is_ascii_whitespace() {
                consumed = i + 1;
                if keep_eol && b == b'\n' {
                    word.push(b'\n');
                }
                break;
            }
            word.push(b);
        }
        let _ = buf;
        reader.consume(consumed);
        if consumed < len || word.len() >= limit {
            break;
        }
    }

    Ok(!word.is_empty())
}

/// Builds a Markov chain frequency table from a text stream.
///
/// Reads words one by one using [`read_word_buf`] and maps every sliding window
/// of `order` consecutive words (the *key*) to the list of words that
/// directly follow it (the *continuations*). The first `order` words of the
/// stream are recorded under the empty-key entry, which serves as the set of
/// valid starting words during generation.
///
/// Words are interned in an [`IndexSet`] to deduplicate identical byte
/// sequences and allow referencing them by a compact `u32` index throughout
/// the table.
///
/// # Parameters
///
/// - `order`: Length of the look-back window (Markov order). A higher order
///   produces output that more faithfully mirrors the source text, at the
///   cost of variety.
/// - `keep_eol`: Forwarded to [`read_word_buf`]; preserves newline tokens as
///   distinct words when `true`.
/// - `limit`: Maximum byte length of a single word, forwarded to [`read_word_buf`].
/// - `reader`: Buffered reader over the input text.
///
/// # Returns
///
/// A pair `(words, table)` wrapped in [`std::io::Result`]:
///
/// - `words` — the interning set of all unique [`Word`]s encountered.
/// - `table` — the Markov table mapping each `order`-gram key to its list
///   of observed successors. Successor lists may contain duplicates: a word
///   that follows a given key *n* times appears *n* times, which naturally
///   weights the random selection during text generation.
///
/// # Errors
///
/// Propagates any [`std::io::Error`] returned by the underlying reader.
#[allow(clippy::type_complexity)]
fn markov_table(
    order: NonZero<usize>,
    keep_eol: bool,
    limit: usize,
    reader: &mut impl BufRead,
) -> std::io::Result<(
    IndexSet<Word, RandomState>,
    HashMap<Box<[u32]>, SmallVec<[u32; 2]>, RandomState>,
)> {
    use hashbrown::hash_map::RawEntryMut;

    let order: usize = order.get();
    let mut window: Vec<u32> = Vec::with_capacity(order);
    let mut result: HashMap<Box<[u32]>, SmallVec<[u32; 2]>, RandomState> =
        HashMap::with_hasher(RandomState::new());
    let mut words: IndexSet<Word, RandomState> =
        IndexSet::with_hasher(RandomState::new());
    let mut word_buf: Word = Vec::new();

    loop {
        if !read_word_buf(reader, keep_eol, limit, &mut word_buf)? {
            return Ok((words, result));
        }

        let word_id: u32 = match words.get_index_of(word_buf.as_slice()) {
            Some(id) => id as u32,
            None => {
                let (id, _) = words.insert_full(word_buf.clone());
                id as u32
            }
        };

        match result.raw_entry_mut().from_key(window.as_slice()) {
            RawEntryMut::Occupied(mut e) => e.get_mut().push(word_id),
            RawEntryMut::Vacant(e) => {
                e.insert(
                    window.as_slice().into(),
                    SmallVec::from_buf_and_len([word_id, 0], 1),
                );
            }
        }

        if window.len() == order {
            window.rotate_left(1);
            *window.last_mut().unwrap() = word_id;
        } else {
            window.push(word_id);
        }
    }
}

/// Generates a pseudo-random text sequence from a pre-built Markov table.
///
/// Starting from the empty-key entry (which holds the valid opening words),
/// the function repeatedly looks up the current `order`-gram key in `table`,
/// picks a random successor from the candidate list, writes it to `stream`,
/// then slides the key forward by one word. Generation stops as soon as
/// `limit` words have been emitted, or earlier if the current key has no
/// entry in the table (i.e. a dead end was reached).
///
/// # Parameters
///
/// - `words`: Interning set of all unique [`Word`]s, used to resolve `u32`
///   indices stored in `table` back to their byte content before writing.
/// - `table`: Markov frequency table produced by [`markov_table`].
/// - `limit`: Maximum number of words to emit.
/// - `order`: Look-back window size; must match the order used to build
///   `table`.
///   A mismatch will cause an immediate return after the very first word,
///   as no `order`-length key will ever be found.
/// - `width`: Display width in bytes. When non-zero, right-padding spaces are
///   inserted so that a word which would overflow the current column is pushed
///   to the next line instead. `0` disables line wrapping entirely.
/// - `rng`: Random number generator used for successor selection.
/// - `stream`: Output sink; each word is followed by a single ASCII space.
///
/// # Returns
///
/// - `Ok(())` once `limit` words have been written, or upon reaching a dead
///   end.
/// - `Err(e)` if an I/O error occurs while writing to `stream`.
///
/// # Panics
///
/// Panics if a key maps to an empty successor list, which should never occur
/// when `table` was produced by [`markov_table`] (every key is inserted
/// alongside at least one successor).
fn markov_generate(
    words: &IndexSet<Word, RandomState>,
    table: &HashMap<Box<[u32]>, SmallVec<[u32; 2]>, RandomState>,
    limit: usize,
    order: NonZero<usize>,
    width: usize,
    rng: &mut impl Rng,
    stream: &mut impl Write,
) -> std::io::Result<()> {
    let mut nb: usize = 0;
    let order: usize = order.get();
    let mut curr: usize = 0;

    let mut window: Vec<u32> = Vec::with_capacity(order);

    while nb < limit {
        let Some(values) = table.get(window.as_slice()) else {
            return Ok(());
        };
        let word_id: u32 = *values
            .choose(rng)
            .expect("The List of words cannot be empty");

        let word: &Word = &words[word_id as usize];

        if width > 0 {
            if word.len() < width && curr + word.len() > width {
                stream.write_all(&b" ".repeat(width - curr + 1))?;
                curr = 1;
            }
            curr = (curr + word.len() + 1) % width;
        }

        stream.write_all(word)?;
        stream.write_all(b" ")?;

        if window.len() == order {
            window.rotate_left(1);
            *window.last_mut().unwrap() = word_id;
        } else {
            window.push(word_id);
        }

        nb += 1;
    }

    stream.flush()?;

    Ok(())
}
