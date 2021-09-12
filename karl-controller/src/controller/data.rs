use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use chrono;
use serde::{Serialize, Deserialize};
use karl_common::*;

/// Persistent data sink.
///
/// Operations to the data sink must be authenticated in the above layer.
pub struct DataSink {
    pub data_path: PathBuf,
    pub tag_locks: Arc<RwLock<HashMap<Tag, Arc<RwLock<()>>>>>,
    last_dt: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct GetDataResult {
    pub timestamps: Vec<String>,
    pub data: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
pub struct PushDataResult {
    pub timestamp: String,
}

impl DataSink {
    /// Creates a directory for the data sink relative to the `controller_path`.
    ///
    /// Generates the path to the persistent data sink, which is just
    /// a directory in the filesystem. Removes the existing directory at
    /// the path, `<controller_path>/data/`, if it already exists.
    pub fn new(controller_path: PathBuf) -> Self {
        let data_path = controller_path.join("data");
        if data_path.exists() {
            warn!("removing existing data path! ({:?})", data_path);
            fs::remove_dir_all(&data_path).expect("error removing data path");
        }
        info!("initialized data store at {:?}", data_path);
        Self {
            data_path: data_path.to_path_buf(),
            tag_locks: Arc::new(RwLock::new(HashMap::new())),
            last_dt: String::new(),
        }
    }

    fn rwlock(&self, tag: &str) -> Result<Arc<RwLock<()>>, Error> {
        let path = self.data_path.join(tag);
        let tags = self.tag_locks.read().unwrap();
        if !path.exists() {
            drop(tags);
            let mut tags = self.tag_locks.write().unwrap();
            // check optimistic locking
            if path.exists() {
                assert!(tags.contains_key(tag));
                Ok(tags.get(tag).unwrap().clone())
            } else {
                assert!(!tags.contains_key(tag));
                let lock = Arc::new(RwLock::new(()));
                tags.insert(tag.to_string(), lock.clone());
                fs::create_dir_all(&path)?;
                Ok(lock)
            }
        } else {
            assert!(path.is_dir());
            Ok(tags.get(tag).unwrap().clone())
        }
    }

    /// Push sensor data.
    ///
    /// Parameters:
    /// - `entity_id`: the entity ID.
    /// - `tag`: output tag.
    /// - `data`: the data.
    ///
    /// Returns: timestamp.
    pub fn push_data(
        &mut self,
        tags: &Vec<String>,
        data: &Vec<u8>,
    ) -> Result<PushDataResult, Error> {
        if tags.is_empty() {
            return Err(Error::BadRequest);
        }
        loop {
            let dt = chrono::prelude::Local::now().format("%+").to_string();
            if dt == self.last_dt {
                continue;
            }

            #[cfg(target_os = "linux")]
            {
                let original_path = {
                    let tag = &tags[0];
                    let lock = self.rwlock(tag)?;
                    let _lock = lock.write().unwrap();
                    let path = self.data_path.join(tag).join(&dt);
                    debug!("push_data tag={} timestamp={} (len {})",
                        tag, dt, data.len());
                    fs::write(&path, &data)?;
                    path
                };
                // use symlinks to save space
                for i in 1..tags.len() {
                    let tag = &tags[i];
                    let lock = self.rwlock(tag)?;
                    let _lock = lock.write().unwrap();
                    let path = self.data_path.join(tag).join(&dt);
                    debug!("push_data tag={} timestamp={} (len {}) symlink",
                        tag, dt, data.len());
                    std::os::unix::fs::symlink(&original_path, &path)?;
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                for tag in tags {
                    let lock = self.rwlock(tag)?;
                    let _lock = lock.write().unwrap();
                    let path = self.data_path.join(tag).join(&dt);
                    debug!("push_data tag={} timestamp={} (len {})",
                        tag, dt, data.len());
                    fs::write(path, &data)?;
                }
            }

            self.last_dt = dt.clone();
            break Ok(PushDataResult {
                timestamp: dt,
            })
        }
    }

    /// Get data for the given tag.
    ///
    /// Parameters:
    /// - `tag`: data category.
    /// - `lower`: lower timestamp, inclusive.
    /// - `upper`: upper timestamp, inclusive.
    ///
    /// Returns: the timestamps and raw bytes of data for this tag within
    /// the given timestamp range.
    ///
    /// Errors if the tag does not exist or the expected path does not exist.
    pub fn get_data(
        &self,
        tag: &str,
        lower: String,
        upper: String,
    ) -> Result<GetDataResult, Error> {
        self.get_data_inner(tag, Some(lower), Some(upper))
    }

    /// Get data for the given tag.
    ///
    /// Parameters:
    /// - `tag`: data category.
    /// - `lower`: lower timestamp, inclusive. None if no lower bound.
    /// - `upper`: upper timestamp, inclusive. None if no upper bound.
    ///
    /// Returns: the timestamps and raw bytes of data for this tag within
    /// the given timestamp range.
    ///
    /// Errors if the tag does not exist or the expected path does not exist.
    pub fn get_data_inner(
        &self,
        tag: &str,
        lower: Option<String>,
        upper: Option<String>,
    ) -> Result<GetDataResult, Error> {
        debug!("get_data tag={} {:?}", tag, lower);
        let path = self.data_path.join(tag);
        let lock = self.rwlock(tag)?;
        let _lock = lock.read().unwrap();

        let mut paths: Vec<_> = fs::read_dir(path)?.map(|r| r.unwrap()
            .path().as_path().file_name().unwrap()
            .to_str().unwrap()
            .to_string()
        ).collect();
        if paths.is_empty() {
            return Ok(GetDataResult { timestamps: vec![], data: vec![] });
        }

        paths.sort();
        let start_i = if let Some(lower) = lower {
            match paths.binary_search(&lower) { Ok(i) => i, Err(i) => i }
        } else {
            0
        };
        let end_i = if let Some(upper) = upper {
            match paths.binary_search(&upper) { Ok(i) => i+1, Err(i) => i }
        } else {
            paths.len() - 1
        };
        let timestamps: Vec<String> = paths.drain(start_i..end_i).collect();
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

    /// List the tags in the data sink, in alphabetical order.
    pub fn list_tags(&self) -> Vec<String> {
        let mut tags: Vec<_> = self.tag_locks.read().unwrap().keys()
            .map(|tag| tag.clone())
            .collect();
        tags.sort();
        tags
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
        let (_dir, mut sink) = init_test();
        let tag = "t1";
        let data = vec![1, 2, 3, 4];
        let tag_path = sink.data_path.join(tag);
        assert!(!tag_path.exists(), "tag path does not initially exist");

        let res = {
            let res = sink.push_data(&vec![tag.to_string()], &data);
            assert!(res.is_ok());
            res.unwrap()
        };
        assert!(tag_path.exists(), "tag path was created");
        assert!(tag_path.join(&res.timestamp).exists(), "data was pushed");
        assert!(tag_path.join(&res.timestamp).is_file(), "data is a file");
    }

    #[test]
    fn test_push_multiple_data_all_written() {
        let (_dir, mut sink) = init_test();
        let tag = "t1";
        let n = 100;
        let data_path = sink.data_path.clone();
        let timestamps = (0..n)
            .map(|i| {
                sink.push_data(&vec![tag.to_string()], &vec![i])
            })
            .map(|res| {
                assert!(res.is_ok());
                res.unwrap()
            })
            .map(|res| {
                res.timestamp
            })
            .map(|timestamp| {
                assert!(data_path.join(tag).join(&timestamp).is_file());
                timestamp
            })
            .collect::<HashSet<_>>();
        assert_eq!(timestamps.len(), n as usize);
    }

    #[test]
    fn test_push_data_multiple_tags() {
        let (_dir, mut sink) = init_test();
        let data = vec![1, 2, 3, 4];
        let a = String::from("camera.a");
        let b = String::from("camera.b");
        let a_path = sink.data_path.join(&a);
        let b_path = sink.data_path.join(&b);
        assert!(!a_path.exists(), "tag path does not initially exist");
        assert!(!b_path.exists(), "tag path does not initially exist");

        let res = {
            let res = sink.push_data(&vec![a.clone(), b.clone()], &data);
            assert!(res.is_ok());
            res.unwrap()
        };
        assert!(a_path.exists(), "tag path was created");
        assert!(a_path.join(&res.timestamp).exists(), "data was pushed");
        assert!(a_path.join(&res.timestamp).is_file(), "data is a file");
        assert!(b_path.exists(), "tag path was created");
        assert!(b_path.join(&res.timestamp).exists(), "data was pushed");
        assert!(b_path.join(&res.timestamp).is_file(), "data is a file");
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
        let (_dir, mut sink) = init_test();
        let tag = "abc";
        let timestamp = sink.push_data(&vec![tag.to_string()], &vec![1]).unwrap().timestamp;

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
    fn test_get_data_single_value_not_exact_upper() {
        let (_dir, mut sink) = init_test();
        let tag = "abc";
        let timestamp = sink.push_data(&vec![tag.to_string()], &vec![1]).unwrap().timestamp;

        let res = {
            let res = sink.get_data(
                tag,
                timestamp.clone(),
                chrono::prelude::Local::now().format("%+").to_string(),
            );
            assert!(res.is_ok());
            res.unwrap()
        };
        assert_eq!(res.timestamps.len(), 1);
        assert_eq!(res.data.len(), 1);
        assert_eq!(res.timestamps[0], timestamp);
        assert_eq!(res.data, vec![vec![1]]);
    }

    #[test]
    fn test_get_data_single_value_not_exact_lower() {
        let (_dir, mut sink) = init_test();
        let tag = "abc";
        let timestamp = sink.push_data(&vec![tag.to_string()], &vec![1]).unwrap().timestamp;

        let res = {
            let res = sink.get_data(
                tag,
                String::from("2021-09-09T23:56:36.001788121-07:00"),
                timestamp.clone(),
            );
            assert!(res.is_ok());
            res.unwrap()
        };
        assert_eq!(res.timestamps.len(), 1);
        assert_eq!(res.data.len(), 1);
        assert_eq!(res.timestamps[0], timestamp);
        assert_eq!(res.data, vec![vec![1]]);
    }

    #[test]
    fn test_get_data_single_value_not_exact() {
        let (_dir, mut sink) = init_test();
        let tag = "abc";
        let timestamp = sink.push_data(&vec![tag.to_string()], &vec![1]).unwrap().timestamp;

        let res = {
            let res = sink.get_data(
                tag,
                String::from("2021-09-09T23:56:36.001788121-07:00"),
                chrono::prelude::Local::now().format("%+").to_string(),
            );
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
        let (_dir, mut sink) = init_test();
        let tag = "abc";
        let t1 = sink.push_data(&vec![tag.to_string()], &vec![1]).unwrap().timestamp;
        let t2 = sink.push_data(&vec![tag.to_string()], &vec![2]).unwrap().timestamp;
        let t3 = sink.push_data(&vec![tag.to_string()], &vec![3]).unwrap().timestamp;
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
        let (_dir, mut sink) = init_test();
        let tag = "t1";
        let n: u8 = 100;
        let timestamps = (0..n)
            .map(|i| sink.push_data(&vec![tag.to_string()], &vec![i]).unwrap().timestamp)
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
