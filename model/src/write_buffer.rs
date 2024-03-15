// Portal Hardware Wallet model library
// 
// Copyright (c) 2024 Alekos Filini
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use crate::MessageFragment;

pub struct WriteBuffer<const DATA_LEN: usize, const NUM_BUFS: usize, const PREFIX_LEN: usize> {
    _prefix: [u8; PREFIX_LEN],
    buffer: [[u8; DATA_LEN]; NUM_BUFS],
    cursor: usize,
}

impl<const DATA_LEN: usize, const NUM_BUFS: usize, const PREFIX_LEN: usize>
    WriteBuffer<DATA_LEN, NUM_BUFS, PREFIX_LEN>
{
    pub fn append(&mut self, fragment: &MessageFragment) {
        let mut data_iter = fragment.get_filled_data().iter();

        for i in 0usize..NUM_BUFS {
            let left = (DATA_LEN * (i + 1)).saturating_sub(self.cursor);
            for b in data_iter.by_ref().take(left) {
                self.buffer[i][self.cursor % DATA_LEN] = *b;
                self.cursor += 1;
            }

            if self.cursor % DATA_LEN == 0 {
                // Skip the prefix + the address byte
                self.cursor += PREFIX_LEN + 1;
            }
        }
    }

    pub fn get_data(&self) -> impl Iterator<Item = &[u8; DATA_LEN]> {
        // Take as many buffers as necessary plus the last one which is the terminator
        // and always needs to be written to complete the transaction

        let take = self.cursor / DATA_LEN + 1;

        self.buffer
            .iter()
            .enumerate()
            .filter_map(move |(i, b)| match i {
                i if i < take || i == NUM_BUFS - 1 => Some(b),
                _ => None,
            })
    }
}

pub trait WriteBufferInit<const DATA_LEN: usize, const NUM_BUFS: usize, const PREFIX_LEN: usize> {
    fn new() -> WriteBuffer<DATA_LEN, NUM_BUFS, PREFIX_LEN>;

    fn init_fields(
        buffer: [[u8; DATA_LEN]; NUM_BUFS],
    ) -> WriteBuffer<DATA_LEN, NUM_BUFS, PREFIX_LEN> {
        WriteBuffer {
            _prefix: [0; PREFIX_LEN],
            buffer,
            cursor: 1 + PREFIX_LEN,
        }
    }
}
