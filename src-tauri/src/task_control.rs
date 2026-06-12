use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use thiserror::Error;

#[derive(Default)]
pub struct TaskCancellationRegistry {
    active: Mutex<HashMap<String, ActiveTask>>,
}

#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    status: Arc<Mutex<MeetingTaskStatus>>,
}

struct ActiveTask {
    token: CancellationToken,
    status: Arc<Mutex<MeetingTaskStatus>>,
}

#[derive(Debug, Error)]
pub enum TaskControlError {
    #[error("Task control lock failed: {0}")]
    Lock(String),
    #[error("A task is already running for meeting {0}")]
    AlreadyRunning(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelMeetingTaskResult {
    pub meeting_id: String,
    pub cancel_requested: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingTaskStatus {
    pub meeting_id: String,
    pub kind: String,
    pub phase: String,
    pub message: String,
    pub current: u32,
    pub total: Option<u32>,
    pub percent: Option<u8>,
    pub cancel_requested: bool,
}

impl TaskCancellationRegistry {
    pub fn begin(
        &self,
        meeting_id: &str,
        kind: &str,
    ) -> Result<CancellationToken, TaskControlError> {
        let mut active = self
            .active
            .lock()
            .map_err(|error| TaskControlError::Lock(error.to_string()))?;
        if active
            .get(meeting_id)
            .map(|task| !task.token.is_cancelled())
            .unwrap_or(false)
        {
            return Err(TaskControlError::AlreadyRunning(meeting_id.to_string()));
        }

        let token = CancellationToken::new(meeting_id, kind);
        active.insert(
            meeting_id.to_string(),
            ActiveTask {
                token: token.clone(),
                status: Arc::clone(&token.status),
            },
        );
        Ok(token)
    }

    pub fn cancel(&self, meeting_id: &str) -> Result<bool, TaskControlError> {
        let active = self
            .active
            .lock()
            .map_err(|error| TaskControlError::Lock(error.to_string()))?;
        let Some(task) = active.get(meeting_id) else {
            return Ok(false);
        };
        task.token.cancel();
        Ok(true)
    }

    pub fn get_status(
        &self,
        meeting_id: &str,
    ) -> Result<Option<MeetingTaskStatus>, TaskControlError> {
        let active = self
            .active
            .lock()
            .map_err(|error| TaskControlError::Lock(error.to_string()))?;
        let Some(task) = active.get(meeting_id) else {
            return Ok(None);
        };
        Ok(Some(task.snapshot()?))
    }

    pub fn list_statuses(&self) -> Result<Vec<MeetingTaskStatus>, TaskControlError> {
        let active = self
            .active
            .lock()
            .map_err(|error| TaskControlError::Lock(error.to_string()))?;
        active.values().map(ActiveTask::snapshot).collect()
    }

    pub fn finish(&self, meeting_id: &str) -> Result<(), TaskControlError> {
        let mut active = self
            .active
            .lock()
            .map_err(|error| TaskControlError::Lock(error.to_string()))?;
        active.remove(meeting_id);
        Ok(())
    }
}

impl CancellationToken {
    fn new(meeting_id: &str, kind: &str) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(MeetingTaskStatus {
                meeting_id: meeting_id.to_string(),
                kind: kind.to_string(),
                phase: "starting".to_string(),
                message: "Starting task".to_string(),
                current: 0,
                total: None,
                percent: None,
                cancel_requested: false,
            })),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Ok(mut status) = self.status.lock() {
            status.cancel_requested = true;
            status.phase = "canceling".to_string();
            status.message = "Stopping after the current work item".to_string();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn update_progress(
        &self,
        phase: &str,
        message: &str,
        current: u32,
        total: Option<u32>,
    ) -> Result<(), TaskControlError> {
        let mut status = self.lock_status()?;
        status.phase = phase.to_string();
        status.message = message.to_string();
        status.current = current;
        status.total = total;
        status.percent = total.and_then(|total| {
            if total == 0 {
                None
            } else {
                Some(((current.min(total) * 100) / total).min(100) as u8)
            }
        });
        status.cancel_requested = self.is_cancelled();
        Ok(())
    }

    fn lock_status(&self) -> Result<MutexGuard<'_, MeetingTaskStatus>, TaskControlError> {
        self.status
            .lock()
            .map_err(|error| TaskControlError::Lock(error.to_string()))
    }
}

impl ActiveTask {
    fn snapshot(&self) -> Result<MeetingTaskStatus, TaskControlError> {
        self.status
            .lock()
            .map(|status| status.clone())
            .map_err(|error| TaskControlError::Lock(error.to_string()))
    }
}
