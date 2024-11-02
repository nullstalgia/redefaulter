#[derive(Debug, thiserror::Error)]
pub enum MenuMacroError {
    // #[error("Only bools are implemented currently. Use #[menuid(skip)] to skip fields")]
    // UnimplementedType,
    #[error("The following ID has no associated struct field: {0}")]
    FieldNotFound(String),
}
