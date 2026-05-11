// SPDX-License-Identifier: AGPL-3.0-only
//! Pure-Rust NoCloud cidata ISO generation.
//!
//! Replaces the `genisoimage` shell dependency with `hadris-iso` for
//! creating the tiny ISO9660 images that cloud-init expects. The ISOs
//! typically contain only `user-data` and `meta-data` (< 64 KiB total).

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use hadris_iso::joliet::JolietLevel;
use hadris_iso::write::options::{BaseIsoLevel, CreationFeatures, FormatOptions};
use hadris_iso::write::{File as IsoFile, InputFiles, IsoImageWriter};
use hadris_iso::read::PathSeparator;

/// Generate a NoCloud cidata ISO at `output_path`.
///
/// The ISO contains exactly two files: `user-data` and `meta-data`, with
/// volume ID `cidata`. Joliet extensions are enabled for broad guest
/// compatibility.
pub fn generate_nocloud_iso(
    output_path: &Path,
    user_data: &[u8],
    meta_data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let files = InputFiles {
        path_separator: PathSeparator::ForwardSlash,
        files: vec![
            IsoFile::File {
                name: Arc::new("user-data".to_string()),
                contents: user_data.to_vec(),
            },
            IsoFile::File {
                name: Arc::new("meta-data".to_string()),
                contents: meta_data.to_vec(),
            },
        ],
    };

    let format_options = FormatOptions {
        volume_name: "cidata".to_string(),
        system_id: None,
        volume_set_id: None,
        publisher_id: None,
        preparer_id: None,
        application_id: None,
        sector_size: 2048,
        path_separator: PathSeparator::ForwardSlash,
        features: CreationFeatures {
            filenames: BaseIsoLevel::Level1 {
                supports_lowercase: true,
                supports_rrip: false,
            },
            long_filenames: true,
            joliet: Some(JolietLevel::Level3),
            rock_ridge: None,
            el_torito: None,
            hybrid_boot: None,
        },
        strict_charset: false,
    };

    // 512 KiB is more than enough for cloud-init payloads
    let iso_size = std::cmp::max(512 * 1024, user_data.len() + meta_data.len() + 128 * 1024);
    let mut buffer = Cursor::new(vec![0u8; iso_size]);

    IsoImageWriter::format_new(&mut buffer, files, format_options)?;

    // Trim trailing zero sectors to produce a minimal ISO
    let data = buffer.into_inner();
    let trimmed_len = trim_trailing_zeros(&data);
    std::fs::write(output_path, &data[..trimmed_len])?;

    Ok(())
}

/// Find the last non-zero byte and round up to sector boundary (2048).
fn trim_trailing_zeros(data: &[u8]) -> usize {
    let last_nonzero = data.iter().rposition(|&b| b != 0).unwrap_or(0);
    let sector = 2048;
    ((last_nonzero / sector) + 1) * sector
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generate_nocloud_iso_creates_valid_file() {
        let tmp = TempDir::new().unwrap();
        let iso_path = tmp.path().join("test-cidata.iso");

        let user_data = b"#cloud-config\npackages:\n  - curl\n";
        let meta_data = b"instance-id: test\nlocal-hostname: test\n";

        generate_nocloud_iso(&iso_path, user_data, meta_data).unwrap();

        assert!(iso_path.exists());
        let bytes = std::fs::read(&iso_path).unwrap();
        assert!(bytes.len() >= 2048, "ISO too small: {} bytes", bytes.len());
        // ISO9660 magic at sector 16 (byte offset 32768)
        assert_eq!(&bytes[32769..32774], b"CD001", "Missing ISO9660 signature");
    }

    #[test]
    fn generate_nocloud_iso_volume_id_cidata() {
        let tmp = TempDir::new().unwrap();
        let iso_path = tmp.path().join("vol-check.iso");

        generate_nocloud_iso(&iso_path, b"test", b"test").unwrap();

        let bytes = std::fs::read(&iso_path).unwrap();
        // Volume ID is at offset 32808 (sector 16 byte 40) in the primary volume descriptor
        let vol_id_region = &bytes[32808..32840];
        let vol_id = std::str::from_utf8(vol_id_region)
            .unwrap_or("")
            .trim();
        assert!(
            vol_id.starts_with("cidata"),
            "Volume ID should be 'cidata', got '{}'",
            vol_id
        );
    }

    #[test]
    fn trim_trailing_zeros_rounds_to_sector() {
        let mut data = vec![0u8; 8192];
        data[100] = 0xFF;
        assert_eq!(trim_trailing_zeros(&data), 2048);

        data[4000] = 0xFF;
        assert_eq!(trim_trailing_zeros(&data), 4096);
    }
}
