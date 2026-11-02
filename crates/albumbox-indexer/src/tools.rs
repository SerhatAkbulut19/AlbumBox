//! External tools (ffmpeg / ffprobe). Spawned without a console window so the
//! Windows service never flashes windows.

use std::{
    path::PathBuf,
    process::{Command, Output},
};

use anyhow::{bail, Context};

#[derive(Debug, Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Default for Tools {
    fn default() -> Self {
        Self { ffmpeg: "ffmpeg".into(), ffprobe: "ffprobe".into() }
    }
}

pub fn command(program: &PathBuf) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn run(mut cmd: Command) -> anyhow::Result<Output> {
    let out = cmd.output().with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
        bail!("{:?} failed: {tail}", cmd.get_program());
    }
    Ok(out)
}

impl Tools {
    /// True when both tools can be started.
    pub fn available(&self) -> bool {
        [&self.ffmpeg, &self.ffprobe].iter().all(|p| {
            let mut c = command(p);
            c.arg("-version");
            c.output().map(|o| o.status.success()).unwrap_or(false)
        })
    }
}
