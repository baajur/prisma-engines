//! This module is responsible for the management of the contents of the
//! migrations folder. The migrations folder contains multiple migration
//! folders, named after the migration id, and each containing:
//!
//! - A migration script

use migration_connector::Migration;
use std::{
    ffi::OsStr,
    fs::{create_dir, read_dir, DirEntry},
    io::{self, Write as _},
    path::{Path, PathBuf},
};

/// The file name for migration scripts, not including the file extension.
pub const MIGRATION_SCRIPT_FILENAME: &str = "migration";

/// Create a folder for a new migration.
pub(crate) fn create_migration_folder(
    migrations_folder_path: &Path,
    migration_name: &str,
) -> io::Result<MigrationFolder> {
    let timestamp = chrono::Utc::now().format("%Y%m%d%M%S");
    let folder_name = format!(
        "{timestamp}_{migration_name}",
        timestamp = timestamp,
        migration_name = migration_name
    );
    let folder_path = migrations_folder_path.join(folder_name);

    if folder_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            anyhow::anyhow!(
                "The migration folder already exists at {}",
                folder_path.to_string_lossy()
            ),
        ));
    }

    create_dir(&folder_path)?;

    Ok(MigrationFolder(folder_path))
}

/// List the migrations present in the migration folder, ordered by increasing timestamp.
pub(crate) fn list_migrations(migrations_folder_path: &Path) -> io::Result<Vec<MigrationFolder>> {
    let mut entries: Vec<MigrationFolder> = Vec::new();

    for entry in read_dir(migrations_folder_path)? {
        let entry = entry?;

        if entry.file_type()?.is_dir() {
            entries.push(entry.into());
        }
    }

    entries.sort_by(|a, b| a.migration_id().cmp(b.migration_id()));

    Ok(entries)
}

/// Proxy to a folder containing one migration, as returned by
/// `create_migration_folder` and `list_migrations`.
#[derive(Debug)]
pub(crate) struct MigrationFolder(PathBuf);

impl MigrationFolder {
    /// The `{timestamp}_{name}` formatted migration id.
    pub(crate) fn migration_id(&self) -> &str {
        self.0
            .file_name()
            .expect("MigrationFolder::migration_id")
            .to_str()
            .expect("Migration folder name is not valid UTF-8.")
    }

    pub(crate) fn matches_applied_migration(&self, applied_migration: &Migration) -> bool {
        applied_migration.name == self.migration_id()
    }

    #[tracing::instrument]
    pub(crate) fn write_migration_script(&self, script: &str, extension: &str) -> std::io::Result<()> {
        let mut path = self.0.join("migration");

        path.set_extension(extension);

        let mut file = std::fs::File::create(&path)?;
        file.write_all(script.as_bytes())?;

        Ok(())
    }

    #[tracing::instrument]
    pub(crate) fn read_migration_script(&self) -> std::io::Result<String> {
        std::fs::read_to_string(&self.0)
    }
}

impl From<DirEntry> for MigrationFolder {
    fn from(entry: DirEntry) -> MigrationFolder {
        MigrationFolder(entry.path())
    }
}
