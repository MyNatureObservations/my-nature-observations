//! C5a TEST ONLY. File/publication observations never grant production Ready.
use crate::database;
use rusqlite::{Connection, OpenFlags};
use std::{fs, path::Path};

fn create_synthetic(path: &Path) -> Result<(), String> {
    database::create_current_database(path)?;
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    let result=db.execute("INSERT INTO journal_entries VALUES ('c5a','2026-11-04','Synthetic C5a','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\",\"content\":[{\"type\":\"text\",\"text\":\"C5a\"}]}]}',1,'created','updated')",[]).map_err(|e|e.to_string());
    db.close().map_err(|(_, e)| e.to_string())?;
    result.map(|_| ())
}
fn verify_existing(path: &Path) -> Result<(), String> {
    database::initialize_database(path)?; // Current production read-only schema/content validation.
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let value: String = db
            .query_row(
                "SELECT title FROM journal_entries WHERE id='c5a'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let integrity: String = db
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if value != "Synthetic C5a" || integrity != "ok" {
            return Err("invalid synthetic content".into());
        }
        Ok(())
    })();
    db.close().map_err(|(_, e)| e.to_string())?;
    result
}
#[test]
fn current_schema_constructor_is_no_clobber_and_independently_reopenable() {
    let parent = tempfile::tempdir().unwrap();
    let root = super::restore::private::PrivateDir::new(parent.path()).unwrap();
    let path = root.path().join("synthetic.sqlite");
    create_synthetic(&path).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(create_synthetic(&path).is_err());
    for _ in 0..3 {
        verify_existing(&path).unwrap();
    }
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        io::{self, Write},
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        },
        time::{Duration, Instant},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, GetVolumeInformationByHandleW, MOVEFILE_WRITE_THROUGH,
        MoveFileExW, SetFileAttributesW,
    };
    const MARKER: &str = "probe-intent.json";
    const CONTENT: &[u8] = b"{\"format\":\"mno-c5a-initialization-probe\",\"version\":1}\n";
    #[derive(Debug, PartialEq, Eq)]
    enum Observation {
        FilesVerifiedDirectoryPersistenceUnestablished,
    }
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    fn plain(path: &Path, directory: bool) -> io::Result<()> {
        let m = fs::symlink_metadata(path)?;
        if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (directory && !m.is_dir())
            || (!directory && !m.is_file())
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Ok(())
    }
    fn private_root() -> (
        tempfile::TempDir,
        super::super::restore::private::PrivateDir,
    ) {
        let parent = tempfile::tempdir().unwrap();
        let root = super::super::restore::private::PrivateDir::new(parent.path()).unwrap();
        assert_ntfs(root.path());
        (parent, root)
    }
    fn assert_ntfs(root: &Path) {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(root)
            .unwrap();
        let mut name = [0u16; 32];
        assert_ne!(
            unsafe {
                GetVolumeInformationByHandleW(
                    directory.as_raw_handle(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    name.as_mut_ptr(),
                    name.len() as u32,
                )
            },
            0
        );
        let end = name.iter().position(|c| *c == 0).unwrap();
        assert_eq!(
            String::from_utf16(&name[..end]).unwrap(),
            "NTFS",
            "probe acceptance scope is local NTFS"
        );
    }
    fn flush(path: &Path) -> io::Result<()> {
        plain(path, false)?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?
            .sync_all()
    }
    fn publish(
        root: &Path,
        hook: &mut impl FnMut(&str, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        plain(root, true)?;
        let mut temporary = tempfile::Builder::new().prefix(".c5a-").tempfile_in(root)?;
        hook("before-marker-write", temporary.path())?;
        temporary.write_all(CONTENT)?;
        temporary.flush()?;
        hook("before-marker-flush", temporary.path())?;
        temporary.as_file().sync_all()?;
        // Close the marker handle before renaming. TempPath owns only our source
        // name; failure cleans that temporary, never the destination/evidence.
        let source = temporary.into_temp_path();
        let from = wide(&source);
        let to = wide(&root.join(MARKER));
        if unsafe { SetFileAttributesW(from.as_ptr(), FILE_ATTRIBUTE_NORMAL) } == 0 {
            return Err(io::Error::last_os_error());
        }
        hook("before-publication", &source)?;
        // No REPLACE_EXISTING, COPY_ALLOWED, delete fallback, or cross-volume copy.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(io::Error::last_os_error());
        }
        hook("after-publication", &root.join(MARKER))?;
        flush(&root.join(MARKER))?;
        Ok(())
    }
    fn initialize(
        root: &Path,
        hook: &mut impl FnMut(&str, &Path) -> io::Result<()>,
    ) -> Result<Observation, String> {
        plain(root, true).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
            let name = entry.map_err(|e| e.to_string())?.file_name();
            let name = name.to_string_lossy();
            if name == MARKER || name.starts_with(".c5a-") || name.starts_with("mno.sqlite-") {
                return Err("previous initialization evidence; never retry automatically".into());
            }
        }
        let path = root.join("mno.sqlite");
        if path.try_exists().map_err(|e| e.to_string())? {
            return Err("existing database; never overwrite".into());
        }
        publish(root, hook).map_err(|e| e.to_string())?;
        hook("before-database", &path).map_err(|e| e.to_string())?;
        create_synthetic(&path)?;
        hook("after-schema-close", &path).map_err(|e| e.to_string())?;
        hook("before-database-flush", &path).map_err(|e| e.to_string())?;
        flush(&path).map_err(|e| e.to_string())?;
        hook("after-database-flush", &path).map_err(|e| e.to_string())?;
        verify_existing(&path)?;
        Ok(Observation::FilesVerifiedDirectoryPersistenceUnestablished)
    }
    fn no_hook(_: &str, _: &Path) -> io::Result<()> {
        Ok(())
    }
    fn inspect(root: &Path) -> Result<Observation, String> {
        plain(root, true).map_err(|e| e.to_string())?;
        plain(&root.join(MARKER), false).map_err(|e| e.to_string())?;
        if fs::read(root.join(MARKER)).map_err(|e| e.to_string())? != CONTENT {
            return Err("invalid marker".into());
        }
        let path = root.join("mno.sqlite");
        plain(&path, false).map_err(|e| e.to_string())?;
        for suffix in ["-journal", "-wal", "-shm"] {
            if root
                .join(format!("mno.sqlite{suffix}"))
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                return Err("sidecar; never remove".into());
            }
        }
        verify_existing(&path)?;
        Ok(Observation::FilesVerifiedDirectoryPersistenceUnestablished)
    }
    #[test]
    fn creation_flush_reopen_and_production_establishment_remain_separate() {
        let (_parent, root) = private_root();
        assert_eq!(
            initialize(root.path(), &mut no_hook).unwrap(),
            Observation::FilesVerifiedDirectoryPersistenceUnestablished
        );
        let before = fs::read(root.path().join("mno.sqlite")).unwrap();
        for _ in 0..3 {
            assert_eq!(
                inspect(root.path()).unwrap(),
                Observation::FilesVerifiedDirectoryPersistenceUnestablished
            );
        }
        assert!(initialize(root.path(), &mut no_hook).is_err());
        assert_eq!(fs::read(root.path().join("mno.sqlite")).unwrap(), before);
        let state = crate::database_state::DatabaseState::default();
        let untouched = root.path().join("production-guard");
        let production_path = state.initialize_startup(&untouched).unwrap();
        database::initialize_database(&production_path).unwrap();
        assert!(production_path.exists());
        assert!(state.operation(None).is_ok());
    }
    #[test]
    fn publication_is_no_clobber_even_when_destination_appears_at_boundary() {
        for late in [false, true] {
            let (_p, root) = private_root();
            let destination = root.path().join(MARKER);
            if !late {
                fs::write(&destination, b"preserve existing marker").unwrap();
            }
            let result = publish(root.path(), &mut |point, _| {
                if late && point == "before-publication" {
                    fs::write(&destination, b"preserve existing marker")?;
                }
                Ok(())
            });
            assert!(result.is_err());
            assert_eq!(fs::read(destination).unwrap(), b"preserve existing marker");
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        }
    }
    #[test]
    fn concurrent_database_reservations_have_one_winner_and_no_truncation() {
        let (_p, root) = private_root();
        let path = root.path().join("race.sqlite");
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                create_synthetic(&path)
            });
            let b = scope.spawn(|| {
                barrier.wait();
                create_synthetic(&path)
            });
            [a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        verify_existing(&path).unwrap();
    }
    #[test]
    fn sharing_and_read_only_flush_errors_are_propagated() {
        let (_p, root) = private_root();
        let mut held = None;
        let result = publish(root.path(), &mut |point, path| {
            if point == "before-publication" {
                held = Some(OpenOptions::new().read(true).share_mode(1).open(path)?);
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(!root.path().join(MARKER).exists());
        drop(held);
        // Cleanup can be deferred while an external handle denies deletion; do
        // not change sharing flags to force removal. All bytes are synthetic.
        for entry in fs::read_dir(root.path()).unwrap() {
            let path = entry.unwrap().path();
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".c5a-")
            );
            fs::remove_file(path).unwrap();
        }
        let path = root.path().join("flush.txt");
        fs::write(&path, b"unchanged").unwrap();
        let readonly = File::open(&path).unwrap();
        let error = readonly.sync_all().unwrap_err();
        println!("read-only sync_all: {error}");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(path).unwrap(), b"unchanged");
    }
    #[test]
    fn injected_write_flush_permission_and_post_publication_errors_preserve_evidence() {
        for point in [
            "before-marker-write",
            "before-marker-flush",
            "before-publication",
            "after-publication",
            "before-database",
            "after-schema-close",
            "before-database-flush",
            "after-database-flush",
        ] {
            let (_p, root) = private_root();
            let backup = root.path().join("backup-sentinel");
            fs::write(&backup, b"untouched backup sentinel").unwrap();
            let error = initialize(root.path(), &mut |at, _| {
                if at == point {
                    Err(io::Error::from(if at == "before-publication" {
                        io::ErrorKind::PermissionDenied
                    } else {
                        io::ErrorKind::StorageFull
                    }))
                } else {
                    Ok(())
                }
            });
            assert!(error.is_err());
            assert_eq!(fs::read(backup).unwrap(), b"untouched backup sentinel");
            let published = matches!(
                point,
                "after-publication"
                    | "before-database"
                    | "after-schema-close"
                    | "before-database-flush"
                    | "after-database-flush"
            );
            assert_eq!(root.path().join(MARKER).exists(), published);
            if published {
                assert_eq!(fs::read(root.path().join(MARKER)).unwrap(), CONTENT);
                assert!(initialize(root.path(), &mut no_hook).is_err());
            }
            assert!(!fs::read_dir(root.path()).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".c5a-")
            }));
        }
    }
    #[test]
    fn missing_partial_or_sidecar_database_never_gets_recreated() {
        for partial in [false, true] {
            let (_p, root) = private_root();
            publish(root.path(), &mut no_hook).unwrap();
            let path = root.path().join("mno.sqlite");
            if partial {
                File::create(&path).unwrap();
            }
            assert!(inspect(root.path()).is_err());
            assert!(initialize(root.path(), &mut no_hook).is_err());
            assert_eq!(path.exists(), partial);
            if partial {
                assert_eq!(fs::metadata(path).unwrap().len(), 0);
            }
        }
        let (_p, root) = private_root();
        initialize(root.path(), &mut no_hook).unwrap();
        let sidecar = root.path().join("mno.sqlite-wal");
        fs::write(&sidecar, b"preserve WAL evidence").unwrap();
        assert!(inspect(root.path()).is_err());
        assert_eq!(fs::read(sidecar).unwrap(), b"preserve WAL evidence");
    }
    #[test]
    fn nested_directory_flush_is_observation_not_proven_namespace_durability() {
        let (_p, root) = private_root();
        let nested = root.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        for path in nested
            .ancestors()
            .take_while(|path| path.starts_with(root.path()))
        {
            let opened = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path);
            match opened {
                Ok(file) => println!("directory {path:?}: sync_all result {:?}", file.sync_all()),
                Err(e) => println!("directory {path:?}: open result {e}"),
            }
        }
        assert_eq!(
            initialize(&nested, &mut no_hook).unwrap(),
            Observation::FilesVerifiedDirectoryPersistenceUnestablished
        );
        // API success/denial is diagnostic evidence, never a Ready capability.
    }
    #[test]
    fn reparse_attributes_are_rejected_without_following_target() {
        let (_p, root) = private_root();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = root.path().join("link");
        // Directory junctions need no symlink privilege. mklink uses only this
        // synthetic private tree, with no shell-expanded user input.
        let output = std::process::Command::new("cmd.exe")
            .arg("/D")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(plain(&link, true).is_err());
        assert!(publish(&link, &mut no_hook).is_err());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        fs::remove_dir(link).unwrap();
    }
    #[test]
    fn interruption_child() {
        let Some(root) = std::env::var_os("MNO_C5A_ROOT") else {
            return;
        };
        let stop = std::env::var("MNO_C5A_STOP").unwrap();
        initialize(Path::new(&root), &mut |point, _| {
            if point == stop {
                std::process::exit(86);
            }
            Ok(())
        })
        .unwrap();
        panic!("did not interrupt");
    }
    #[test]
    fn interrupted_publication_and_creation_preserve_marker_and_never_retry_initialization() {
        for point in [
            "before-publication",
            "after-publication",
            "before-database",
            "after-schema-close",
            "after-database-flush",
        ] {
            let (_p, root) = private_root();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "backup::windows_initialization_probe::windows::interruption_child",
                    "--nocapture",
                ])
                .env("MNO_C5A_ROOT", root.path())
                .env("MNO_C5A_STOP", point)
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            let status = loop {
                if let Some(s) = child.try_wait().unwrap() {
                    break s;
                }
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("child timed out");
                }
                std::thread::yield_now();
            };
            assert_eq!(status.code(), Some(86));
            if point == "before-publication" {
                assert!(!root.path().join(MARKER).exists());
                assert!(initialize(root.path(), &mut no_hook).is_err());
                assert!(fs::read_dir(root.path()).unwrap().any(|e| {
                    e.unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".c5a-")
                }));
            } else {
                assert_eq!(fs::read(root.path().join(MARKER)).unwrap(), CONTENT);
                assert!(initialize(root.path(), &mut no_hook).is_err());
            }
            let path = root.path().join("mno.sqlite");
            if matches!(point, "after-schema-close" | "after-database-flush") {
                verify_existing(&path).unwrap();
            } else {
                assert!(!path.exists());
            }
        }
    }
}
