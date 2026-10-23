// Included only in the disposable copy of upstream's offline_tests module.
// All installer payloads are inert data; tests call extract/verify, never install.
#[cfg(windows)]
mod note_taker_archives {
    use super::*;
    use std::io::{Cursor, Write};

    const PAYLOAD: &[u8] = b"inert installer fixture, never executed";

    fn archive(name: &str, link: Option<&str>) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        if let Some(target) = link {
            writer.add_symlink(name, target, options).unwrap();
        } else {
            writer.start_file(name, options).unwrap();
            writer.write_all(PAYLOAD).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn extracted_path(installer: &WindowsUpdaterType, extension: &str) -> PathBuf {
        match (installer, extension) {
            (WindowsUpdaterType::Nsis { path, .. }, "exe") => path.clone(),
            (WindowsUpdaterType::Msi { path, .. }, "msi") => {
                PathBuf::from(path.to_str().unwrap().trim_matches('"'))
            }
            _ => panic!("wrong installer type"),
        }
    }

    #[test]
    fn deflated_zip_preserves_exe_and_msi_support() {
        for ext in ["exe", "msi"] {
            let update = updater().restore_update(manifest()).unwrap().unwrap();
            let installer = update
                .extract(&archive(&format!("setup.{ext}"), None))
                .unwrap();
            let path = extracted_path(&installer, ext);
            assert!(path.starts_with(std::env::temp_dir()));
            assert_eq!(std::fs::read(path).unwrap(), PAYLOAD);
        }
    }

    #[test]
    fn direct_exe_and_msi_formats_still_work_without_execution() {
        for (ext, magic) in [
            ("exe", &[0x4d, 0x5a][..]),
            ("msi", &[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1][..]),
        ] {
            let mut bytes = vec![0; 512];
            bytes[..magic.len()].copy_from_slice(magic);
            let update = updater().restore_update(manifest()).unwrap().unwrap();
            let installer = update.extract(&bytes).unwrap();
            assert_eq!(
                std::fs::read(extracted_path(&installer, ext)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn traversal_and_external_symlinks_cannot_escape() {
        let sentinel = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(sentinel.path(), b"unchanged").unwrap();
        let name = sentinel.path().file_name().unwrap().to_str().unwrap();
        for malicious in [format!("../{name}"), format!("..\\{name}")] {
            let update = updater().restore_update(manifest()).unwrap().unwrap();
            assert!(update.extract(&archive(&malicious, None)).is_err());
            assert_eq!(std::fs::read(sentinel.path()).unwrap(), b"unchanged");
        }
        for target in [
            format!("../{name}"),
            sentinel.path().to_str().unwrap().to_string(),
        ] {
            let update = updater().restore_update(manifest()).unwrap().unwrap();
            // Require ZIP's containment rejection, not an OS symlink privilege
            // error, so this proves the fix even without Developer Mode.
            assert!(matches!(
                update.extract(&archive("setup.exe", Some(&target))),
                Err(Error::Extract(zip::result::ZipError::InvalidArchive(message)))
                    if message.contains("escapes the destination directory")
            ));
            assert_eq!(std::fs::read(sentinel.path()).unwrap(), b"unchanged");
        }
    }

    #[test]
    fn absolute_entry_names_remain_inside_extraction_directory() {
        let sentinel = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(sentinel.path(), b"unchanged").unwrap();
        let update = updater().restore_update(manifest()).unwrap().unwrap();
        // zip 9 normalizes roots; either an error or a contained extraction is safe.
        assert!(
            update
                .extract(&archive(sentinel.path().to_str().unwrap(), None))
                .is_err()
        );
        assert_eq!(std::fs::read(sentinel.path()).unwrap(), b"unchanged");
    }

    #[test]
    fn missing_installer_and_truncated_zip_fail() {
        let update = updater().restore_update(manifest()).unwrap().unwrap();
        assert!(matches!(
            update.extract(&archive("readme.txt", None)),
            Err(Error::BinaryNotFoundInArchive)
        ));
        let mut bytes = archive("setup.exe", None);
        bytes.truncate(bytes.len() / 2);
        assert!(update.extract(&bytes).is_err());
    }
}
