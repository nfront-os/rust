//! 9front (Plan 9): times of stat(5), in seconds; set through std (wstat).
use crate::FileTime;
use std::fs;
use std::io;
use std::path::Path;

pub fn set_symlink_file_times(p: &Path, atime: FileTime, mtime: FileTime) -> io::Result<()> {
    // no symbolic links: the file itself
    open(p)?.set_times(fs::FileTimes::new().set_accessed(atime.into()).set_modified(mtime.into()))
}

pub fn from_last_modification_time(meta: &fs::Metadata) -> FileTime {
    FileTime::from_system_time(meta.modified().unwrap_or(std::time::UNIX_EPOCH))
}

pub fn from_last_access_time(meta: &fs::Metadata) -> FileTime {
    FileTime::from_system_time(meta.accessed().unwrap_or(std::time::UNIX_EPOCH))
}

pub fn from_creation_time(_meta: &fs::Metadata) -> Option<FileTime> {
    None
}

pub fn open(path: &Path) -> io::Result<fs::File> {
    fs::File::open(path)
}
