use std::fs;
use std::path::PathBuf;
use chrono;
use serde::{Serialize, Deserialize};
use karl_common::*;

/// Persistent data sink.
///
/// Operations to the data sink must be authenticated in the above layer.
pub struct DataSink {
    pub data_path: PathBuf,
}

/// Result of `get_data` on a directory.
#[derive(Serialize, Deserialize, Default)]
pub struct ReadDirResult {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

impl DataSink {
    /// Creates a directory for the data sink relative to the `karl_path`.
    ///
    /// Generates the path to the persistent data sink, which is just
    /// a directory in the filesystem. Creates an empty directory at
    /// the path, `<karl_path>/data/` if it does not already exist.
    pub fn new(karl_path: PathBuf) -> Self {
        let data_path = karl_path.join("data");
        Self {
            data_path: data_path.to_path_buf(),
        }
    }

    /// Initializes directories at `<karl_path>/data/<id>/<tag>/` for each
    /// of the entity's output tags. An entity is a sensor or module.
    pub fn new_entity(
        &self,
        id: String,
        tags: Vec<String>,
    ) -> Result<(), Error> {
        let path = self.data_path.join(&id);
        fs::create_dir_all(&path)?;
        for tag in &tags {
            fs::create_dir_all(&path.join(tag))?;
        }
        Ok(())
    }

    /// Push sensor data.
    ///
    /// Parameters:
    /// - `sensor_id`: the sensor ID.
    /// - `tag`: output tag.
    /// - `data`: the data.
    ///
    /// Returns: modified tag, and timestamp.
    pub fn push_sensor_data(
        &self,
        sensor_id: SensorID,
        tag: String,
        data: Vec<u8>,
    ) -> Result<(String, String), Error> {
        let path = self.data_path.join(&sensor_id).join(&tag);
        assert!(path.is_dir());
        loop {
            let dt = chrono::prelude::Local::now().format("%+").to_string();
            let path = path.join(&dt);
            if path.exists() {
                continue;
            }
            debug!("push_raw_data sensor_id={} tag={} timestamp={} (len {})",
                sensor_id, tag, dt, data.len());
            fs::write(path, data)?;
            break Ok((format!("{}.{}", &sensor_id, &tag), dt));
        }
    }

    /// Write data to the given tag.
    ///
    /// Parameters:
    /// - `tag`: ID and tag.
    /// - `data`: The bytes to push.
    ///
    /// Returns: modified tag, and timestamp.
    pub fn push_data(
        &self,
        tag: String,
        data: Vec<u8>,
    ) -> Result<(String, String), Error> {
        let (id, tag) = {
            let mut split = tag.split('.');
            (split.next().unwrap(), split.next().unwrap())
        };
        let path = self.data_path.join(id).join(tag);
        loop {
            let dt = chrono::prelude::Local::now().format("%+").to_string();
            let path = path.join(&dt);
            if path.exists() {
                continue;
            }
            debug!("push_data id={} tag={} timestamp={} (len {})",
                id, tag, dt, data.len());
            fs::write(path, data)?;
            break Ok((format!("{}.{}", &id, &tag), dt));
        }
    }

    /// Get data from the given path.
    ///
    /// Parameters:
    /// - `path`: The sanitized path to the data.
    /// - `dir`: Whether the path is a directory.
    ///
    /// Returns: the raw bytes of the file. If the file is a directory,
    /// returns the files and directories in json.
    ///
    /// Errors if the path does not exist. Errors if the path is a file but
    /// the request indicates it is a directory, or vice versa.
    pub fn get_data(
        &self,
        tag: String,
        lower: String,
        upper: String,
    ) -> Result<Vec<u8>, Error> {
        assert_eq!(lower, upper);
        debug!("get {} {}", tag, lower);
        let (id, tag) = {
            let mut split = tag.split('.');
            (split.next().unwrap(), split.next().unwrap())
        };
        let path = self.data_path.join(id).join(tag).join(lower);
        Ok(fs::read(path)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::Path;
    use tempdir::TempDir;

    fn init_test() -> (TempDir, DataSink) {
        let dir = TempDir::new("karl").unwrap();
        let data_sink = DataSink::new(dir.path().to_path_buf());
        (dir, data_sink)
    }

    #[test]
    fn test_data_sink_initial_paths() {
        let (dir, sink) = init_test();
        assert_eq!(sink.data_path, dir.path().join("data"));
        assert_eq!(sink.state_path, dir.path().join("state"));
    }

    #[test]
    fn test_put_directory_simple() {
        let (_dir, sink) = init_test();
        let path = Path::new("raw/camera").to_path_buf();
        let expected_path = sink.data_path.join(&path);
        assert!(!expected_path.exists());
        assert!(sink.put_data(path, None, false).is_ok());
        assert!(expected_path.exists());
        assert!(expected_path.is_dir());
    }

    #[test]
    fn test_put_directory_recursive() {
        let (_dir, sink) = init_test();
        let path = Path::new("raw/camera/1234").to_path_buf();
        let expected_path = sink.data_path.join(&path);
        assert!(!sink.data_path.join("raw/camera/1234").exists());
        assert!(!sink.data_path.join("raw/camera").exists());
        assert!(sink.put_data(path.clone(), None, false).is_err());
        assert!(!sink.data_path.join("raw/camera").exists());
        assert!(!expected_path.exists());
        assert!(sink.put_data(path.clone(), None, true).is_ok());
        assert!(expected_path.exists());
        assert!(expected_path.is_dir());
    }

    #[test]
    fn test_put_file_simple() {
        let (_dir, sink) = init_test();
        let path = Path::new("raw/camera").to_path_buf();
        let expected_path = sink.data_path.join(&path);
        assert!(!expected_path.exists());
        assert!(sink.put_data(path, Some(vec![]), false).is_ok());
        assert!(expected_path.exists());
        assert!(expected_path.is_file());
    }

    #[test]
    fn test_put_file_recursive() {
        let (_dir, sink) = init_test();
        let path = Path::new("raw/camera/1234").to_path_buf();
        let expected_path = sink.data_path.join(&path);
        assert!(!sink.data_path.join("raw/camera/1234").exists());
        assert!(!sink.data_path.join("raw/camera").exists());
        assert!(sink.put_data(path.clone(), Some(vec![]), false).is_err());
        assert!(!sink.data_path.join("raw/camera").exists());
        assert!(!expected_path.exists());
        assert!(sink.put_data(path.clone(), Some(vec![]), true).is_ok());
        assert!(expected_path.exists());
        assert!(expected_path.is_file());
    }

    #[test]
    fn test_get_return_type() {
        let (_dir, sink) = init_test();
        let dir_path = Path::new("raw/camera").to_path_buf();
        let file_path = Path::new("raw/camera/file").to_path_buf();
        let file_bytes: Vec<u8> = vec![10, 20, 30, 40];
        assert!(sink.put_data(file_path.clone(), Some(file_bytes.clone()), true).is_ok());
        assert!(sink.data_path.join(&file_path).exists());
        assert!(sink.data_path.join(&dir_path).exists());

        // get data
        assert!(sink.get_data(file_path.clone(), true).is_err(), "not a directory");
        assert!(sink.get_data(file_path.clone(), false).is_ok());
        assert_eq!(sink.get_data(file_path.clone(), false).unwrap(), file_bytes);
        assert!(sink.get_data(dir_path.clone(), false).is_err(), "not a file");
        assert!(sink.get_data(dir_path.clone(), true).is_ok());
    }

    #[test]
    fn test_get_deserialize_empty_dir() {
        let (_dir, sink) = init_test();
        let path = Path::new("raw/camera").to_path_buf();
        assert!(sink.put_data(path.clone(), None, false).is_ok());
        assert!(sink.data_path.join(&path).is_dir());

        let readdir: ReadDirResult = {
            let bytes = sink.get_data(path.clone(), true);
            assert!(bytes.is_ok(), "error in get_data");
            let deserialized = serde_json::de::from_slice(&bytes.unwrap()[..]);
            assert!(deserialized.is_ok(), "error deserializing bytes as json");
            deserialized.unwrap()
        };
        assert_eq!(readdir.files.len(), 0);
        assert_eq!(readdir.dirs.len(), 0);
    }

    #[test]
    fn test_get_deserialize_nonempty_dir() {
        let (_dir, sink) = init_test();
        let a = Path::new("raw/camera/a").to_path_buf();
        let b = Path::new("raw/camera/b").to_path_buf();
        let c = Path::new("raw/camera/c").to_path_buf();
        let d = Path::new("raw/camera/d").to_path_buf();
        let bytes: Vec<u8> = vec![10, 20, 30, 40];
        assert!(sink.put_data(d.clone(), Some(bytes.clone()), true).is_ok());
        assert!(sink.put_data(c.clone(), Some(bytes.clone()), true).is_ok());
        assert!(sink.put_data(a.clone(), Some(bytes.clone()), true).is_ok());
        assert!(sink.put_data(b.clone(), None, true).is_ok());

        let readdir: ReadDirResult = {
            let bytes = sink.get_data(Path::new("raw/camera").to_path_buf(), true);
            assert!(bytes.is_ok(), "error in get_data");
            let deserialized = serde_json::de::from_slice(&bytes.unwrap()[..]);
            assert!(deserialized.is_ok(), "error deserializing bytes as json");
            deserialized.unwrap()
        };
        assert_eq!(readdir.files.len(), 3);
        assert_eq!(readdir.dirs.len(), 1);
        assert_eq!(
            readdir.files,
            vec!["a".to_string(), "c".to_string(), "d".to_string()],
        );
        assert_eq!(readdir.dirs, vec!["b".to_string()]);
    }
}
