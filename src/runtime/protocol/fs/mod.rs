pub(crate) mod file_protocol_v1;
pub(crate) mod file_protocol_v2;
pub(crate) mod simple_file_system;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum HandleKind {
    Dir,
    File,
}
