use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("文献不存在: {0}")]
    PaperNotFound(String),
    #[error("记录不存在: {0}")]
    NotFound(String),
    #[error("导入数据无效: {0}")]
    InvalidImport(String),
    #[error("备份/恢复失败: {0}")]
    Backup(String),
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP 错误: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
    Msg(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

impl serde::Serialize for CoreError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
