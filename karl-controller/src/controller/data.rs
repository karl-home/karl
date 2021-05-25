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

#[derive(Serialize, Deserialize, Default)]
pub struct GetDataResult {
    pub timestamps: Vec<String>,
    pub data: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
pub struct PushDataResult {
    pub modified_tag: String,
    pub timestamp: String,
}

impl DataSink {
    /// Creates a directory for the data sink relative to the `controller_path`.
    ///
    /// Generates the path to the persistent data sink, which is just
    /// a directory in the filesystem. Creates an empty directory at
    /// the path, `<controller_path>/data/` if it does not already exist.
    pub fn new(controller_path: PathBuf) -> Self {
        let data_path = controller_path.join("data");
        Self {
            data_path: data_path.to_path_buf(),
        }
    }

    /// Push sensor data.
    ///
    /// Parameters:
    /// - `entity_id`: the entity ID.
    /// - `tag`: output tag.
    /// - `data`: the data.
    ///
    /// Returns: modified tag, and timestamp.
    pub fn push_data(
        &self,
        tag: &str,
        data: &Vec<u8>,
    ) -> Result<PushDataResult, Error> {
        let path = self.data_path.join(tag);
        if !path.is_dir() {
            fs::create_dir_all(&path)?;
        }
        loop {
            let dt = chrono::prelude::Local::now().format("%+").to_string();
            let path = path.join(&dt);
            if path.exists() {
                continue;
            }
            debug!("push_data tag={} timestamp={} (len {})",
                tag, dt, data.len());
            fs::write(path, &data)?;
            break Ok(PushDataResult {
                modified_tag: tag.to_string(),
                timestamp: dt,
            })
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
        tag: &str,
        lower: String,
        upper: String,
    ) -> Result<GetDataResult, Error> {
        debug!("get_data tag={} {}", tag, lower);
        let path = self.data_path.join(&tag);
        if !path.is_dir() {
            fs::create_dir_all(&path)?;
        }
        let mut paths: Vec<_> = fs::read_dir(path)?.map(|r| r.unwrap()
            .path().as_path().file_name().unwrap()
            .to_str().unwrap()
            .to_string()
        ).collect();
        if paths.is_empty() {
            return Ok(GetDataResult { timestamps: vec![], data: vec![] });
        }

        paths.sort();
        let start_i = match paths.binary_search(&lower) {
            Ok(i) => i,
            Err(i) => i,
        };
        let end_i = match paths.binary_search(&upper) {
            Ok(i) => i,
            Err(i) => i,
        };
        let timestamps: Vec<String> = paths.drain(start_i..end_i+1).collect();
        let data: Vec<Vec<u8>> =
            timestamps.iter().map(|path| {
                let path = self.data_path.join(&tag).join(&path);
                fs::read(path).unwrap()
            }).collect();
        Ok(GetDataResult {
            timestamps,
            data,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;
    use chrono::Duration;
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
    }

    #[test]
    fn test_push_data_success() {
        let (_dir, sink) = init_test();
        let tag = "t1";
        let data = vec![1, 2, 3, 4];
        let tag_path = sink.data_path.join(tag);
        assert!(!tag_path.exists(), "tag path does not initially exist");

        let res = {
            let res = sink.push_data(tag, &data);
            assert!(res.is_ok());
            res.unwrap()
        };
        assert!(tag_path.exists(), "tag path was created");
        assert_eq!(&res.modified_tag, tag, "modified tag was the same");
        assert!(tag_path.join(&res.timestamp).exists(), "data was pushed");
        assert!(tag_path.join(&res.timestamp).is_file(), "data is a file");
    }

    #[test]
    fn test_push_multiple_data_all_written() {
        let (_dir, sink) = init_test();
        let tag = "t1";
        let n = 100;
        let timestamps = (0..n)
            .map(|i| {
                sink.push_data(tag, &vec![i])
            })
            .map(|res| {
                assert!(res.is_ok());
                res.unwrap()
            })
            .map(|res| {
                assert_eq!(&res.modified_tag, tag);
                res.timestamp
            })
            .map(|timestamp| {
                assert!(sink.data_path.join(tag).join(&timestamp).is_file());
                timestamp
            })
            .collect::<HashSet<_>>();
        assert_eq!(timestamps.len(), n as usize);
    }

    #[test]
    fn test_get_data_nothing_pushed() {
        let (_dir, sink) = init_test();
        let t2 = chrono::prelude::Local::now();
        let t1 = t2.checked_sub_signed(Duration::hours(1)).unwrap();
        let upper = t2.format("%+").to_string();
        let lower = t1.format("%+").to_string();
        let tag = "abc";
        let res = {
            let res = sink.get_data(tag, lower, upper);
            assert!(res.is_ok());
            res.unwrap()
        };
        assert_eq!(res.timestamps.len(), res.data.len());
        assert!(res.timestamps.is_empty());
        assert!(res.data.is_empty());
    }

    #[test]
    fn test_get_data_single_value() {
        let (_dir, sink) = init_test();
        let tag = "abc";
        let timestamp = sink.push_data(tag, &vec![1]).unwrap().timestamp;

        let res = {
            let res = sink.get_data(tag, timestamp.clone(), timestamp.clone());
            assert!(res.is_ok());
            res.unwrap()
        };
        assert_eq!(res.timestamps.len(), 1);
        assert_eq!(res.data.len(), 1);
        assert_eq!(res.timestamps[0], timestamp);
        assert_eq!(res.data, vec![vec![1]]);
    }

    #[test]
    fn test_get_data_timestamp_boundaries() {
        let (_dir, sink) = init_test();
        let tag = "abc";
        let t1 = sink.push_data(tag, &vec![1]).unwrap().timestamp;
        let t2 = sink.push_data(tag, &vec![2]).unwrap().timestamp;
        let t3 = sink.push_data(tag, &vec![3]).unwrap().timestamp;
        assert!(t1 != t2);
        assert!(t2 != t3);
        assert!(t1 != t3);

        let res = {
            let res = sink.get_data(tag, t1.clone(), t3.clone());
            assert!(res.is_ok());
            res.unwrap()
        };
        assert!(res.timestamps.contains(&t2), "contains middle timestamp");
        assert!(res.timestamps.contains(&t1), "contains lower timestamp");
        assert!(res.timestamps.contains(&t3), "contains upper timestamp");
        assert_eq!(res.timestamps.len(), 3);
        assert_eq!(res.timestamps, vec![t1, t2, t3], "sorted order");
        assert_eq!(res.data, vec![vec![1], vec![2], vec![3]], "sorted order");
    }

    #[test]
    fn test_get_data_randomized() {
        let (_dir, sink) = init_test();
        let tag = "t1";
        let n: u8 = 100;
        let timestamps = (0..n)
            .map(|i| sink.push_data(tag, &vec![i]).unwrap().timestamp)
            .collect::<Vec<_>>();
        assert_eq!(timestamps.len(), n as usize);

        let rounds = 10;
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let (lower_i, upper_i) = {
            let i = rng.gen_range(0..n);
            let j = rng.gen_range(0..n);
            (std::cmp::min(i, j), std::cmp::max(i, j))
        };
        for _ in 0..rounds {
            let expected = (upper_i - lower_i + 1) as usize;
            let lower = &timestamps[lower_i as usize];
            let upper = &timestamps[upper_i as usize];
            let res = sink.get_data(tag, lower.clone(), upper.clone());
            assert!(res.is_ok());
            let res = res.unwrap();
            assert_eq!(res.data.len(), expected);
            assert_eq!(res.data[0], vec![lower_i]);
            assert_eq!(res.data[expected - 1], vec![upper_i]);
            assert_eq!(res.timestamps.len(), expected);
            assert_eq!(&res.timestamps[0], lower);
            assert_eq!(&res.timestamps[expected - 1], upper);
        }
    }
}
