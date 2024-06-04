// SPDX-License-Identifier: LGPL-2.1
// Copyright 2021 Daniel Vogelbacher <daniel@chaospixel.com>

use super::Job;
use crate::{AppError, Result};
use async_trait::async_trait;
use log::debug;
use rawler::{
  dng::convert::{convert_raw_file, ConvertParams},
  RawlerError,
};
use std::{
  fmt::Display,
  fs::{remove_file, File},
  io::BufWriter,
};
use std::{path::PathBuf, /*time::Instant*/};
use tokio::task::spawn_blocking;

/// Job for converting RAW to DNG
#[derive(Debug, Clone)]
pub struct Raw2DngJob {
  pub input: PathBuf,
  pub output: PathBuf,
  pub replace: bool,
  pub params: ConvertParams,
}

/// State of conversion
#[derive(Debug)]
pub struct JobResult {
  pub job: Raw2DngJob,
  pub duration: f32,
  pub error: Option<AppError>,
}

impl Display for JobResult {
  /// Pretty print the conversion state
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Some(error) = self.error.as_ref() {
      f.write_fmt(format_args!("Failed: '{}', {}", self.job.input.display(), error))?;
    } else {
      f.write_fmt(format_args!(
        "Converted '{}' => '{}' (in {:.2}s)",
        self.job.input.display(),
        self.job.output.display(),
        self.duration
      ))?;
    }
    Ok(())
  }
}

impl Raw2DngJob {
  fn internal_exec(&self) -> Result<JobResult> {
    if self.output.exists() && !self.replace {
      return Err(AppError::AlreadyExists(self.output.clone()));
    }
    // File name for embedding
    let orig_filename = self
      .input
      .file_name()
      .ok_or(AppError::General("Input has no filename".into()))?
      .to_os_string()
      .to_string_lossy()
      .to_string();

    let mut dng = BufWriter::new(File::create(&self.output)?);

    match convert_raw_file(&self.input, &mut dng, &self.params) {
      Ok(_) => {
        drop(dng);
        Ok(JobResult {
          job: self.clone(),
          duration: 0.0,
          error: None,
        })
      }
      Err(err) => {
        match &err {
          RawlerError::Unsupported { .. } => {
            log::error!("Unsupported file: \"{}\"\n{}", orig_filename, err.to_string());
          }
          RawlerError::DecoderFailed(msg) => {
            log::error!("Failed to decode file: {}", msg);
          }
        }
        drop(dng);
        if let Err(err) = remove_file(&self.output) {
          log::error!("Failed to delete DNG file after decoder error: {:?}", err);
        }
        Err(err.into())
      }
    }
  }
}

#[async_trait]
impl Job for Raw2DngJob {
  type Output = JobResult;

  async fn execute(&self) -> Self::Output {
    debug!("Job running: input: {:?}, output: {:?}", self.input, self.output);
    // let now = Instant::now();
    let cp = self.clone();
    let handle = spawn_blocking(move || cp.internal_exec());
    match handle.await {
      Ok(Ok(mut stat)) => {
        // stat.duration = now.elapsed().as_secs_f32();
        eprintln!("Writing DNG output file: {}", stat.job.output.display());
        stat
      }
      Ok(Err(e)) => JobResult {
        job: self.clone(),
        duration:0.0,
        // duration: now.elapsed().as_secs_f32(),
        error: Some(e),
      },
      Err(e) => JobResult {
        job: self.clone(),
        duration: 0.0,
        // duration: now.elapsed().as_secs_f32(),
        error: Some(AppError::General(format!("Join handle failed: {:?}", e))),
      },
    }
  }
}
