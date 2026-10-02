//! Host-owned task metadata and a linear cancellation/commit decision.
//! No executor, UI dependency, global state or persistent job registry.
use crate::{DocumentInfo, ServiceError};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum TaskState {
    Queued,
    Running,
    CancelRequested,
    Committing,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelOutcome {
    Requested,
    AlreadyCancelled,
    TooLate,
}
#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<AtomicU8>);
impl Default for CancellationToken {
    fn default() -> Self {
        Self(Arc::new(AtomicU8::new(TaskState::Queued as u8)))
    }
}
fn task_error(code: &str, message: &str) -> ServiceError {
    ServiceError {
        code: code.into(),
        message: message.into(),
        details: serde_json::json!({}),
    }
}
impl CancellationToken {
    pub fn state(&self) -> TaskState {
        match self.0.load(Ordering::Acquire) {
            0 => TaskState::Queued,
            1 => TaskState::Running,
            2 => TaskState::CancelRequested,
            3 => TaskState::Committing,
            4 => TaskState::Completed,
            6 => TaskState::Cancelled,
            _ => TaskState::Failed,
        }
    }
    pub fn cancel(&self) -> CancelOutcome {
        loop {
            let state = self.state();
            match state {
                TaskState::Queued | TaskState::Running => {
                    if self
                        .0
                        .compare_exchange(
                            state as u8,
                            TaskState::CancelRequested as u8,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        return CancelOutcome::Requested;
                    }
                }
                TaskState::CancelRequested | TaskState::Cancelled => {
                    return CancelOutcome::AlreadyCancelled;
                }
                _ => return CancelOutcome::TooLate,
            }
        }
    }
    pub fn start(&self) -> Result<(), ServiceError> {
        self.0
            .compare_exchange(
                TaskState::Queued as u8,
                TaskState::Running as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| {
                if matches!(
                    self.state(),
                    TaskState::CancelRequested | TaskState::Cancelled
                ) {
                    task_error("CANCELLED", "任务已取消")
                } else {
                    task_error("TASK_STATE", "任务不能重复执行")
                }
            })
    }
    pub fn checkpoint(&self) -> Result<(), ServiceError> {
        if matches!(
            self.state(),
            TaskState::CancelRequested | TaskState::Cancelled
        ) {
            Err(task_error("CANCELLED", "任务已取消"))
        } else {
            Ok(())
        }
    }
    /// Called immediately before an atomic edit, file side effect or read-result installation.
    /// Once this succeeds, cancellation cannot claim that the operation had no effect.
    pub fn begin_commit(&self) -> Result<(), ServiceError> {
        self.0
            .compare_exchange(
                TaskState::Running as u8,
                TaskState::Committing as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| {
                if matches!(
                    self.state(),
                    TaskState::CancelRequested | TaskState::Cancelled
                ) {
                    task_error("CANCELLED", "任务已取消")
                } else {
                    task_error("TASK_STATE", "任务不在可提交状态")
                }
            })
    }
    pub fn finish(&self, success: bool) {
        // A request becomes terminal only after the worker releases its temporaries.
        loop {
            let state = self.state();
            let terminal = match state {
                TaskState::CancelRequested => TaskState::Cancelled,
                TaskState::Running | TaskState::Committing => {
                    if success {
                        TaskState::Completed
                    } else {
                        TaskState::Failed
                    }
                }
                _ => return,
            };
            if self
                .0
                .compare_exchange(
                    state as u8,
                    terminal as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return;
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskVersion {
    pub document_id: Option<String>,
    pub document_revision: Option<String>,
    pub workspace_revision: Option<String>,
    pub generation: u64,
    pub rule_revision: u64,
    pub geometry_policy_hash: String,
}
impl TaskVersion {
    pub fn capture(info: Option<&DocumentInfo>, generation: u64, rule_revision: u64) -> Self {
        Self {
            document_id: info.map(|d| d.document_id.clone()),
            document_revision: info.map(|d| d.revision.clone()),
            workspace_revision: info.map(|d| d.workspace_revision.clone()),
            generation,
            rule_revision,
            geometry_policy_hash: info.map_or_else(String::new, |d| {
                editor_core::hash::sha256_hex(
                    &serde_json::to_vec(&d.manufacturing_precision)
                        .expect("validated finite manufacturing precision"),
                )
            }),
        }
    }
}
#[derive(Clone, Debug)]
pub struct TaskContext {
    pub task_id: u64,
    pub input: TaskVersion,
    pub cancel_token: CancellationToken,
}
impl TaskContext {
    pub fn new(task_id: u64, input: TaskVersion) -> Self {
        Self {
            task_id,
            input,
            cancel_token: CancellationToken::default(),
        }
    }
    pub fn validate(&self, current: &TaskVersion) -> Result<(), ServiceError> {
        self.cancel_token.checkpoint()?;
        if &self.input != current {
            return Err(task_error(
                "STALE_TASK",
                "文档、工作区或策略已变化，旧任务未执行",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskReceipt {
    pub task_id: u64,
    pub input: TaskVersion,
    pub result_version: TaskVersion,
    pub state: TaskState,
}
