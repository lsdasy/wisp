use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),
    #[error("ошибка базы данных: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("ошибка сериализации: {0}")]
    Json(#[from] serde_json::Error),
    #[error("результат устарел или не найден — повторите поиск")]
    UnknownItem,
    #[error("не удалось открыть: {0}")]
    Open(String),
    #[error("доступ запрещён: {0}")]
    Forbidden(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
