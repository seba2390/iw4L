use std::sync::OnceLock;

use super::{Cursor, DecodeError, Result};

#[derive(Clone, Copy)]
pub(super) struct Word {
    boundary: u32,
    width: u8,
    pub operation: u8,
    pub zeros: u16,
    pub value: i16,
}

pub(super) struct Book(Vec<Word>);

impl Book {
    pub fn read(&self, input: &mut Cursor<'_>) -> Result<Word> {
        let key = input.preview(24);
        let index = self.0.partition_point(|word| word.boundary <= key);
        let word = *self
            .0
            .get(index.wrapping_sub(1))
            .ok_or_else(|| DecodeError::new("entropy prefix"))?;
        input.take(word.width as usize)?;
        Ok(word)
    }
}

pub(super) struct Format {
    pub books: Vec<Book>,
    bands: Vec<(u32, usize, Vec<usize>)>,
}

impl Format {
    pub fn bands(&self, rate: u32, size: usize) -> Result<&[usize]> {
        self.bands
            .iter()
            .find(|(r, n, _)| *r == rate && *n == size)
            .map(|(_, _, bands)| bands.as_slice())
            .ok_or_else(|| DecodeError::new("unsupported exponent grid"))
    }
}

pub(super) fn format() -> &'static Format {
    static FORMAT: OnceLock<Format> = OnceLock::new();
    FORMAT.get_or_init(|| {
        let books = super::table::BOOKS
            .iter()
            .map(|book| {
                Book(
                    book.iter()
                        .map(|&(boundary, width, operation, zeros, value)| Word {
                            boundary,
                            width,
                            operation,
                            zeros,
                            value,
                        })
                        .collect(),
                )
            })
            .collect();
        let bands = super::table::BANDS
            .iter()
            .map(|&(rate, size, widths)| (rate, size, widths.to_vec()))
            .collect();
        Format { books, bands }
    })
}
