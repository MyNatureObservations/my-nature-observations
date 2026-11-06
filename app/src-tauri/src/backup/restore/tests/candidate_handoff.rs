//! C7.2 probe. The caller owns a private, isolated fixture namespace.
//! No production selection or admission capability is issued here.
use super::*;
use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

type ProbeResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn pin_directory(path: &Path) -> io::Result<File> {
    let pin = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if !private::is_plain_directory(&pin.metadata()?) {
        return Err(io::Error::other("unsafe generation directory"));
    }
    Ok(pin)
}

fn pin_file(path: &Path, sharing: u32) -> io::Result<File> {
    let pin = OpenOptions::new()
        .read(true)
        .share_mode(sharing)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = pin.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err(io::Error::other("unsafe candidate file"));
    }
    Ok(pin)
}

fn standalone(path: &Path) -> ProbeResult<()> {
    for entry in fs::read_dir(path.parent().unwrap())? {
        if entry?.file_name() != DATABASE {
            return Err("unexpected candidate directory entry or SQLite sidecar".into());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    Copied,
    BeforeVerification,
    BeforeWritableOpen,
}

struct Pending {
    path: PathBuf,
    digest: String,
    identity: private::FileIdentity,
    _root: File,
    _directory: File,
}

fn verify(path: &Path, pin: &File, digest: &str) -> ProbeResult<()> {
    standalone(path)?;
    check_header(path, pin.metadata()?.len())?;
    if handle_digest(pin)? != digest {
        return Err("candidate digest mismatch".into());
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let result = db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0));
    db.close().map_err(|(_, error)| error)?;
    if result? != "ok" {
        return Err("candidate integrity check failed".into());
    }
    // Independently reopen using the production current-schema and asset checks.
    database::initialize_database(path)?;
    standalone(path)?;
    Ok(())
}

fn construct(
    archive: &Path,
    root: &Path,
    mut hook: impl FnMut(Boundary, &Path) -> ProbeResult<()>,
) -> ProbeResult<Pending> {
    let root_pin = pin_directory(root)?;
    let source = pin_file(archive, FILE_SHARE_READ)?;
    let prepared = prepare_file_in(source, root, Arc::new(AtomicBool::new(false)), |path| {
        fs2::available_space(path)
    })?;
    let manifest = prepared.manifest();
    if manifest.format_version != 5 || manifest.database_schema_version != database::SCHEMA_VERSION
    {
        return Err("candidate requires current backup format and schema".into());
    }
    let digest = manifest.database_sha256.clone();
    let closed = prepared
        .try_close()
        .map_err(|_| "candidate staging close failed")?;
    let directory = root.join(uuid::Uuid::new_v4().to_string());
    private::create_retained_directory(&directory)?;
    let directory_pin = pin_directory(&directory)?;
    let path = directory.join(DATABASE);
    let mut output = private::create_file(&path)?;
    let identity = private::file_identity(&output)?;
    let mut input = &closed._file;
    input.seek(SeekFrom::Start(0))?;
    io::copy(&mut input, &mut output)?;
    hook(Boundary::Copied, &path)?;
    output.sync_all()?;
    drop(output);
    hook(Boundary::BeforeVerification, &path)?;
    let pin = pin_file(&path, FILE_SHARE_READ)?;
    if private::file_identity(&pin)? != identity {
        return Err("retained candidate object changed".into());
    }
    verify(&path, &pin, &digest)?;
    Ok(Pending {
        path,
        digest,
        identity,
        _root: root_pin,
        _directory: directory_pin,
    })
}

struct Admitted<'a> {
    db: Connection,
    _bridge: File,
    _pending: &'a Pending,
}

impl Admitted<'_> {
    fn close(self) -> rusqlite::Result<()> {
        self.db.close().map_err(|(_, error)| error)
    }
}

impl Pending {
    // The fixture models logical admission after verification, before permitting
    // writes. The overlapping pin prevents rename/delete across the transition.
    fn admit(
        &self,
        hook: impl FnOnce(Boundary, &Path) -> ProbeResult<()>,
    ) -> ProbeResult<Admitted<'_>> {
        let immutable = pin_file(&self.path, FILE_SHARE_READ)?;
        if private::file_identity(&immutable)? != self.identity {
            return Err("pending candidate object changed".into());
        }
        verify(&self.path, &immutable, &self.digest)?;
        let bridge = pin_file(&self.path, FILE_SHARE_READ | FILE_SHARE_WRITE)?;
        if private::file_identity(&bridge)? != self.identity {
            return Err("handoff object changed".into());
        }
        // Competing writers are still denied while the hook exercises the handoff.
        hook(Boundary::BeforeWritableOpen, &self.path)?;
        standalone(&self.path)?;
        drop(immutable);
        let db = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        if db.is_readonly(rusqlite::MAIN_DB)? {
            db.close().map_err(|(_, error)| error)?;
            return Err("candidate reopened read-only".into());
        }
        Ok(Admitted {
            db,
            _bridge: bridge,
            _pending: self,
        })
    }
}

fn backup(root: &Path) -> PathBuf {
    let db_path = root.join("source.sqlite");
    database::create_current_database(&db_path).unwrap();
    let db = Connection::open(&db_path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('entry','2026-10-25','Synthetic',?1,2,'created','updated')", [DOC]).unwrap();
    let image = root.join("asset.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 100, 80, 255]))
        .save(&image)
        .unwrap();
    let asset = crate::photographs::prepare(&image, "entry".into()).unwrap();
    asset.insert(&db).unwrap();
    let document = serde_json::json!({"type":"doc","content":[{"type":"journalImage","attrs":{
        "nodeId":uuid::Uuid::new_v4().to_string(),"assetId":asset.asset_id,
        "placement":"left","size":"small","caption":"Fixture","alt":"Green square"
    }}]});
    db.execute(
        "UPDATE journal_entries SET content_json=?1",
        [document.to_string()],
    )
    .unwrap();
    let archive = root.join("source.mno-backup");
    create_backup(&db, &archive, BackupKind::Manual).unwrap();
    db.close().unwrap();
    fs::remove_file(image).unwrap();
    archive
}

#[test]
fn repeated_candidates_admit_and_reopen_after_legitimate_edits() {
    let fixture = TempDir::new().unwrap();
    let archive = backup(fixture.path());
    let original = fs::read(&archive).unwrap();
    let root = fixture.path().join("generations");
    private::ensure_directory(&root).unwrap();
    let active = fixture.path().join("mno.sqlite");
    fs::write(&active, b"active journal").unwrap();
    let wal = fixture.path().join("mno.sqlite-wal");
    fs::write(&wal, b"unrelated WAL").unwrap();
    let mut previous = None;
    for _ in 0..2 {
        let pending = construct(&archive, &root, |_, _| Ok(())).unwrap();
        let admitted = pending
            .admit(|_, path| {
                assert!(OpenOptions::new().write(true).open(path).is_err());
                assert!(fs::remove_file(path).is_err());
                Ok(())
            })
            .unwrap();
        assert!(fs::rename(&pending.path, pending.path.with_extension("moved")).is_err());
        admitted
            .db
            .execute("UPDATE journal_entries SET title='edited'", [])
            .unwrap();
        assert_eq!(
            private::file_identity(&admitted._bridge).unwrap(),
            pending.identity
        );
        admitted.close().unwrap();
        assert_ne!(
            handle_digest(&File::open(&pending.path).unwrap()).unwrap(),
            pending.digest
        );
        database::initialize_database(&pending.path).unwrap();
        let db =
            Connection::open_with_flags(&pending.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let title: String = db
            .query_row("SELECT title FROM journal_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "edited");
        db.close().unwrap();
        if let Some((path, bytes)) = previous.take() {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        previous = Some((pending.path.clone(), fs::read(&pending.path).unwrap()));
    }
    assert_eq!(fs::read(active).unwrap(), b"active journal");
    assert_eq!(fs::read(wal).unwrap(), b"unrelated WAL");
    assert_eq!(fs::read(archive).unwrap(), original);
}

#[test]
fn candidate_failures_preserve_inputs_and_sidecars() {
    let fixture = TempDir::new().unwrap();
    let archive = backup(fixture.path());
    let original = fs::read(&archive).unwrap();
    let root = fixture.path().join("generations");
    private::ensure_directory(&root).unwrap();
    let active = fixture.path().join("mno.sqlite");
    fs::write(&active, b"active").unwrap();
    let old = root.join("previous");
    fs::create_dir(&old).unwrap();
    let old_db = old.join(DATABASE);
    fs::write(&old_db, b"old").unwrap();

    for fault in [Boundary::Copied, Boundary::BeforeVerification] {
        assert!(
            construct(&archive, &root, |step, path| {
                if step == fault {
                    if fault == Boundary::Copied {
                        return Err("injected interruption".into());
                    }
                    fs::write(path, b"tampered candidate")?;
                }
                Ok(())
            })
            .is_err()
        );
    }

    let pending = construct(&archive, &root, |_, _| Ok(())).unwrap();
    assert!(
        pending
            .admit(|_, _| Err("injected handoff failure".into()))
            .is_err()
    );
    let writer = OpenOptions::new().write(true).open(&pending.path).unwrap();
    assert!(pending.admit(|_, _| Ok(())).is_err());
    drop(writer);
    let mut competing_reader = None;
    assert!(
        pending
            .admit(|_, path| {
                competing_reader = Some(pin_file(path, FILE_SHARE_READ)?);
                Ok(())
            })
            .is_err()
    );
    drop(competing_reader);
    let changed = construct(&archive, &root, |_, _| Ok(())).unwrap();
    fs::write(&changed.path, b"same object with changed bytes").unwrap();
    assert!(changed.admit(|_, _| Ok(())).is_err());
    for suffix in ["-wal", "-shm"] {
        let sidecar = pending.path.with_file_name(format!("{DATABASE}{suffix}"));
        fs::write(&sidecar, b"diagnostic evidence").unwrap();
        assert!(pending.admit(|_, _| Ok(())).is_err());
        assert_eq!(fs::read(&sidecar).unwrap(), b"diagnostic evidence");
        fs::remove_file(sidecar).unwrap(); // Fixture reset only.
    }
    let replacement = pending.path.with_extension("saved");
    fs::rename(&pending.path, &replacement).unwrap();
    fs::copy(&replacement, &pending.path).unwrap();
    assert!(pending.admit(|_, _| Ok(())).is_err());

    let damaged = fixture.path().join("damaged.mno-backup");
    fs::write(&damaged, &original[..original.len() / 2]).unwrap();
    assert!(construct(&damaged, &root, |_, _| Ok(())).is_err());
    fs::write(&damaged, &original).unwrap();
    rewrite(&damaged, |_, bytes| bytes[0] ^= 1);
    let tampered = fs::read(&damaged).unwrap();
    assert!(construct(&damaged, &root, |_, _| Ok(())).is_err());
    assert_eq!(fs::read(damaged).unwrap(), tampered);
    assert_eq!(fs::read(archive).unwrap(), original);
    assert_eq!(fs::read(active).unwrap(), b"active");
    assert_eq!(fs::read(old_db).unwrap(), b"old");
}
