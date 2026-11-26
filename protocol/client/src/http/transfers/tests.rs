use super::*;
use super::super::Error;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("nightjar-transfers-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("saved.bin")
}

#[test]
fn a_download_lands_whole_or_not_at_all() {
    let dest = scratch("whole");
    std::fs::write(&dest, b"old").unwrap();
    // Cut short: the old file stays and the partial one is gone.
    let result = download_into(&dest, |sink| {
        sink.write_all(b"new but sh").unwrap();
        Err(FileError::Host(Error::Malformed(
            "the download ended after 10 of 20 bytes".into(),
        )))
    });
    assert!(matches!(result, Err(FileError::Host(Error::Malformed(_)))));
    assert_eq!(std::fs::read(&dest).unwrap(), b"old");
    assert!(!partial_path(&dest).exists());
    // Cancelled: the same.
    let result = download_into(&dest, |sink| {
        sink.write_all(b"x").unwrap();
        Err(FileError::Host(Error::Cancelled))
    });
    assert!(matches!(result, Err(FileError::Host(Error::Cancelled))));
    assert!(!partial_path(&dest).exists());
    // Whole: it takes the free name.
    std::fs::remove_file(&dest).unwrap();
    let result = download_into(&dest, |sink| {
        sink.write_all(b"new and whole").unwrap();
        Ok(13)
    });
    assert_eq!(result.unwrap(), 13);
    assert_eq!(std::fs::read(&dest).unwrap(), b"new and whole");
    assert!(!partial_path(&dest).exists());
    std::fs::remove_dir_all(dest.parent().unwrap()).unwrap();
}

#[test]
fn a_file_that_takes_the_name_during_a_download_is_not_replaced() {
    let dest = scratch("taken");
    let result = download_into(&dest, |sink| {
        sink.write_all(b"downloaded").unwrap();
        // A browser saves its own file under the name meanwhile.
        std::fs::write(&dest, b"the user's").unwrap();
        Ok(10)
    });
    match result {
        Err(FileError::Local(e)) => {
            assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists, "{e}");
            assert!(e.to_string().contains("was not replaced"), "{e}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(&dest).unwrap(), b"the user's");
    assert!(!partial_path(&dest).exists());
    std::fs::remove_dir_all(dest.parent().unwrap()).unwrap();
}

#[test]
fn a_part_file_that_is_already_there_is_left_alone() {
    let dest = scratch("part");
    let part = partial_path(&dest);
    std::fs::write(&part, b"a browser's download").unwrap();
    let mut called = false;
    let result = download_into(&dest, |_| {
        called = true;
        Ok(0)
    });
    assert!(
        matches!(&result, Err(FileError::Local(e)) if e.kind() == std::io::ErrorKind::AlreadyExists),
        "{result:?}"
    );
    assert!(!called);
    assert_eq!(std::fs::read(&part).unwrap(), b"a browser's download");
    assert!(!dest.exists());
    std::fs::remove_dir_all(dest.parent().unwrap()).unwrap();
}

#[test]
fn a_partial_file_that_cannot_be_deleted_is_named_in_the_error() {
    let dest = scratch("stays");
    let part = partial_path(&dest);
    let result = download_into(&dest, |_| {
        // Swap the open file's name for a directory, which remove_file cannot delete.
        std::fs::remove_file(&part).unwrap();
        std::fs::create_dir(&part).unwrap();
        Err(FileError::Host(Error::Cancelled))
    });
    match result {
        Err(FileError::Local(e)) => {
            let text = e.to_string();
            assert!(
                text.contains("saved.bin.part") && text.contains("could not be deleted"),
                "{text}"
            );
        }
        other => panic!("{other:?}"),
    }
    std::fs::remove_dir_all(dest.parent().unwrap()).unwrap();
}

#[test]
fn a_download_into_a_missing_folder_is_a_local_failure() {
    let dest = std::env::temp_dir()
        .join("nightjar-no-such-folder-7f3a")
        .join("x.bin");
    let mut called = false;
    let result = download_into(&dest, |_| {
        called = true;
        Ok(0)
    });
    assert!(matches!(result, Err(FileError::Local(_))));
    assert!(!called, "nothing is fetched without a place to put it");
}

#[test]
fn copy_new_and_unlink_opens_source_before_creating_target() {
    let dest = scratch("copy-new");
    let missing_src = dest.with_file_name("missing.part");
    let absent_to = dest.with_file_name("absent.bin");
    let err = copy_new_and_unlink(&missing_src, &absent_to).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert!(!absent_to.exists(), "missing source left an empty target");

    let src = dest.with_file_name("ready.part");
    std::fs::write(&src, b"payload").unwrap();
    std::fs::write(&dest, b"existing").unwrap();
    let err = copy_new_and_unlink(&src, &dest).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&dest).unwrap(), b"existing");
    assert_eq!(std::fs::read(&src).unwrap(), b"payload");

    copy_new_and_unlink(&src, &absent_to).unwrap();
    assert_eq!(std::fs::read(&absent_to).unwrap(), b"payload");
    assert!(!src.exists());
    std::fs::remove_dir_all(dest.parent().unwrap()).unwrap();
}

