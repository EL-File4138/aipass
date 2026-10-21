//! Relative paths in encrypted exports and sync metadata use the same format on every OS.
use serde::{Deserialize, Deserializer, Serializer};
use std::path::{Path, PathBuf};

pub fn normalize(path: &Path) -> Result<String, &'static str> {
    let text = path
        .to_str()
        .ok_or("path must be UTF-8")?
        .replace('\\', "/");
    if text.is_empty()
        || text.contains([':', '\0'])
        || text
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("path must be a portable relative path");
    }
    Ok(text)
}

pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&normalize(path).map_err(serde::ser::Error::custom)?)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
    let value = String::deserialize(deserializer)?;
    normalize(Path::new(&value))
        .map(PathBuf::from)
        .map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Record {
        #[serde(with = "super")]
        path: PathBuf,
    }

    #[test]
    fn windows_and_unix_metadata_share_one_wire_format() {
        for raw in [r"objects\fixture.aipobj", "objects/fixture.aipobj"] {
            let data = serde_json::json!({ "path": raw });
            let record: Record = serde_json::from_value(data).unwrap();
            assert_eq!(record.path, Path::new("objects").join("fixture.aipobj"));
            assert_eq!(
                serde_json::to_value(record).unwrap()["path"],
                "objects/fixture.aipobj"
            );
        }
        let record = Record {
            path: Path::new("objects").join("fixture.aipobj"),
        };
        assert_eq!(
            serde_json::to_value(record).unwrap()["path"],
            "objects/fixture.aipobj"
        );
    }

    #[test]
    fn rejects_windows_and_unix_escapes_on_every_platform() {
        for path in [
            "/outside",
            r"\outside",
            "../outside",
            r"..\outside",
            "a/../b",
            r"a\..\b",
            r"C:\outside",
            "C:outside",
            r"\\host\share",
            "a//b",
            "a/./b",
            "a\0b",
        ] {
            assert!(
                serde_json::from_value::<Record>(serde_json::json!({"path":path})).is_err(),
                "{path}"
            );
        }
    }
}
