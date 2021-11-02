use std::path::PathBuf;

use serde::Serialize;

use crate::output::error::{Error, Result};
use crate::proto::ExperimentID;
use crate::proto::SimulationShortID;

use super::RELATIVE_PARTS_FOLDER;

/// Minimum size of a string kept in memory.
/// Corresponds to the minimum size of a non-terminal part (see multipart uploading)
const MAX_BYTE_SIZE: usize = 5242880;
const IN_MEMORY_SIZE: usize = MAX_BYTE_SIZE * 2;

const CHAR_COMMA: u8 = 0x2c; // ,
const CHAR_OPEN_LEFT_SQUARE_BRACKET: u8 = 0x5b; // [
const CHAR_OPEN_RIGHT_SQUARE_BRACKET: u8 = 0x5d; // ]

/// ### Buffer for list of outputs
///
/// Persists in parts onto disk with an in-memory cache layer
pub struct OutputPartBuffer {
    output_type: &'static str,
    current: Vec<u8>,
    pub parts: Vec<PathBuf>,
    base_path: PathBuf,
    initial_step: bool,
}

impl OutputPartBuffer {
    pub fn new(
        output_type_name: &'static str,
        experiment_id: ExperimentID,
        simulation_run_id: SimulationShortID,
    ) -> Result<OutputPartBuffer> {
        let mut base_path = PathBuf::from(RELATIVE_PARTS_FOLDER);
        base_path.push(experiment_id);
        base_path.push(simulation_run_id.to_string());

        std::fs::create_dir_all(&base_path)?;

        // Twice the size so we rarerly exceed it
        let mut current = Vec::with_capacity(IN_MEMORY_SIZE * 2);
        current.push(CHAR_OPEN_LEFT_SQUARE_BRACKET); // New step array

        Ok(OutputPartBuffer {
            output_type: output_type_name,
            current,
            parts: Vec::new(),
            base_path,
            initial_step: true,
        })
    }

    pub fn is_at_capacity(&self) -> bool {
        self.current.len() > IN_MEMORY_SIZE
    }

    pub fn persist_current_on_disk(&mut self) -> Result<()> {
        let mut next_i = self.parts.len();

        let current = std::mem::replace(
            &mut self.current,
            Vec::from(String::with_capacity(IN_MEMORY_SIZE * 2)),
        );

        let part_count = current.len() / MAX_BYTE_SIZE; // Number of parts we can make

        for i in 0..part_count {
            let mut path = self.base_path.clone();
            path.push(format!("{}-{}.part", self.output_type, next_i.to_string()));
            std::fs::File::create(&path)?;

            let contents = if i == part_count - 1 {
                &current[i * MAX_BYTE_SIZE..]
            } else {
                &current[i * MAX_BYTE_SIZE..(i + 1) * MAX_BYTE_SIZE]
            };

            std::fs::write(&path, contents)?;
            self.parts.push(path);
            next_i += 1;
        }

        Ok(())
    }

    pub fn append_step<S: Serialize>(&mut self, step: S) -> Result<()> {
        // TODO OS - COMPILE BLOCK - No finalized field on OutputPartBuffer
        if self.finalized {
            return Err(Error::from("Cannot append to finalized part buffer"));
        }

        if !self.initial_step {
            self.current.push(CHAR_COMMA); // Previous step existed
        } else {
            self.initial_step = false;
        }

        let mut step_vec = serde_json::to_vec(step)?;
        self.current.append(&mut step_vec);

        Ok(())
    }

    pub fn finalize(mut self) -> Result<(Vec<u8>, Vec<PathBuf>)> {
        self.current.push(CHAR_OPEN_RIGHT_SQUARE_BRACKET);
        Ok((self.current, self.parts))
    }
}