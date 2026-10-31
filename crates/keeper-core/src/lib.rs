pub mod drive;
pub mod explorer;
pub mod lockhunter;
pub mod ops;
pub mod sync;
pub mod taskmgr;
pub mod treesize;
pub mod vhdx;
mod vhdx_workflow;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum KeeperError {
    #[error("I/O 錯誤: {0}")]
    Io(String),
    #[error("路徑不存在或無法存取: {0}")]
    PathNotFound(String),
    #[error("網路磁碟機安全限制: {0}")]
    NetworkDriveRestriction(String),
    #[error("圖片解析錯誤: {0}")]
    ImageError(String),
    #[error("系統操作失敗: {0}")]
    SystemError(String),
    #[error("操作已由使用者取消")]
    OperationCancelled,
}

impl From<std::io::Error> for KeeperError {
    fn from(err: std::io::Error) -> Self {
        KeeperError::Io(err.to_string())
    }
}

pub type KeeperResult<T> = Result<T, KeeperError>;
