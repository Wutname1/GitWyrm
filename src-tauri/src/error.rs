use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
  #[error("git error: {0}")]
  Git(#[from] git2::Error),
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),
  #[error("{0}")]
  Other(String),
}

impl Serialize for AppError {
  fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    let message = self.to_string();
    // Tauri serializes command errors before returning them to the UI. Logging
    // at this boundary guarantees that an error shown to the user also has a
    // durable entry in gitwyrm.log.
    log::error!("Command failed: {message}");
    serializer.serialize_str(&message)
  }
}

impl specta::Type for AppError {
  fn inline(_: &mut specta::TypeCollection, _: specta::Generics) -> specta::datatype::DataType {
    specta::datatype::DataType::Primitive(specta::datatype::PrimitiveType::String)
  }
}
