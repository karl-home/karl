use std::fs;
use std::path::PathBuf;
use crate::common::Error;
use crate::controller::types::*;

/// Persistent data sink.
///
/// Operations to the data sink must be authenticated in the above layer.
pub struct DataSink {
    pub path: PathBuf,
}

impl DataSink {
    /// Creates a directory for the data sink relative to the `karl_path`.
    ///
    /// Generates the path to the persistent data sink, which is just
    /// a directory in the filesystem. Creates an empty directory at
    /// the path, `<karl_path>/data/` if it does not already exist.
    pub fn new(karl_path: PathBuf) -> Self {
        let path = karl_path.join("data");
        fs::create_dir_all(&path).unwrap();
        Self {
            path: path.to_path_buf(),
        }
    }

    /// Initializes a directory at `<karl_path>/data/<sensor_id>`
    /// for the new sensor.
    pub fn new_sensor(&self, sensor_id: SensorID) {
        let sensor_path = self.path.join(sensor_id);
        fs::create_dir_all(&sensor_path).unwrap();
    }

    /// Write data to the given path.
    ///
    /// Parameters:
    /// - `path`: The path to the data.
    /// - `bytes`: The bytes to write to the file, otherwise a directory.
    /// - `recursive`: Whether to create parent directories if they do not
    ///    already exist.
    ///
    /// Errors if the path requires creating parent directories and recursive
    /// is false. Errors if creating a file that already exists.
    pub fn put_data(
        &self,
        path: PathBuf,
        bytes: Option<Vec<u8>>,
        recursive: bool,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Get data from the given path.
    ///
    /// Parameters:
    /// - `path`: The path to the data.
    /// - `dir`: Whether the path is a directory.
    ///
    /// Returns: the raw bytes of the file. If the file is a directory,
    /// returns an array of files and directories in the directory.
    ///
    /// Errors if the path does not exist. Errors if the path is a file but
    /// the request indicates it is a directory.
    pub fn get_data(
        path: PathBuf,
        dir: bool,
    ) -> Result<Vec<u8>, Error> {
        Ok(vec![])
    }

    /// Deletes data at the given path.
    ///
    /// Parameters:
    /// - `path`: The path to the data.
    /// - `recursive`: If the path is a directory, whether to delete the
    ///    directory and its internal contents.
    ///
    /// Errors if deleting a path that does not exist. Errors if deleting
    /// a directory when recursive is false.
    pub fn delete_data(
        &self,
        path: PathBuf,
        recursive: bool,
    ) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
}
