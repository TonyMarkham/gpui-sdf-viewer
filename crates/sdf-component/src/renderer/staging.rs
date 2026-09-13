use std::sync::mpsc;
use wgpu::{Buffer, MapRangeError};

use super::staging_state::StagingState;

/// One ring slot: a MAP_READ staging buffer awaiting asynchronous readback.
pub(crate) struct Staging {
    pub(crate) buffer: Buffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bytes_per_row: u32,
    pub(crate) state: StagingState,
    pub(crate) mapped: Option<mpsc::Receiver<std::result::Result<(), wgpu::BufferAsyncError>>>,
    /// Submission order stamp. Ring-slot index is not submission order —
    /// slots are reused as they free — so presentation ordering reads this.
    pub(crate) sequence: u64,
}

impl Staging {
    /// Copies the mapped buffer contents and unmaps. Only valid after the map
    /// callback reported success.
    pub(crate) fn read_mapped(&self) -> std::result::Result<Vec<u8>, MapRangeError> {
        let data = match self.buffer.get_mapped_range(..) {
            Ok(view) => {
                let data = view.to_vec();
                drop(view);
                data
            }
            Err(error) => {
                self.buffer.unmap();
                return Err(error);
            }
        };
        self.buffer.unmap();
        Ok(data)
    }
}
