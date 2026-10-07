// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_bridge::{AppOpChange, AppOpClass, AppOpMode, AppOpsBackend};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const DEFAULT_CMD_PATH: &str = "/system/bin/cmd";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug)]
pub enum ShellBackendError {
    Spawn(io::Error),
    ExitFailure(Option<i32>),
}

#[derive(Debug, Clone)]
pub struct ShellAppOpsBackend {
    cmd_path: PathBuf,
}

impl Default for ShellAppOpsBackend {
    fn default() -> Self {
        Self::new(DEFAULT_CMD_PATH)
    }
}

impl ShellAppOpsBackend {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            cmd_path: path.into(),
        }
    }

    pub fn command_spec(&self, change: AppOpChange) -> CommandSpec {
        command_spec_for(&self.cmd_path, change)
    }
}

impl AppOpsBackend for ShellAppOpsBackend {
    type Error = ShellBackendError;

    fn set_mode(&mut self, change: AppOpChange) -> Result<(), Self::Error> {
        let spec = self.command_spec(change);
        let status = Command::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(ShellBackendError::Spawn)?;

        if status.success() {
            Ok(())
        } else {
            Err(ShellBackendError::ExitFailure(status.code()))
        }
    }
}

pub fn command_spec_for(cmd_path: &Path, change: AppOpChange) -> CommandSpec {
    CommandSpec {
        program: cmd_path.to_path_buf(),
        args: vec![
            "appops".to_string(),
            "set".to_string(),
            change.uid.to_string(),
            app_op_name(change.op).to_string(),
            mode_code(change.mode).to_string(),
        ],
    }
}

pub const fn app_op_name(op: AppOpClass) -> &'static str {
    match op {
        AppOpClass::Camera => "CAMERA",
        AppOpClass::RecordAudio => "RECORD_AUDIO",
        AppOpClass::FineLocation => "FINE_LOCATION",
        AppOpClass::CoarseLocation => "COARSE_LOCATION",
    }
}

pub const fn mode_code(mode: AppOpMode) -> &'static str {
    match mode {
        AppOpMode::Allowed => "0",
        AppOpMode::Ignored => "1",
        AppOpMode::Foreground => "4",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_camera_uses_uid_mode_without_shell_interpolation() {
        let backend = ShellAppOpsBackend::default();
        let spec = backend.command_spec(AppOpChange {
            uid: 10234,
            op: AppOpClass::Camera,
            mode: AppOpMode::Foreground,
        });

        assert_eq!(spec.program, PathBuf::from("/system/bin/cmd"));
        assert_eq!(spec.args, vec!["appops", "set", "10234", "CAMERA", "4"]);
    }

    #[test]
    fn all_modes_use_aosp_numeric_values() {
        assert_eq!(mode_code(AppOpMode::Allowed), "0");
        assert_eq!(mode_code(AppOpMode::Ignored), "1");
        assert_eq!(mode_code(AppOpMode::Foreground), "4");
    }

    #[test]
    fn operation_names_match_aosp_appops_names() {
        assert_eq!(app_op_name(AppOpClass::Camera), "CAMERA");
        assert_eq!(app_op_name(AppOpClass::RecordAudio), "RECORD_AUDIO");
        assert_eq!(app_op_name(AppOpClass::FineLocation), "FINE_LOCATION");
        assert_eq!(app_op_name(AppOpClass::CoarseLocation), "COARSE_LOCATION");
    }
}
